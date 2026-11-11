//! Native product windows backed by application documents and workbenches.
//!
//! This module owns shell state, focus, command routing, terminals, and
//! transient interactions. Save, open, protected closure, tools, rendering, and
//! launch live in submodules.

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet},
    future::Future,
    path::Path,
    pin::Pin,
    sync::{Arc, OnceLock},
    time::Duration,
};

use gpui::{prelude::FluentBuilder, *};

mod closure;
pub(super) mod commands;
mod document_watch;
mod extension_restart;
mod key_hold;
mod key_routing;
mod language_mode;
mod launch;
mod navigation;
mod open;
pub(super) mod operations;
mod recovery_listing;
mod relocate;
mod render;
mod reopen;
mod roots;
mod save;
pub(super) mod sessions;
mod show_keybindings;
mod stripes;
mod tab_drag;
mod tool_picker;
mod tools;
mod view_state;

#[cfg(test)]
use super::commands::dispatch::SHOW_COMPLETIONS_COMMAND;
use closure::install_protected_window_close;
#[cfg(test)]
pub(super) use launch::bind_product_keys;
pub(super) use launch::finish_product_startup;
use launch::open_tab_tear_out_window;
#[cfg(any(test, feature = "test-support"))]
use launch::product_filesystems;
#[cfg(test)]
use launch::{dispatch_launch_request, open_tab_transfer_window};
pub(crate) use launch::{open_product_window, run};

use super::{
    CommandCompletion, CommandPalette,
    command_history::ApplicationCommandHistory,
    commands::dispatch::{
        ApplicationProductCommands, CLOSE_TAB_COMMAND, CLOSE_WINDOW_COMMAND, COPY_COMMAND,
        CUT_COMMAND, NEW_COMMAND, NEW_TERMINAL_COMMAND, NEW_WINDOW_COMMAND, OPEN_COMMAND,
        PASTE_COMMAND, ProductCommandDispatcher, ProductCommandSource, ProductCommandTarget,
        QUIT_COMMAND, REDO_COMMAND, RESET_ZOOM_COMMAND, SAVE_AS_COMMAND, SAVE_COMMAND,
        SELECT_ALL_COMMAND, SHOW_EXTENSION_REPORT_COMMAND, SPLIT_HORIZONTAL_COMMAND,
        SPLIT_VERTICAL_COMMAND, TargetTab, UNDO_COMMAND, ZOOM_IN_COMMAND, ZOOM_OUT_COMMAND,
        dispatch_close_to_captured_target, dispatch_open_to_captured_target,
        dispatch_save_to_captured_target,
    },
    commands::keymaps::ApplicationKeymaps,
    documents::{ApplicationDocuments, DocumentCollection, DocumentId, DocumentState},
    entry::OpenRequest,
    extensions::host::handlers::TransientError,
    filesystem::{
        FileSystemProviderRegistry, LocalFileSystemProvider, ResourceError, ResourceKind,
        ResourceStat, ResourceVersion,
    },
    model::{Buffer, BufferModel},
    open::{OpenResource, enumerate_directory, load_resource},
    picker::{Picker, PickerEntry, PickerEvent},
    terminal::session::TerminalSession,
    terminal::view::TerminalView,
    transient::{
        InputEvent, InputView, PromptText, TransientOutcome, TransientOwner, TransientSurface,
    },
    workbench::{
        Area, CloseRequestOutcome, DockLocation, DocumentCloseDisposition, PaneId, SplitDirection,
        SplitPlacement, TabId, TabSurfaceId, TerminalSessionId, ToolContent, ToolFrameId,
        Workbench, WorkbenchLayout,
    },
    workspace::WorkspaceState,
    workspace_tree::{
        WorkspaceTree, WorkspaceTreeEvent, WorkspaceTreeRequest, WorkspaceTreeResponse,
    },
};

/// Every open product session. A session opens when it is created and closes
/// with its window. The registry answers which workbench and window a session
/// has without reading the session, which may be updating.
#[derive(Default)]
struct ApplicationSessions(RefCell<Vec<OpenSession>>);

struct OpenSession {
    shell: WeakEntity<ProductShell>,
    workbench: WeakEntity<Workbench>,
    window: Option<AnyWindowHandle>,
    /// Whether this session was activated last, which makes it the active
    /// session while it has no window.
    active: bool,
}

impl Global for ApplicationSessions {}

struct ApplicationProtectedClosure(RefCell<bool>);

impl Global for ApplicationProtectedClosure {}

struct ApplicationTerminalSessions(RefCell<TerminalSessions>);

impl Global for ApplicationTerminalSessions {}

struct ApplicationLaunchGate(RefCell<LaunchGate>);

impl Global for ApplicationLaunchGate {}

enum LaunchGate {
    Pending(Vec<Option<OpenRequest>>),
    Ready,
    Failed,
}

impl LaunchGate {
    fn request(&mut self, request: Option<OpenRequest>) -> Option<Option<OpenRequest>> {
        match self {
            Self::Pending(requests) => {
                requests.push(request);
                None
            }
            Self::Ready => Some(request),
            Self::Failed => None,
        }
    }

    /// The queued requests; `None` entries are reopen requests without a
    /// resource.
    fn complete(&mut self) -> Vec<Option<OpenRequest>> {
        match std::mem::replace(self, Self::Ready) {
            Self::Pending(requests) => requests,
            Self::Ready => Vec::new(),
            Self::Failed => {
                *self = Self::Failed;
                Vec::new()
            }
        }
    }

    fn fail(&mut self) {
        *self = Self::Failed;
    }
}

#[derive(Default)]
struct TerminalSessions {
    next_id: u64,
    sessions: HashMap<TerminalSessionId, Entity<TerminalSession>>,
}

impl TerminalSessions {
    fn allocate_id(&mut self) -> TerminalSessionId {
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("terminal identity exhausted");
        TerminalSessionId(self.next_id)
    }

    fn remove(&mut self, id: TerminalSessionId) -> Option<Entity<TerminalSession>> {
        self.sessions.remove(&id)
    }
}

pub(crate) use knot_model::workbench::closure::CloseKind as ProtectedCloseKind;
use knot_model::workbench::closure::{
    Approval as CloseApproval, ApprovalKind as CloseApprovalKind, ViewKey as ProtectedViewKey,
};

#[derive(Clone)]
struct ProtectedCloseRequest {
    kind: ProtectedCloseKind,
    origin: ProductCommandTarget,
    caller: Option<operations::OperationCaller>,
    /// Whether dirty documents losing their final view close without asking,
    /// their changes discarded once the close commits.
    discard: bool,
}

#[derive(Clone)]
struct ProtectedView {
    key: ProtectedViewKey,
    pinned: bool,
    /// The area of the view's pane.
    area: Area,
    target: Option<ProductCommandTarget>,
}

struct ProtectedClosePlan {
    request: ProtectedCloseRequest,
    scope: Vec<ProtectedViewKey>,
    prompts: Vec<(DocumentId, ProductCommandTarget)>,
    /// Discards approved without asking, at the revision the plan saw.
    discarded: Vec<CloseApproval>,
    /// Quit keeps dirty documents in durable recovery instead of asking about them.
    hot_exit: bool,
}

#[derive(Debug)]
enum ProtectedCloseFailure {
    InvalidTarget,
    InProgress,
    Cancelled,
    PersistenceFailed { message: String },
}

impl ProtectedCloseFailure {
    fn command_outcome(self) -> knot_protocol::CommandOutcome {
        use knot_protocol::CommandOutcome;
        match self {
            Self::InvalidTarget => CommandOutcome::InvalidTarget,
            Self::InProgress => CommandOutcome::Unavailable,
            Self::Cancelled => CommandOutcome::Cancelled,
            Self::PersistenceFailed { message } => CommandOutcome::HandlerFailure { message },
        }
    }
}

type ProtectedCloseCompletion = Box<
    dyn FnOnce(Result<knot_protocol::PresentationCloseOutcome, ProtectedCloseFailure>, &mut App),
>;

struct ApplicationFileSystems(Arc<FileSystemProviderRegistry>);

impl Global for ApplicationFileSystems {}

#[derive(Clone)]
enum OpenDialogOutcome {
    Selected(std::path::PathBuf),
    Cancelled,
    Failed(String),
}

type OpenDialogFuture = Pin<Box<dyn Future<Output = OpenDialogOutcome> + Send + 'static>>;

trait ProductOpenDialog: Send + Sync {
    fn select(&self) -> OpenDialogFuture;
}

struct ApplicationOpenDialog(Arc<dyn ProductOpenDialog>);

impl Global for ApplicationOpenDialog {}

#[derive(Clone)]
enum SaveDialogOutcome {
    Selected(super::resource::ResourceUri),
    Cancelled,
    Failed(String),
}

type SaveDialogFuture = Pin<Box<dyn Future<Output = SaveDialogOutcome> + Send + 'static>>;

trait ProductSaveDialog: Send + Sync {
    fn select(&self, suggested_name: String) -> SaveDialogFuture;
}

struct ApplicationSaveDialog(Arc<dyn ProductSaveDialog>);

impl Global for ApplicationSaveDialog {}

impl ApplicationSessions {
    /// Open `shell`, or record the workbench an open one now shows.
    fn register(&self, shell: WeakEntity<ProductShell>, workbench: &Entity<Workbench>) {
        let mut sessions = self.0.borrow_mut();
        match sessions.iter_mut().find(|open| open.shell == shell) {
            Some(open) => open.workbench = workbench.downgrade(),
            None => sessions.push(OpenSession {
                shell,
                workbench: workbench.downgrade(),
                window: None,
                active: false,
            }),
        }
    }

    /// Record the window an open session is shown in.
    fn attach(&self, shell: EntityId, window: AnyWindowHandle) {
        if let Some(open) = self
            .0
            .borrow_mut()
            .iter_mut()
            .find(|open| open.shell.entity_id() == shell)
        {
            open.window = Some(window);
        }
    }

    /// Mark `shell` as the session activated last.
    #[cfg(any(test, feature = "test-support"))]
    fn activate(&self, shell: EntityId) {
        for open in self.0.borrow_mut().iter_mut() {
            open.active = open.shell.entity_id() == shell;
        }
    }

    /// The activated session, when it has no window.
    fn active_without_window(&self) -> Option<Entity<ProductShell>> {
        self.0
            .borrow()
            .iter()
            .find(|open| open.active && open.window.is_none())
            .and_then(|open| open.shell.upgrade())
    }

    fn close(&self, shell: EntityId) {
        self.0
            .borrow_mut()
            .retain(|open| open.shell.entity_id() != shell);
    }

    fn is_open(&self, shell: EntityId) -> bool {
        self.0
            .borrow()
            .iter()
            .any(|open| open.shell.entity_id() == shell)
    }

    /// The open sessions, in the order they opened.
    fn shells(&self) -> Vec<Entity<ProductShell>> {
        let mut sessions = self.0.borrow_mut();
        sessions.retain(|open| open.shell.upgrade().is_some());
        sessions
            .iter()
            .filter_map(|open| open.shell.upgrade())
            .collect()
    }

    /// The open session with the given public ID.
    fn find(&self, id: &str) -> Option<Entity<ProductShell>> {
        self.shells()
            .into_iter()
            .find(|shell| session_id(shell.entity_id()) == id)
    }

    /// The window an open session is shown in.
    fn window(&self, shell: EntityId) -> Option<AnyWindowHandle> {
        self.0
            .borrow()
            .iter()
            .find(|open| open.shell.entity_id() == shell)
            .and_then(|open| open.window)
    }

    /// The open session shown in `window`.
    fn in_window(&self, window: AnyWindowHandle) -> Option<Entity<ProductShell>> {
        self.0
            .borrow()
            .iter()
            .find(|open| open.window == Some(window))
            .and_then(|open| open.shell.upgrade())
    }

    /// The open session showing `workbench`.
    fn session_for(&self, workbench: &Entity<Workbench>) -> Option<WeakEntity<ProductShell>> {
        self.0.borrow().iter().find_map(|open| {
            (open.workbench.upgrade().as_ref() == Some(workbench)).then(|| open.shell.clone())
        })
    }

    fn view_count(&self, document: DocumentId, cx: &App) -> usize {
        let mut sessions = self.0.borrow_mut();
        sessions
            .retain(|open| open.shell.upgrade().is_some() && open.workbench.upgrade().is_some());
        sessions
            .iter()
            .filter_map(|open| open.workbench.upgrade())
            .map(|workbench| workbench.read(cx).view_count(document))
            .sum()
    }
}

/// The public ID of the session `shell`: `WindowTarget.window` names it.
pub(crate) fn session_id(shell: EntityId) -> String {
    shell.as_u64().to_string()
}

/// Whether `shell` is open and shows `workbench`.
pub(super) fn session_is_live(
    shell: &WeakEntity<ProductShell>,
    workbench: &Entity<Workbench>,
    cx: &App,
) -> bool {
    cx.global::<ApplicationSessions>()
        .session_for(workbench)
        .is_some_and(|open| open == *shell)
}

/// The open product sessions, in the order they opened.
pub(crate) fn sessions(cx: &App) -> Vec<Entity<ProductShell>> {
    cx.try_global::<ApplicationSessions>()
        .map(ApplicationSessions::shells)
        .unwrap_or_default()
}

/// The open session a public ID names.
pub(crate) fn find_session(id: &str, cx: &App) -> Option<Entity<ProductShell>> {
    cx.try_global::<ApplicationSessions>()?.find(id)
}

/// The session shown in `window`.
pub(crate) fn session_in_window(window: AnyWindowHandle, cx: &App) -> Option<Entity<ProductShell>> {
    cx.try_global::<ApplicationSessions>()?.in_window(window)
}

/// The session shown in the active window, or without an active window the
/// session activated last when it has no window.
pub(crate) fn active_session(cx: &App) -> Option<Entity<ProductShell>> {
    match cx.active_window() {
        Some(window) => session_in_window(window, cx),
        None => cx
            .try_global::<ApplicationSessions>()?
            .active_without_window(),
    }
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static TEST_SESSION_IDS: RefCell<HashMap<u64, String>> = RefCell::default();
}

/// The public ID of the session last shown in `window`.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn test_session_id(window: AnyWindowHandle) -> String {
    TEST_SESSION_IDS.with_borrow(|ids| ids[&window.window_id().as_u64()].clone())
}

/// The window an open session is shown in, without reading the session.
pub(crate) fn session_window(shell: EntityId, cx: &App) -> Option<AnyWindowHandle> {
    cx.global::<ApplicationSessions>().window(shell)
}

/// Where an open lands: beside the captured tab, or in the center when the
/// open was captured without one.
#[derive(Clone)]
struct OpenTarget {
    shell: WeakEntity<ProductShell>,
    workbench: WeakEntity<Workbench>,
    tab: Option<TargetTab>,
}

impl OpenTarget {
    fn from_command(target: &ProductCommandTarget) -> Self {
        Self {
            shell: target.shell.clone(),
            workbench: target.workbench.clone(),
            tab: target.tab,
        }
    }

    /// A target at a live tab of `workbench`.
    fn at(
        shell: WeakEntity<ProductShell>,
        workbench: &Entity<Workbench>,
        pane: PaneId,
        tab: TabId,
        cx: &App,
    ) -> Self {
        let capture = workbench
            .read(cx)
            .capture_tab_move(pane, tab)
            .expect("open targets name live tabs");
        Self {
            shell,
            workbench: workbench.downgrade(),
            tab: Some(TargetTab {
                pane,
                tab,
                surface: capture.tab.surface,
                presentation: capture.presentation,
            }),
        }
    }

    fn matches_presentation(&self, workbench: &Entity<Workbench>, cx: &App) -> bool {
        self.workbench.upgrade().as_ref() == Some(workbench)
            && self.tab.is_none_or(|tab| tab.is_live(workbench.read(cx)))
    }
}

/// State committed by resource Open, independent of command dispatch outcomes.
#[derive(Clone)]
enum OpenCommit {
    Document {
        document: DocumentId,
        target: OpenTarget,
    },
    Workspace(
        #[cfg_attr(
            not(test),
            allow(dead_code, reason = "tests inspect the installed workspace")
        )]
        super::workspace::WorkspaceSnapshot,
    ),
}

#[derive(Debug)]
enum OpenFailure {
    InvalidTarget,
    Cancelled,
    Resource(super::open::OpenResourceError),
}

impl OpenFailure {
    fn command_outcome(self) -> knot_protocol::CommandOutcome {
        use knot_protocol::CommandOutcome;
        match self {
            Self::InvalidTarget => CommandOutcome::InvalidTarget,
            Self::Cancelled => CommandOutcome::Cancelled,
            Self::Resource(error) => CommandOutcome::HandlerFailure {
                message: error.to_string(),
            },
        }
    }
}

fn target_document(target: &ProductCommandTarget) -> Option<DocumentId> {
    match target.surface()? {
        TabSurfaceId::Document(document) => Some(document),
        TabSurfaceId::Terminal(_) | TabSurfaceId::Tool(_) => None,
    }
}

pub(crate) struct ProductShell {
    workbench: Entity<Workbench>,
    window: Option<AnyWindowHandle>,
    tab_drag: std::rc::Rc<RefCell<tab_drag::TabDragState>>,
    dock_resize: Option<DockResizeDrag>,
    /// The open right-click menu of a tool stripe button.
    stripe_menu: Option<stripes::StripeMenu>,
    split_resize: Option<SplitResizeDrag>,
    recent_walk: Option<knot_model::workbench::RecentWalk>,
    /// The pane rendered over the whole workbench, while it stays focused.
    maximized: Option<PaneId>,
    /// Per dock, the pane that was maximized when the dock was hidden;
    /// showing the dock again maximizes it.
    hidden_maximized: [Option<PaneId>; 3],
    /// Closed tabs Reopen Closed Tab recreates, most recent last.
    closed_tabs: knot_model::workbench::RecentlyClosed<reopen::ClosedTab>,
    command_palette: Option<CommandPalette<ProductCommandTarget>>,
    command_palette_subscription: Option<Subscription>,
    transient: Option<ActiveTransient>,
    workspace: WorkspaceState,
    /// Keeps the file index of each workspace root alive.
    file_index_leases: Vec<super::file_index::FileIndexLease>,
    /// The document of the last focused document tab, which new terminals
    /// follow while a terminal or tool has focus.
    last_focused_document: Option<DocumentId>,
    /// The last focused document tab, which the status bar keeps describing
    /// while its tab stays visible and focus is elsewhere.
    last_focused_editor_tab: Option<super::workbench::TabId>,
    application_observer: Option<Subscription>,
    application_activation: Option<Subscription>,
    _appearance_observer: Option<Subscription>,
    /// Repaints the status bar when its items or settings change.
    _status_bar_observers: Vec<Subscription>,
    application_focus: Vec<(FocusHandle, Subscription, Subscription)>,
    workspace_tree: Option<Entity<WorkspaceTree>>,
    workspace_tree_subscription: Option<Subscription>,
    contributed_views: BTreeMap<String, ContributedView>,
    _contributed_view_release: Option<Subscription>,
    _window_closed: Option<Subscription>,
    _extension_host_release: Option<Subscription>,
    open_generation: u64,
    tasks: Vec<Task<()>>,
    extension_report_open: bool,
    extension_report_subscription: Option<Subscription>,
    view_state: view_state::ViewStateTracking,
    /// Creation order across product windows; window identities are reused.
    created: u64,
    /// Input waiting for a key-invoked extension command to finish.
    key_hold: Option<key_hold::KeyHold>,
    key_hold_generation: u64,
    keys: key_routing::KeyRouting,
    /// What has Knot-side focus. The attached window focuses its handle and
    /// reports focus a click moves.
    focus: Focus,
    /// The empty center's handle, which takes keyboard input while no pane
    /// has focus.
    empty_focus: FocusHandle,
}

/// What has Knot-side focus, by identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FocusTarget {
    layer: Focus,
    pane: Option<PaneId>,
    tab: Option<TabId>,
}

/// The layer of a session that has focus: the focused pane's active
/// surface, the command palette, or the transient interaction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Focus {
    #[default]
    Workbench,
    Palette,
    Transient,
}

/// One live transient interaction owned by this window. Native interactions
/// have no extension owner and are never cancelled by extension lifecycles.
struct ActiveTransient {
    owner: Option<TransientOwner>,
    surface: Option<TransientSurface>,
    focus: Option<FocusHandle>,
    completion: Option<tokio::sync::oneshot::Sender<TransientOutcome>>,
    restore_focus: Focus,
    _subscription: Option<Subscription>,
}

impl ActiveTransient {
    fn render(&self) -> Option<AnyElement> {
        self.surface.as_ref().map(|surface| match surface {
            TransientSurface::Picker(view) => view.clone().into_any_element(),
            TransientSurface::Input(view) => view.clone().into_any_element(),
        })
    }

    fn settle(&mut self, outcome: TransientOutcome) {
        if let Some(completion) = self.completion.take() {
            let _ = completion.send(outcome);
        }
    }
}

struct ContributedView {
    content: super::workbench::ExtensionView,
    _event: Option<Subscription>,
    _observer: Subscription,
}

impl ContributedView {
    fn instance_id(&self, cx: &App) -> knot_protocol::ViewId {
        self.content.instance_id(cx)
    }

    #[cfg(any(test, feature = "test-support"))]
    fn tree(&self) -> &Entity<super::tree_view::TreeView> {
        match &self.content {
            super::workbench::ExtensionView::Tree(view) => view,
            super::workbench::ExtensionView::Text(_) => {
                panic!("contributed view is not a tree")
            }
        }
    }
}

#[derive(Clone, Copy)]
struct DockResizeDrag {
    dock: DockLocation,
    start_pointer: Point<Pixels>,
    start_size: f32,
}

/// An ordinary divider being dragged: its split, identified by its boundary
/// pane, and the split's painted extent along its direction.
#[derive(Clone, Copy)]
struct SplitResizeDrag {
    boundary: PaneId,
    horizontal: bool,
    start_pointer: Point<Pixels>,
    start_ratio: f32,
    extent: f32,
}

/// The smallest extent, in logical pixels, a divider drag leaves either side.
const MIN_DRAGGED_PANE_SIZE: f32 = 120.;

impl ProductShell {
    /// Open a session showing `workbench`, without a window.
    fn new(workbench: Entity<Workbench>, cx: &mut Context<Self>) -> Self {
        cx.global::<ApplicationSessions>()
            .register(cx.weak_entity(), &workbench);
        document_watch::install(cx);
        Self {
            workbench,
            window: None,
            tab_drag: Default::default(),
            dock_resize: None,
            stripe_menu: None,
            split_resize: None,
            recent_walk: None,
            maximized: None,
            hidden_maximized: [None; 3],
            closed_tabs: knot_model::workbench::RecentlyClosed::new(reopen::CLOSED_TAB_LIMIT),
            command_palette: None,
            command_palette_subscription: None,
            transient: None,
            workspace: WorkspaceState::default(),
            file_index_leases: Vec::new(),
            last_focused_document: None,
            last_focused_editor_tab: None,
            application_observer: None,
            application_activation: None,
            _appearance_observer: None,
            _status_bar_observers: Vec::new(),
            application_focus: Vec::new(),
            workspace_tree: None,
            workspace_tree_subscription: None,
            contributed_views: BTreeMap::new(),
            _contributed_view_release: None,
            _window_closed: None,
            _extension_host_release: None,
            open_generation: 0,
            tasks: Vec::new(),
            extension_report_open: false,
            extension_report_subscription: None,
            view_state: Default::default(),
            created: {
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            },
            key_hold: None,
            key_hold_generation: 0,
            keys: Default::default(),
            focus: Focus::default(),
            empty_focus: cx.focus_handle(),
        }
    }

    /// Bring the session's window to the front once the current update ends.
    fn activate_window(&self, cx: &mut App) {
        let Some(window) = self.window else {
            return;
        };
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, _| window.activate_window());
        });
    }

    /// The modifier keys held in the session's window; none without one.
    fn modifiers(&self, cx: &mut App) -> Modifiers {
        self.window
            .and_then(|window| window.update(cx, |_, window, _| window.modifiers()).ok())
            .unwrap_or_default()
    }

    /// Ask with a native prompt in the session's window once the current
    /// update ends, so a caller may hold the session or the window.
    pub(super) fn prompt(
        &self,
        level: PromptLevel,
        title: impl Into<String>,
        detail: Option<String>,
        buttons: Vec<PromptButton>,
        cx: &mut App,
    ) -> PromptAnswer {
        prompt(self.window, level, title.into(), detail, buttons, cx)
    }

    /// Orders product windows by creation.
    pub(crate) fn creation_order(&self) -> u64 {
        self.created
    }

    /// Show the session in `window`, which focuses what the session has
    /// focused. Closing the window ends the session: its live frames and any
    /// pending transient interaction go, while other sessions keep theirs.
    pub(super) fn attach_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.window.is_some() {
            return;
        }
        let handle = window.window_handle();
        self.window = Some(handle);
        cx.global::<ApplicationSessions>()
            .attach(cx.entity_id(), handle);
        #[cfg(any(test, feature = "test-support"))]
        TEST_SESSION_IDS.with_borrow_mut(|ids| {
            ids.insert(handle.window_id().as_u64(), session_id(cx.entity_id()))
        });
        let shell = cx.weak_entity();
        self._window_closed = Some(cx.on_window_closed(move |cx, _| {
            let shell = shell.clone();
            cx.defer(move |cx| {
                let Some(shell) = shell.upgrade() else { return };
                if cx.windows().contains(&handle) {
                    return;
                }
                shell.update(cx, |this, cx| this.end(cx));
            });
        }));
        self.follow_focus(cx);
    }

    /// Close the session without its window.
    fn end(&mut self, cx: &mut Context<Self>) {
        cx.global::<ApplicationSessions>().close(cx.entity_id());
        let ids = self.contributed_views.keys().cloned().collect::<Vec<_>>();
        for id in ids {
            self.detach_contributed_view(&id, cx);
        }
        self.finish_transient(TransientOutcome::Cancelled, cx);
    }

    /// Close the session's window, which ends the session, or end a session
    /// without one.
    fn close_window(&mut self, cx: &mut Context<Self>) -> Result<(), ProtectedCloseFailure> {
        let Some(window) = self.window else {
            self.end(cx);
            return Ok(());
        };
        cx.update_window(window, |_, window, cx| {
            tab_drag::cancel_source_window(window, cx);
            window.remove_window();
        })
        .map_err(|_| ProtectedCloseFailure::InvalidTarget)
    }

    /// Show the session in `native_window` and report it to extensions.
    fn observe_extension_host(&mut self, native_window: &mut Window, cx: &mut Context<Self>) {
        self.attach_window(native_window, cx);
        self.observe_window_bounds(native_window, cx);
        self.application_activation = Some(cx.observe_window_activation(
            native_window,
            |_, window, cx| {
                super::extensions::host::application_changed(cx);
                if window.is_window_active() {
                    document_watch::recheck_all(cx);
                }
            },
        ));
        self._appearance_observer = Some(
            native_window.observe_window_appearance(|_, cx| super::themes::selection_changed(cx)),
        );
        // The bar and settings change only what the window paints, so they
        // redraw it without notifying the session, which extensions observe.
        self._status_bar_observers = vec![
            cx.observe_global_in::<super::status_bar::ApplicationStatusBar>(
                native_window,
                |_, window, _| window.refresh(),
            ),
            cx.observe_global_in::<super::settings::ApplicationSettings>(
                native_window,
                |_, window, _| window.refresh(),
            ),
        ];
        self.observe_application(cx);
    }

    /// Report the session to extensions as it changes and show their
    /// contributed frames, which needs no window.
    pub(crate) fn observe_application(&mut self, cx: &mut Context<Self>) {
        self.application_observer = Some(cx.observe(&cx.entity(), |this, _, cx| {
            super::extensions::host::application_changed(cx);
            this.schedule_view_state(cx);
            this.prompt_external_change(cx);
        }));
        super::extensions::host::application_changed(cx);
        if let Some(host) = super::extensions::host::entity(cx) {
            self.extension_report_subscription = Some(cx.observe(&host, |this, _, cx| {
                this.sync_contributed_views(cx);
                cx.notify();
            }));
            self._contributed_view_release = Some(cx.on_release(|this, cx| {
                for (_, instance) in std::mem::take(&mut this.contributed_views) {
                    super::extensions::host::detach_view_instance(instance.instance_id(cx), cx);
                }
            }));
            // Host notifications reconcile definitions while it lives; release
            // removes every remaining frame without waiting for another event.
            self._extension_host_release = Some(cx.observe_release(&host, |this, _, cx| {
                // The host is leased and losing ownership, so drop the frames
                // without calling back into it.
                let frames = std::mem::take(&mut this.contributed_views);
                for (id, _instance) in frames {
                    if let Some(frame) = this.workbench.read(cx).tool_by_stable_id(&id) {
                        this.workbench
                            .update(cx, |workbench, _| workbench.unregister_tool(frame));
                    }
                }
            }));
            self.sync_contributed_views(cx);
        }
    }

    /// Make this session the active one while no window is active, as
    /// focusing its window would.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn activate(&mut self, cx: &mut Context<Self>) {
        cx.global::<ApplicationSessions>().activate(cx.entity_id());
        super::extensions::host::application_changed(cx);
    }

    fn observe_application_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handles = self
            .workbench
            .read(cx)
            .panes()
            .iter()
            .flat_map(|pane| pane.tabs())
            .filter_map(|tab| {
                self.workbench
                    .read(cx)
                    .command_view(tab)
                    .map(|view| view.focus_handle(cx))
            })
            .collect::<Vec<_>>();
        self.application_focus
            .retain(|(handle, _, _)| handles.contains(handle));
        for handle in handles {
            if self
                .application_focus
                .iter()
                .any(|(current, _, _)| current == &handle)
            {
                continue;
            }
            // A view focused before its subscription exists sends no event.
            if handle.is_focused(window) {
                self.focus_view(&handle, cx);
            }
            self.application_focus.push((
                handle.clone(),
                cx.on_focus(&handle, window, {
                    let handle = handle.clone();
                    move |this, _, cx| {
                        this.focus_view(&handle, cx);
                        super::extensions::host::application_changed(cx);
                        this.schedule_view_state(cx);
                    }
                }),
                cx.on_blur(&handle, window, |_, _, cx| {
                    super::extensions::host::application_changed(cx)
                }),
            ));
        }
        self.observe_editor_view_state(cx);
    }

    /// Focus the surface whose view owns `handle`. The window reports focus
    /// a click gives a view the same way.
    pub(crate) fn focus_view(&mut self, handle: &FocusHandle, cx: &mut Context<Self>) {
        let workbench = self.workbench.read(cx);
        let Some((pane, tab, document)) = workbench.panes().iter().find_map(|pane| {
            pane.tabs()
                .iter()
                .find(|tab| {
                    workbench
                        .command_view(tab)
                        .is_some_and(|view| &view.focus_handle(cx) == handle)
                })
                .map(|tab| (pane.id(), tab.id(), tab.document_id()))
        }) else {
            return;
        };
        self.focus = Focus::Workbench;
        self.workbench.update(cx, |workbench, _| {
            workbench.focus_pane(pane);
        });
        if document.is_some() {
            self.last_focused_document = document;
            self.last_focused_editor_tab = Some(tab);
        }
        self.follow_focus(cx);
    }

    /// The directory a new terminal in this window starts in: the focused
    /// terminal's current directory, otherwise the window's default.
    fn terminal_directory(&self, cx: &App) -> Option<std::path::PathBuf> {
        let focused = self
            .workbench
            .read(cx)
            .focused_pane()
            .map(|pane| pane.active_tab().surface_id());
        match focused {
            Some(TabSurfaceId::Terminal(id)) => Self::terminal_session_directory(id, cx),
            _ => None,
        }
        .or_else(|| self.default_terminal_directory(cx))
    }

    fn terminal_session_directory(id: TerminalSessionId, cx: &App) -> Option<std::path::PathBuf> {
        let sessions = cx.global::<ApplicationTerminalSessions>().0.borrow();
        sessions
            .sessions
            .get(&id)
            .and_then(|session| session.read(cx).current_directory())
    }

    /// The window's default terminal directory: the root of its most recently
    /// focused document, otherwise its first root, otherwise home.
    fn default_terminal_directory(&self, cx: &App) -> Option<std::path::PathBuf> {
        let document = self
            .workbench
            .read(cx)
            .focused_pane()
            .and_then(|pane| pane.active_tab().document_id())
            .or(self.last_focused_document);
        let documents = Self::documents(cx);
        let uri = document
            .and_then(|document| documents.read(cx).get(document))
            .and_then(|document| document.resource_uri());
        let roots = self.workspace.snapshot();
        let roots = roots
            .roots()
            .iter()
            .map(|root| root.uri.clone())
            .collect::<Vec<_>>();
        super::terminal::directory::working_directory(
            &roots,
            uri,
            directories::BaseDirs::new().map(|base| base.home_dir().to_owned()),
        )
    }

    pub(crate) fn application_window(
        &self,
        session: EntityId,
        buffers: &std::collections::HashMap<DocumentId, knot_protocol::BufferHandle>,
        cx: &App,
    ) -> knot_protocol::WindowInfo {
        use knot_protocol::{
            PaneInfo, PaneTarget, PresentationInfo, PresentationTarget, SurfaceKind, WindowInfo,
            WindowTarget,
        };
        let target = WindowTarget {
            window: session_id(session),
        };
        let workbench = self.workbench.read(cx);
        let documents = cx.global::<ApplicationDocuments>().0.read(cx);
        let focused = workbench
            .focused_pane()
            .filter(|_| self.focus == Focus::Workbench)
            .map(|pane| (pane.id(), pane.active_tab_id()));
        let mut current_target = None;
        let mut current_kind = None;
        let mut last_editor_target = None;
        let panes = workbench
            .panes()
            .iter()
            .map(|pane| {
                let pane_target = PaneTarget {
                    window: target.clone(),
                    workbench: self.workbench.entity_id().as_u64().to_string(),
                    pane: pane.id().value().to_string(),
                };
                let tabs = pane
                    .tabs()
                    .iter()
                    .map(|tab| {
                        let presentation = tab.presentation().value().to_string();
                        let view_target = PresentationTarget {
                            pane: pane_target.clone(),
                            tab: tab.id().value().to_string(),
                            presentation,
                        };
                        if focused == Some((pane.id(), tab.id())) {
                            current_target = Some(view_target.clone());
                            current_kind = Some(tab.surface_id());
                        }
                        if self.last_focused_editor_tab == Some(tab.id())
                            && tab.document_id().is_some()
                            && tab.id() == pane.active_tab_id()
                        {
                            last_editor_target = Some(view_target.clone());
                        }
                        let document = tab.document_id();
                        PresentationInfo {
                            target: view_target,
                            kind: match tab.surface_id() {
                                TabSurfaceId::Document(_) => SurfaceKind::Document,
                                TabSurfaceId::Terminal(_) => SurfaceKind::Terminal,
                                TabSurfaceId::Tool(_) => SurfaceKind::Tool,
                            },
                            document: document
                                .and_then(|id| documents.get(id))
                                .map(|document| document.identity().to_string()),
                            buffer: document.and_then(|id| buffers.get(&id).copied()),
                            tool: match tab.surface_id() {
                                TabSurfaceId::Tool(id) => {
                                    workbench.tool(id).map(|tool| tool.stable_id.clone())
                                }
                                TabSurfaceId::Document(_) | TabSurfaceId::Terminal(_) => None,
                            },
                        }
                    })
                    .collect::<Vec<_>>();
                let selected = tabs
                    .iter()
                    .find(|tab| tab.target.tab == pane.active_tab_id().value().to_string())
                    .unwrap()
                    .target
                    .clone();
                PaneInfo {
                    target: pane_target,
                    tabs,
                    selected,
                }
            })
            .collect();
        // The focused editor, else the last focused editor while its tab is
        // shown; a focused terminal describes no editor.
        let status_target = match current_kind {
            Some(TabSurfaceId::Document(_)) => current_target.clone(),
            Some(TabSurfaceId::Terminal(_)) => None,
            _ => last_editor_target,
        };
        let workspace = self.workspace.snapshot();
        WindowInfo {
            target: target.clone(),
            panes,
            current_target,
            status_target,
            workspace: operations::workspace_info(target, &workspace),
        }
    }

    fn command_dispatcher(cx: &App) -> Entity<ProductCommandDispatcher> {
        cx.global::<ApplicationProductCommands>().0.clone()
    }

    fn create_terminal(
        &mut self,
        cx: &mut Context<Self>,
    ) -> (TerminalSessionId, Entity<TerminalView>) {
        let directory = self.default_terminal_directory(cx);
        self.create_terminal_in(directory, cx)
    }

    fn create_terminal_in(
        &mut self,
        directory: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) -> (TerminalSessionId, Entity<TerminalView>) {
        let id = cx
            .global::<ApplicationTerminalSessions>()
            .0
            .borrow_mut()
            .allocate_id();
        let session = cx.new(|cx| TerminalSession::new(directory, cx));
        let view = cx.new(|cx| TerminalView::new(session.clone(), cx));
        cx.global::<ApplicationTerminalSessions>()
            .0
            .borrow_mut()
            .sessions
            .insert(id, session);
        (id, view)
    }

    fn close_terminal(&mut self, id: TerminalSessionId, cx: &mut Context<Self>) {
        let session = cx
            .global::<ApplicationTerminalSessions>()
            .0
            .borrow_mut()
            .remove(id);
        if let Some(session) = session {
            session.update(cx, |session, cx| session.close(cx));
        }
    }

    fn close_terminal_ids(ids: impl IntoIterator<Item = TerminalSessionId>, cx: &mut App) {
        for id in ids {
            let session = cx
                .global::<ApplicationTerminalSessions>()
                .0
                .borrow_mut()
                .remove(id);
            if let Some(session) = session {
                session.update(cx, |session, cx| session.close(cx));
            }
        }
    }

    /// Give Knot-side focus to `focus` and have the window follow.
    fn set_focus(&mut self, focus: Focus, cx: &mut Context<Self>) {
        self.focus = focus;
        self.follow_focus(cx);
        // After the window follows, so released keys reach what took focus.
        self.release_keys_for_focus(cx);
    }

    /// Focus the focused pane's active surface.
    pub(super) fn focus_active_surface(&mut self, cx: &mut Context<Self>) {
        self.set_focus(Focus::Workbench, cx);
    }

    /// Focus `pane`'s active surface.
    fn focus_pane_surface(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        self.workbench.update(cx, |workbench, _| {
            workbench.focus_pane(pane);
        });
        self.focus_active_surface(cx);
    }

    /// What has Knot-side focus, by identity. Reading it never reads a view,
    /// so a view handling input may ask.
    fn focus_target(&self, cx: &App) -> FocusTarget {
        let pane = self.workbench.read(cx).focused_pane();
        FocusTarget {
            layer: self.focus,
            pane: pane.map(|pane| pane.id()),
            tab: pane.map(|pane| pane.active_tab_id()),
        }
    }

    /// The focus handle of what has Knot-side focus.
    fn focused_handle(&self, cx: &App) -> Option<FocusHandle> {
        match self.focus {
            Focus::Workbench => {
                let workbench = self.workbench.read(cx);
                match workbench.focused_pane_id() {
                    Some(_) => workbench
                        .focused_command_view()
                        .map(|view| view.focus_handle(cx)),
                    None => Some(self.empty_focus.clone()),
                }
            }
            Focus::Palette => self
                .command_palette
                .as_ref()
                .map(|palette| palette.picker().read(cx).focus_handle(cx)),
            Focus::Transient => self
                .transient
                .as_ref()
                .and_then(|active| active.focus.clone()),
        }
    }

    /// Have the attached window focus what has Knot-side focus once the
    /// current update ends, so a caller may hold the window.
    fn follow_focus(&self, cx: &mut Context<Self>) {
        let Some(window) = self.window else {
            return;
        };
        let shell = cx.weak_entity();
        cx.defer(move |cx| {
            let Some(handle) = shell
                .upgrade()
                .and_then(|shell| shell.read(cx).focused_handle(cx))
            else {
                return;
            };
            let _ = window.update(cx, |_, window, cx| window.focus(&handle, cx));
        });
    }

    /// The focused pane's active tab, with whether its view has focus.
    pub(crate) fn capture_command_target(
        &mut self,
        cx: &mut Context<Self>,
    ) -> ProductCommandTarget {
        let workbench = self.workbench.read(cx);
        let focused = self.focus == Focus::Workbench;
        let pane = workbench.focused_pane();
        let view = pane
            .and_then(|pane| workbench.command_view(pane.active_tab()))
            .and_then(|view| match view {
                super::workbench::CommandView::Extension(extension) if focused => {
                    Some(extension.instance_id(cx))
                }
                _ => None,
            });
        ProductCommandTarget {
            shell: cx.entity().downgrade(),
            workbench: self.workbench.downgrade(),
            tab: pane.map(|pane| {
                let tab = pane.active_tab();
                TargetTab {
                    pane: pane.id(),
                    tab: tab.id(),
                    surface: tab.surface_id(),
                    presentation: tab.presentation(),
                }
            }),
            focused,
            view,
        }
    }

    fn dispatch_command(
        &mut self,
        command: knot_protocol::Command,
        target: ProductCommandTarget,
        cx: &mut Context<Self>,
    ) {
        Self::command_dispatcher(cx).update(cx, |dispatcher, cx| {
            dispatcher.dispatch(command, target, cx);
        });
    }

    fn dispatch_source(&mut self, source: &ProductCommandSource, cx: &mut Context<Self>) {
        let target = self.capture_command_target(cx);
        self.dispatch_command(source.command(), target, cx);
    }

    #[cfg(test)]
    fn open_command_palette(&mut self, cx: &mut Context<Self>) {
        let origin = self.capture_command_target(cx);
        self.open_command_palette_from(origin, cx);
    }

    fn record_palette_command(name: &str, cx: &App) {
        let Some(history) = cx.try_global::<ApplicationCommandHistory>() else {
            return;
        };
        let catalog = Self::command_dispatcher(cx).read(cx).catalog();
        let catalog = catalog.borrow();
        history
            .0
            .record(name, |recent| catalog.resolve(recent).is_ok());
    }

    /// The keymap view kind of a captured target's focused tab; contributed
    /// frames and native tools share this path instead of a shell-level
    /// special case.
    fn target_view_kind(target: &ProductCommandTarget, cx: &App) -> Option<String> {
        target.command_view(cx).and_then(|view| {
            target.focused.then(|| match view {
                super::workbench::CommandView::Editor(_) => "editor".to_owned(),
                super::workbench::CommandView::Terminal(_) => "terminal".to_owned(),
                super::workbench::CommandView::Workspace(_) => "workspace-tree".to_owned(),
                super::workbench::CommandView::Extension(view) => view.kind(cx),
            })
        })
    }

    fn open_command_palette_from(&mut self, origin: ProductCommandTarget, cx: &mut Context<Self>) {
        let definitions = Self::command_dispatcher(cx)
            .read(cx)
            .definitions()
            .collect::<Vec<_>>();
        let bindings = cx
            .try_global::<ApplicationKeymaps>()
            .map(|keymaps| {
                keymaps
                    .0
                    .shown_bindings(Self::target_view_kind(&origin, cx).as_deref())
            })
            .unwrap_or_default();
        let recent = cx
            .try_global::<ApplicationCommandHistory>()
            .map(|history| history.0.recent())
            .unwrap_or_default();
        let palette = CommandPalette::new(definitions, &bindings, &recent, origin, cx);
        let picker = palette.picker().clone();
        let subscription = cx.subscribe(&picker, |this, _picker, event: &PickerEvent, cx| {
            if matches!(event, PickerEvent::Highlighted(_)) {
                return;
            }
            let Some(palette) = this.command_palette.take() else {
                return;
            };
            this.command_palette_subscription = None;
            // Give focus back before dispatch, so a command that moves focus
            // or opens a transient takes it from the origin.
            this.restore_palette_origin_focus(&palette.origin(), cx);
            if let PickerEvent::Confirmed(entry) = event {
                let command = palette.command(*entry);
                Self::record_palette_command(command.name.as_ref(), cx);
                this.dispatch_command(command, palette.origin(), cx);
            }
            this.refocus_active_transient(cx);
            cx.notify();
        });
        self.command_palette = Some(palette);
        self.command_palette_subscription = Some(subscription);
        self.set_focus(Focus::Palette, cx);
        cx.notify();
    }

    fn transient_slot_free(&self) -> Result<(), TransientError> {
        if self.transient.is_some() {
            Err(TransientError::OperationInProgress)
        } else {
            Ok(())
        }
    }

    fn finish_transient(&mut self, outcome: TransientOutcome, cx: &mut Context<Self>) {
        let Some(mut active) = self.transient.take() else {
            return;
        };
        active.settle(outcome);
        if active.focus.is_some() {
            self.set_focus(active.restore_focus, cx);
        }
        cx.notify();
    }

    /// Focus the pane the palette opened from, or the focused pane when that
    /// pane is gone.
    fn restore_palette_origin_focus(
        &mut self,
        origin: &ProductCommandTarget,
        cx: &mut Context<Self>,
    ) {
        if let Some(origin) = origin.tab
            && self.workbench.read(cx).pane(origin.pane).is_some()
        {
            self.focus_pane_surface(origin.pane, cx);
        } else {
            self.focus_active_surface(cx);
        }
    }

    fn refocus_active_transient(&mut self, cx: &mut Context<Self>) {
        if self
            .transient
            .as_ref()
            .is_some_and(|active| active.focus.is_some())
        {
            self.set_focus(Focus::Transient, cx);
        }
    }

    /// Cancel the live interaction when it belongs to the requested owner.
    /// A `None` invocation cancels any interaction of that extension lifetime.
    pub(crate) fn cancel_transient(&mut self, owner: TransientOwner, cx: &mut Context<Self>) {
        let matches = self
            .transient
            .as_ref()
            .and_then(|active| active.owner)
            .is_some_and(|active| {
                active.extension == owner.extension
                    && active.lifecycle == owner.lifecycle
                    && (owner.invocation.is_none() || active.invocation == owner.invocation)
            });
        if matches {
            self.finish_transient(TransientOutcome::Cancelled, cx);
        }
    }

    /// Present a lifecycle-authorized picker, or reject when another extension
    /// interaction already owns this window.
    pub(crate) fn begin_picker(
        &mut self,
        owner: TransientOwner,
        title: String,
        items: Vec<knot_protocol::PickerItem>,
        highlights: Option<tokio::sync::watch::Sender<Option<String>>>,
        cx: &mut Context<Self>,
    ) -> Result<tokio::sync::oneshot::Receiver<TransientOutcome>, TransientError> {
        self.begin_owned_picker(Some(owner), title, items, highlights, cx)
    }

    /// Present a picker in the transient slot. With `highlights`, the ID of
    /// the highlighted item is published as it changes, starting with the
    /// initial one.
    fn begin_owned_picker(
        &mut self,
        owner: Option<TransientOwner>,
        title: String,
        items: Vec<knot_protocol::PickerItem>,
        highlights: Option<tokio::sync::watch::Sender<Option<String>>>,
        cx: &mut Context<Self>,
    ) -> Result<tokio::sync::oneshot::Receiver<TransientOutcome>, TransientError> {
        let entries = items
            .into_iter()
            .map(|item| PickerEntry {
                id: Some(item.id),
                description: item.description,
                detail: item.detail,
                hint: item.hint,
                icon: item.icon,
                ..PickerEntry::new(item.label)
            })
            .collect();
        self.present_picker(owner, title, Some(entries), highlights, None, cx)
            .map(|(receiver, _)| receiver)
    }

    /// Present a lifecycle-authorized picker whose rows the owner supplies:
    /// each query, starting with the empty one, is published on `queries`
    /// and answered through the returned view.
    pub(crate) fn begin_queried_picker(
        &mut self,
        owner: TransientOwner,
        title: String,
        highlights: tokio::sync::watch::Sender<Option<String>>,
        queries: tokio::sync::watch::Sender<Option<String>>,
        cx: &mut Context<Self>,
    ) -> Result<
        (
            tokio::sync::oneshot::Receiver<TransientOutcome>,
            WeakEntity<Picker>,
        ),
        TransientError,
    > {
        self.present_picker(
            Some(owner),
            title,
            None,
            Some(highlights),
            Some(queries),
            cx,
        )
    }

    /// Present a picker over `entries`, or with supplied rows when there are
    /// none. Entries carry the IDs that outcomes and highlights name.
    fn present_picker(
        &mut self,
        owner: Option<TransientOwner>,
        title: String,
        entries: Option<Vec<PickerEntry>>,
        highlights: Option<tokio::sync::watch::Sender<Option<String>>>,
        queries: Option<tokio::sync::watch::Sender<Option<String>>>,
        cx: &mut Context<Self>,
    ) -> Result<
        (
            tokio::sync::oneshot::Receiver<TransientOutcome>,
            WeakEntity<Picker>,
        ),
        TransientError,
    > {
        self.transient_slot_free()?;
        let restore_focus = self.focus;
        let view = cx.new(|cx| match entries {
            Some(entries) => Picker::new(Some(title), entries, super::TRANSIENT_KEY_CONTEXT, cx),
            None => Picker::supplied(Some(title), super::TRANSIENT_KEY_CONTEXT, cx),
        });
        let weak = view.downgrade();
        let focus = view.read(cx).focus_handle(cx);
        let id_of = |picker: &Picker, entry: Option<usize>| {
            entry.and_then(|entry| picker.entry(entry).id.clone())
        };
        if let Some(highlights) = &highlights {
            let picker = view.read(cx);
            highlights.send_replace(id_of(picker, picker.selected_entry()));
        }
        if let Some(queries) = &queries {
            queries.send_replace(Some(view.read(cx).query(cx)));
        }
        let subscription = cx.subscribe(
            &view,
            move |this: &mut Self, view, event: &PickerEvent, cx| {
                let outcome = match event {
                    PickerEvent::Highlighted(entry) => {
                        if let Some(highlights) = &highlights {
                            highlights.send_replace(id_of(view.read(cx), *entry));
                        }
                        return;
                    }
                    PickerEvent::Queried(query) => {
                        if let Some(queries) = &queries {
                            queries.send_replace(Some(query.clone()));
                        }
                        return;
                    }
                    PickerEvent::Confirmed(entry) => TransientOutcome::Picker(
                        id_of(view.read(cx), Some(*entry)).expect("confirmed entry"),
                    ),
                    PickerEvent::Dismissed => TransientOutcome::Dismissed,
                };
                this.finish_transient(outcome, cx);
            },
        );
        let (completion, receiver) = tokio::sync::oneshot::channel();
        self.transient = Some(ActiveTransient {
            owner,
            surface: Some(TransientSurface::Picker(view)),
            focus: Some(focus),
            completion: Some(completion),
            restore_focus,
            _subscription: Some(subscription),
        });
        self.set_focus(Focus::Transient, cx);
        cx.notify();
        Ok((receiver, weak))
    }

    /// Present a lifecycle-authorized single-line input, or reject when another
    /// extension interaction already owns this window. With `changes`, the
    /// value is published as it changes, starting with the initial one; the
    /// returned view takes the owner's messages.
    pub(crate) fn begin_input(
        &mut self,
        owner: TransientOwner,
        title: String,
        initial: String,
        prompt: Option<String>,
        changes: Option<tokio::sync::watch::Sender<Option<String>>>,
        cx: &mut Context<Self>,
    ) -> Result<
        (
            tokio::sync::oneshot::Receiver<TransientOutcome>,
            WeakEntity<InputView>,
        ),
        TransientError,
    > {
        self.transient_slot_free()?;
        let restore_focus = self.focus;
        let view = cx.new(|cx| InputView::new(title, initial, prompt, changes, cx));
        let weak = view.downgrade();
        let focus = view.read(cx).focus_handle(cx);
        let subscription = cx.subscribe(&view, |this: &mut Self, _view, event: &InputEvent, cx| {
            this.finish_transient(
                match event {
                    InputEvent::Accepted(value) => TransientOutcome::Input(value.clone()),
                    InputEvent::Dismissed => TransientOutcome::Dismissed,
                },
                cx,
            );
        });
        let (completion, receiver) = tokio::sync::oneshot::channel();
        self.transient = Some(ActiveTransient {
            owner: Some(owner),
            surface: Some(TransientSurface::Input(view)),
            focus: Some(focus),
            completion: Some(completion),
            restore_focus,
            _subscription: Some(subscription),
        });
        self.set_focus(Focus::Transient, cx);
        cx.notify();
        Ok((receiver, weak))
    }

    /// Present the native confirmation prompt, settling the shared slot when it
    /// is answered.
    pub(crate) fn begin_confirmation(
        &mut self,
        owner: TransientOwner,
        text: PromptText,
        confirm_label: String,
        cancel_label: String,
        cx: &mut Context<Self>,
    ) -> Result<tokio::sync::oneshot::Receiver<TransientOutcome>, TransientError> {
        self.transient_slot_free()?;
        let restore_focus = self.focus;
        let prompt = self.prompt(
            PromptLevel::Warning,
            text.title,
            text.message,
            vec![
                PromptButton::new(confirm_label),
                PromptButton::cancel(cancel_label),
            ],
            cx,
        );
        let (completion, receiver) = tokio::sync::oneshot::channel();
        self.transient = Some(ActiveTransient {
            owner: Some(owner),
            surface: None,
            focus: None,
            completion: Some(completion),
            restore_focus,
            _subscription: None,
        });
        cx.spawn(async move |this, cx| {
            let outcome = match prompt.await {
                Ok(0) => TransientOutcome::Confirmed,
                _ => TransientOutcome::Dismissed,
            };
            let _ = this.update(cx, |this, cx| this.finish_transient(outcome, cx));
        })
        .detach();
        cx.notify();
        Ok(receiver)
    }

    /// Present the native message prompt. Resolution carries no result, but the
    /// interaction still occupies the shared slot while it is open.
    pub(crate) fn begin_message(
        &mut self,
        owner: TransientOwner,
        text: PromptText,
        cx: &mut Context<Self>,
    ) -> Result<tokio::sync::oneshot::Receiver<TransientOutcome>, TransientError> {
        self.transient_slot_free()?;
        let restore_focus = self.focus;
        let prompt = self.prompt(
            PromptLevel::Info,
            text.title,
            text.message,
            vec![PromptButton::new("OK")],
            cx,
        );
        let (completion, receiver) = tokio::sync::oneshot::channel();
        self.transient = Some(ActiveTransient {
            owner: Some(owner),
            surface: None,
            focus: None,
            completion: Some(completion),
            restore_focus,
            _subscription: None,
        });
        cx.spawn(async move |this, cx| {
            let _ = prompt.await;
            let _ = this.update(cx, |this, cx| {
                this.finish_transient(TransientOutcome::Dismissed, cx)
            });
        })
        .detach();
        cx.notify();
        Ok(receiver)
    }

    /// Present the native directory prompt, settling the shared slot when it is
    /// answered. Path validation and normalization belong to the caller.
    pub(crate) fn begin_folder_picker(
        &mut self,
        owner: Option<TransientOwner>,
        cx: &mut Context<Self>,
    ) -> Result<tokio::sync::oneshot::Receiver<TransientOutcome>, TransientError> {
        self.transient_slot_free()?;
        let restore_focus = self.focus;
        let selection: OpenDialogFuture =
            if let Some(dialog) = cx.try_global::<ApplicationOpenDialog>() {
                dialog.0.select()
            } else {
                let selection = cx.prompt_for_paths(PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: false,
                    prompt: Some("Choose Folder".into()),
                });
                Box::pin(async move {
                    match selection.await {
                        Ok(Ok(Some(paths))) => paths
                            .into_iter()
                            .next()
                            .map(OpenDialogOutcome::Selected)
                            .unwrap_or(OpenDialogOutcome::Cancelled),
                        Ok(Ok(None)) => OpenDialogOutcome::Cancelled,
                        Ok(Err(error)) => OpenDialogOutcome::Failed(error.to_string()),
                        Err(error) => OpenDialogOutcome::Failed(error.to_string()),
                    }
                })
            };
        let (completion, receiver) = tokio::sync::oneshot::channel();
        self.transient = Some(ActiveTransient {
            owner,
            surface: None,
            focus: None,
            completion: Some(completion),
            restore_focus,
            _subscription: None,
        });
        cx.spawn(async move |this, cx| {
            let outcome = match selection.await {
                OpenDialogOutcome::Selected(path) => TransientOutcome::Folder(path),
                OpenDialogOutcome::Cancelled => TransientOutcome::Dismissed,
                OpenDialogOutcome::Failed(message) => TransientOutcome::Failed(message),
            };
            let _ = this.update(cx, |this, cx| this.finish_transient(outcome, cx));
        })
        .detach();
        cx.notify();
        Ok(receiver)
    }

    fn render_transient(&self) -> Option<AnyElement> {
        self.transient.as_ref().and_then(|active| active.render())
    }

    fn documents(cx: &App) -> Entity<DocumentCollection> {
        cx.global::<ApplicationDocuments>().0.clone()
    }

    pub(crate) fn filesystems(cx: &App) -> Arc<FileSystemProviderRegistry> {
        cx.global::<ApplicationFileSystems>().0.clone()
    }

    #[cfg(any(test, feature = "test-support"))]
    fn new_tab(&mut self, cx: &mut Context<Self>) {
        let pane = self.workbench.read(cx).focused_pane_id();
        self.new_document_in_pane(pane, cx);
    }

    fn new_document_in_pane(
        &mut self,
        pane: Option<PaneId>,
        cx: &mut Context<Self>,
    ) -> Option<(DocumentId, OpenTarget)> {
        self.new_untitled_in_pane(pane, "Untitled", "", cx)
    }

    /// The center pane a new tab beside `pane` opens in: `pane` itself when
    /// it is in the center, else the center pane, or `None` when the center
    /// is empty. A pane that no longer exists yields `Err`.
    fn center_destination(&self, pane: Option<PaneId>, cx: &App) -> Result<Option<PaneId>, ()> {
        let workbench = self.workbench.read(cx);
        match pane {
            Some(pane) => match workbench.pane_area(pane).ok_or(())? {
                Area::Center => Ok(Some(pane)),
                Area::Dock(_) => Ok(workbench.center_pane()),
            },
            None => Ok(workbench.center_pane()),
        }
    }

    /// Open an untitled document holding `text` beside `pane`, in the center,
    /// and focus it. The text is its clean state, so closing it unedited
    /// never prompts.
    fn new_untitled_in_pane(
        &mut self,
        pane: Option<PaneId>,
        title: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Option<(DocumentId, OpenTarget)> {
        let destination = self.center_destination(pane, cx).ok()?;
        self.next_open_generation();
        let documents = Self::documents(cx);
        let model = cx.new(|_| Buffer::new(BufferModel::from_text(text)));
        let document = documents.update(cx, |documents, cx| {
            documents.create_untitled(title.to_owned(), model, cx)
        });
        let model = documents.read(cx).get(document).unwrap().model().clone();
        let (pane, tab) = self
            .workbench
            .update(cx, |workbench, cx| match destination {
                Some(pane) => {
                    let tab = workbench
                        .open_tab_for_document(pane, document, model, cx)
                        .unwrap();
                    (pane, tab)
                }
                None => workbench.open_document_in_center(document, model, cx),
            });
        self.focus_active_surface(cx);
        cx.notify();
        Some((
            document,
            OpenTarget::at(cx.weak_entity(), &self.workbench, pane, tab, cx),
        ))
    }

    /// Opens a focused terminal starting in `directory`. In a dock it opens
    /// beside the terminal in `pane`, else beside the most recently used dock
    /// terminal, else in the terminal home dock; in the center, beside `pane`
    /// or in the center pane.
    fn new_terminal_in_pane(
        &mut self,
        pane: Option<PaneId>,
        location: operations::TerminalLocation,
        directory: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) -> Option<OpenTarget> {
        enum Destination {
            Pane(PaneId),
            Dock(DockLocation),
            Center,
        }
        let destination = match location {
            operations::TerminalLocation::Dock => {
                let workbench = self.workbench.read(cx);
                let beside = pane.filter(|pane| {
                    workbench.pane(*pane).is_some_and(|pane| {
                        matches!(pane.active_tab().surface_id(), TabSurfaceId::Terminal(_))
                    })
                });
                match beside.or_else(|| workbench.recent_dock_terminal().map(|(pane, _)| pane)) {
                    Some(pane) => Destination::Pane(pane),
                    None => Destination::Dock(workbench.terminal_home()),
                }
            }
            operations::TerminalLocation::Center => match self.center_destination(pane, cx).ok()? {
                Some(pane) => Destination::Pane(pane),
                None => Destination::Center,
            },
        };
        let (id, view) = self.create_terminal_in(directory, cx);
        let Some((pane, tab)) = self.workbench.update(cx, |workbench, _| match destination {
            Destination::Pane(pane) => workbench
                .open_terminal_tab(pane, id, view)
                .map(|tab| (pane, tab)),
            Destination::Dock(dock) => workbench.open_terminal_in_dock(dock, id, view),
            Destination::Center => workbench.open_terminal_in_center(id, view),
        }) else {
            self.close_terminal(id, cx);
            return None;
        };
        self.focus_active_surface(cx);
        cx.notify();
        Some(OpenTarget::at(
            cx.weak_entity(),
            &self.workbench,
            pane,
            tab,
            cx,
        ))
    }

    /// Toggle the dock terminals (see [`Workbench::toggle_terminal`]); with
    /// none, open one in the terminal home dock.
    pub(super) fn toggle_terminal(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), knot_protocol::CommandOutcome> {
        let toggle = self
            .workbench
            .update(cx, |workbench, _| workbench.toggle_terminal());
        if toggle == knot_model::workbench::TerminalToggle::NeedsTerminal {
            let directory = self.terminal_directory(cx);
            self.new_terminal_in_pane(None, operations::TerminalLocation::Dock, directory, cx)
                .ok_or(knot_protocol::CommandOutcome::InvalidTarget)?;
        }
        self.publish_tool_layout(cx);
        self.focus_active_surface(cx);
        cx.notify();
        Ok(())
    }

    #[cfg(test)]
    fn split(&mut self, direction: SplitDirection, cx: &mut Context<Self>) {
        let Some(pane) = self.workbench.read(cx).focused_pane_id() else {
            return;
        };
        self.split_pane(pane, direction, SplitPlacement::After, cx);
    }

    fn split_pane(
        &mut self,
        pane: PaneId,
        direction: SplitDirection,
        placement: SplitPlacement,
        cx: &mut Context<Self>,
    ) -> bool {
        let terminal = match self
            .workbench
            .read(cx)
            .pane(pane)
            .map(|pane| pane.active_tab().surface_id())
        {
            Some(TabSurfaceId::Terminal(id)) => Some(id),
            _ => None,
        };
        let new_session = terminal.map(|id| {
            let directory = Self::terminal_session_directory(id, cx)
                .or_else(|| self.default_terminal_directory(cx));
            self.create_terminal_in(directory, cx)
        });
        let split = self
            .workbench
            .update(cx, |workbench, cx| match new_session.clone() {
                Some((id, view)) => {
                    workbench.split_pane_with_terminal(pane, direction, placement, id, view, cx)
                }
                None => workbench.split_pane(pane, direction, placement, cx),
            });
        if split.is_some() {
            self.focus_active_surface(cx);
            cx.notify();
            true
        } else {
            if let Some((id, _)) = new_session {
                self.close_terminal(id, cx);
            }
            false
        }
    }

    fn activate_tab(&mut self, pane: PaneId, tab: TabId, cx: &mut Context<Self>) {
        let activated = self.workbench.update(cx, |workbench, _| {
            workbench.activate_tab(pane, tab);
            workbench
                .pane(pane)
                .is_some_and(|pane| pane.active_tab_id() == tab)
        });
        if activated {
            self.focus_active_surface(cx);
        }
        cx.notify();
    }

    #[cfg(any(test, feature = "test-support"))]
    fn focus_active_editor(&mut self, cx: &mut Context<Self>) {
        self.focus_active_surface(cx);
    }

    /// Report an outcome that has no other visible effect in this session.
    /// The prompt needs no answer.
    pub(super) fn show_notice(
        &self,
        level: PromptLevel,
        title: impl Into<String>,
        detail: Option<String>,
        cx: &mut App,
    ) {
        show_notice(self.window, level, title.into(), detail, cx);
    }

    pub(crate) fn toggle_extension_report(&mut self, cx: &mut Context<Self>) {
        self.extension_report_open = !self.extension_report_open;
        cx.notify();
    }
}

/// The index of the button a prompt was answered with; an error when the
/// session has no window or it closed first.
type PromptAnswer = tokio::sync::oneshot::Receiver<usize>;

/// Ask with a native prompt in a session's `window` once the current update
/// ends, so a caller may hold the session or the window.
fn prompt(
    window: Option<AnyWindowHandle>,
    level: PromptLevel,
    title: String,
    detail: Option<String>,
    buttons: Vec<PromptButton>,
    cx: &mut App,
) -> PromptAnswer {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    if let Some(window) = window {
        cx.defer(move |cx| {
            let Ok(answer) = window.update(cx, |_, window, cx| {
                window.prompt(level, &title, detail.as_deref(), &buttons, cx)
            }) else {
                return;
            };
            cx.foreground_executor()
                .spawn(async move {
                    if let Ok(answer) = answer.await {
                        let _ = sender.send(answer);
                    }
                })
                .detach();
        });
    }
    receiver
}

/// Report an outcome that has no other visible effect in a session shown in
/// `window`. The prompt needs no answer.
pub(super) fn show_notice(
    window: Option<AnyWindowHandle>,
    level: PromptLevel,
    title: String,
    detail: Option<String>,
    cx: &mut App,
) {
    drop(prompt(
        window,
        level,
        title,
        detail,
        vec![PromptButton::new("OK")],
        cx,
    ));
}

#[cfg(any(test, feature = "test-support"))]
fn create_untitled_document(documents: &Entity<DocumentCollection>, cx: &mut App) -> DocumentId {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("")));
    documents.update(cx, |documents, cx| {
        documents.create_untitled("Untitled", model, cx)
    })
}

fn resource_title(uri: &super::resource::ResourceUri) -> String {
    uri.as_url()
        .path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .map(percent_encoding::percent_decode_str)
        .and_then(|title| title.decode_utf8().ok())
        .filter(|title| !title.is_empty())
        .map(|title| title.into_owned())
        .unwrap_or_else(|| uri.to_string())
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) mod driver;
#[cfg(test)]
mod tests;
