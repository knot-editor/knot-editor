//! Windows backed by application documents and workbenches, each rendered by
//! a native window.
//!
//! This module owns window state, focus, command routing, terminals, and
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

mod about;
mod application_log;
mod closure;
pub(super) mod commands;
mod document_watch;
mod extension_entitlements;
mod extension_restart;
mod key_hold;
mod key_routing;
mod language_mode;
mod launch;
mod navigation;
mod notifications;
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
pub(crate) mod surface_menu;
mod tab_drag;
mod tool_commands;
mod tool_picker;
mod tools;
mod view_state;

#[cfg(test)]
use super::commands::dispatch::SHOW_COMPLETIONS_COMMAND;
use closure::install_protected_window_close;
use knot_model::launch::{Destination, PlacedRequest, PlacementWindow, place};
#[cfg(test)]
pub(super) use launch::bind_product_keys;
pub(super) use launch::finish_product_startup;
use launch::open_tab_tear_out_window;
#[cfg(any(test, feature = "test-support"))]
#[cfg(test)]
use launch::product_filesystems;
#[cfg(test)]
use launch::{dispatch_launch_request, open_tab_transfer_window};
pub(crate) use launch::{open_product_window, run};

use super::{
    CommandCompletion, CommandPalette,
    commands::dispatch::{
        ApplicationCommandSource, ApplicationProductCommands, CLOSE_TAB_COMMAND,
        CLOSE_WINDOW_COMMAND, COPY_COMMAND, CUT_COMMAND, ContributedMenuCommand, HIDE_COMMAND,
        HIDE_OTHERS_COMMAND, MINIMIZE_WINDOW_COMMAND, NEW_COMMAND, NEW_TERMINAL_COMMAND,
        NEW_WINDOW_COMMAND, OPEN_COMMAND, PASTE_COMMAND, ProductCommandDispatcher,
        ProductCommandSource, ProductCommandTarget, QUIT_COMMAND, REDO_COMMAND, RESET_ZOOM_COMMAND,
        SAVE_AS_COMMAND, SAVE_COMMAND, SELECT_ALL_COMMAND, SHOW_ALL_COMMAND,
        SHOW_EXTENSION_REPORT_COMMAND, SPLIT_HORIZONTAL_COMMAND, SPLIT_VERTICAL_COMMAND, TargetTab,
        UNDO_COMMAND, ZOOM_IN_COMMAND, ZOOM_OUT_COMMAND, ZOOM_WINDOW_COMMAND,
        dispatch_close_to_captured_target, dispatch_open_to_captured_target,
        dispatch_save_to_captured_target,
    },
    commands::keymaps::ApplicationKeymaps,
    documents::{ApplicationDocuments, DocumentCollection, DocumentId, DocumentState},
    entry::{LaunchBatch, OpenRequest},
    extensions::host::handlers::TransientError,
    filesystem::{
        FileSystemProviderRegistry, LocalFileSystemProvider, ResourceError, ResourceKind,
        ResourceStat, ResourceVersion,
    },
    model::{Buffer, BufferModel},
    open::{OpenResource, load_resource},
    picker::{Picker, PickerAsk, PickerEntry, PickerEvent, PickerTab},
    terminal::session::{TerminalSession, TerminalSessionEvent},
    terminal::view::TerminalView,
    transient::{
        InputEvent, InputSpec, InputView, PromptText, TransientOutcome, TransientOwner,
        TransientSurface,
    },
    workbench::{
        Area, CloseRequestOutcome, DockLocation, DocumentCloseDisposition, PaneId, SplitDirection,
        SplitPlacement, TabId, TabSurfaceId, TerminalSessionId, ToolContent, ToolFrameId,
        Workbench, WorkbenchLayout,
    },
    workspace::WorkspaceState,
};

/// Every open window. A window opens when it is created and closes with its
/// native window. The registry answers which workbench and native window a
/// window has without reading the window, which may be updating.
#[derive(Default)]
struct ApplicationWindows(RefCell<Vec<OpenWindow>>);

struct OpenWindow {
    /// When the window was last activated, from a count over all windows;
    /// zero while it never was.
    activated: u64,
    window: WeakEntity<KnotWindow>,
    workbench: WeakEntity<Workbench>,
    native_window: Option<AnyWindowHandle>,
    /// Whether this window was activated last, which makes it the active
    /// window while it has no native window.
    active: bool,
}

impl Global for ApplicationWindows {}

/// Present in tests that open windows without a native window, which the
/// application's own launch and window commands then create. It owns each
/// window until it ends, as its native window would.
#[cfg(any(test, feature = "test-support"))]
#[derive(Default)]
pub(crate) struct HeadlessWindows(RefCell<Vec<Entity<KnotWindow>>>);

#[cfg(any(test, feature = "test-support"))]
impl Global for HeadlessWindows {}

struct ApplicationProtectedClosure(RefCell<bool>);

impl Global for ApplicationProtectedClosure {}

struct ApplicationTerminalSessions(RefCell<TerminalSessions>);

impl Global for ApplicationTerminalSessions {}

struct ApplicationLaunchGate(RefCell<LaunchGate>);

impl Global for ApplicationLaunchGate {}

enum LaunchGate {
    Pending(Vec<Option<LaunchBatch>>),
    Ready,
    Failed,
}

impl LaunchGate {
    fn request(&mut self, request: Option<LaunchBatch>) -> Option<Option<LaunchBatch>> {
        match self {
            Self::Pending(requests) => {
                requests.push(request);
                None
            }
            Self::Ready => Some(request),
            Self::Failed => None,
        }
    }

    /// The queued batches; `None` entries are reopen requests without a
    /// resource.
    fn complete(&mut self) -> Vec<Option<LaunchBatch>> {
        match std::mem::replace(self, Self::Ready) {
            Self::Pending(requests) => requests,
            Self::Ready => Vec::new(),
            Self::Failed => {
                *self = Self::Failed;
                Vec::new()
            }
        }
    }

    /// The batches queued so far, which stay pending.
    fn take_queued(&mut self) -> Vec<LaunchBatch> {
        match self {
            Self::Pending(requests) => std::mem::take(requests).into_iter().flatten().collect(),
            Self::Ready | Self::Failed => Vec::new(),
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

use crate::app::notifications::{NoticeAction, NoticeSpec, Severity};
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

impl ApplicationWindows {
    /// Open `window`, or record the workbench an open one now shows.
    fn register(&self, window: WeakEntity<KnotWindow>, workbench: &Entity<Workbench>) {
        let mut windows = self.0.borrow_mut();
        match windows.iter_mut().find(|open| open.window == window) {
            Some(open) => open.workbench = workbench.downgrade(),
            None => windows.push(OpenWindow {
                activated: 0,
                window,
                workbench: workbench.downgrade(),
                native_window: None,
                active: false,
            }),
        }
    }

    /// Record the native window an open window is shown in.
    fn attach(&self, window: EntityId, native_window: AnyWindowHandle) {
        if let Some(open) = self
            .0
            .borrow_mut()
            .iter_mut()
            .find(|open| open.window.entity_id() == window)
        {
            open.native_window = Some(native_window);
        }
    }

    /// Mark `window` as the one activated last.
    fn activate(&self, window: EntityId) {
        let mut windows = self.0.borrow_mut();
        let stamp = windows.iter().map(|open| open.activated).max().unwrap_or(0) + 1;
        for open in windows.iter_mut() {
            open.active = open.window.entity_id() == window;
            if open.active {
                open.activated = stamp;
            }
        }
    }

    /// The open windows, the most recently activated first; windows never
    /// activated follow, the most recently opened first.
    fn by_activation(&self) -> Vec<Entity<KnotWindow>> {
        let mut windows = self.0.borrow_mut();
        windows.retain(|open| open.window.upgrade().is_some());
        let mut order = windows.iter().enumerate().collect::<Vec<_>>();
        order.sort_by_key(|(index, open)| std::cmp::Reverse((open.activated, *index)));
        order
            .into_iter()
            .filter_map(|(_, open)| open.window.upgrade())
            .collect()
    }

    /// The activated window, when it has no native window.
    fn active_without_window(&self) -> Option<Entity<KnotWindow>> {
        self.0
            .borrow()
            .iter()
            .find(|open| open.active && open.native_window.is_none())
            .and_then(|open| open.window.upgrade())
    }

    /// Close `window`. When it was the activated one, the window opened last
    /// takes over, as the platform activates another native window.
    fn close(&self, window: EntityId) {
        let mut windows = self.0.borrow_mut();
        let was_active = windows
            .iter()
            .any(|open| open.active && open.window.entity_id() == window);
        windows.retain(|open| open.window.entity_id() != window);
        if was_active && let Some(last) = windows.last_mut() {
            last.active = true;
        }
    }

    fn is_open(&self, window: EntityId) -> bool {
        self.0
            .borrow()
            .iter()
            .any(|open| open.window.entity_id() == window)
    }

    /// The open windows, in the order they opened.
    fn windows(&self) -> Vec<Entity<KnotWindow>> {
        let mut windows = self.0.borrow_mut();
        windows.retain(|open| open.window.upgrade().is_some());
        windows
            .iter()
            .filter_map(|open| open.window.upgrade())
            .collect()
    }

    /// The open window with the given public ID.
    fn find(&self, id: &str) -> Option<Entity<KnotWindow>> {
        self.windows()
            .into_iter()
            .find(|window| window_id(window.entity_id()) == id)
    }

    /// The native window an open window is shown in.
    fn native_window(&self, window: EntityId) -> Option<AnyWindowHandle> {
        self.0
            .borrow()
            .iter()
            .find(|open| open.window.entity_id() == window)
            .and_then(|open| open.native_window)
    }

    /// The open window shown in `native_window`.
    fn in_native_window(&self, native_window: AnyWindowHandle) -> Option<Entity<KnotWindow>> {
        self.0
            .borrow()
            .iter()
            .find(|open| open.native_window == Some(native_window))
            .and_then(|open| open.window.upgrade())
    }

    /// The open window showing `workbench`.
    fn window_for(&self, workbench: &Entity<Workbench>) -> Option<WeakEntity<KnotWindow>> {
        self.0.borrow().iter().find_map(|open| {
            (open.workbench.upgrade().as_ref() == Some(workbench)).then(|| open.window.clone())
        })
    }

    fn view_count(&self, document: DocumentId, cx: &App) -> usize {
        let mut windows = self.0.borrow_mut();
        windows
            .retain(|open| open.window.upgrade().is_some() && open.workbench.upgrade().is_some());
        windows
            .iter()
            .filter_map(|open| open.workbench.upgrade())
            .map(|workbench| workbench.read(cx).view_count(document))
            .sum()
    }
}

/// The public ID of `window`: `WindowTarget.window` names it.
pub(crate) fn window_id(window: EntityId) -> String {
    window.as_u64().to_string()
}

/// Whether `window` is open and shows `workbench`.
pub(super) fn window_is_live(
    window: &WeakEntity<KnotWindow>,
    workbench: &Entity<Workbench>,
    cx: &App,
) -> bool {
    cx.global::<ApplicationWindows>()
        .window_for(workbench)
        .is_some_and(|open| open == *window)
}

/// The open windows, in the order they opened.
pub(crate) fn windows(cx: &App) -> Vec<Entity<KnotWindow>> {
    cx.try_global::<ApplicationWindows>()
        .map(ApplicationWindows::windows)
        .unwrap_or_default()
}

/// The open window a public ID names.
pub(crate) fn find_window(id: &str, cx: &App) -> Option<Entity<KnotWindow>> {
    cx.try_global::<ApplicationWindows>()?.find(id)
}

/// The open windows, the most recently activated first.
pub(super) fn windows_by_activation(cx: &App) -> Vec<Entity<KnotWindow>> {
    cx.try_global::<ApplicationWindows>()
        .map(ApplicationWindows::by_activation)
        .unwrap_or_default()
}

/// The window shown in `native_window`.
pub(crate) fn window_in(native_window: AnyWindowHandle, cx: &App) -> Option<Entity<KnotWindow>> {
    cx.try_global::<ApplicationWindows>()?
        .in_native_window(native_window)
}

/// The window shown in the active native window, or without one the window
/// activated last when it has no native window.
pub(crate) fn active_window(cx: &App) -> Option<Entity<KnotWindow>> {
    match cx.active_window() {
        Some(native_window) => window_in(native_window, cx),
        None => cx
            .try_global::<ApplicationWindows>()?
            .active_without_window(),
    }
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static TEST_WINDOW_IDS: RefCell<HashMap<u64, String>> = RefCell::default();
}

/// The public ID of the window last shown in `native_window`.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn test_window_id(native_window: AnyWindowHandle) -> String {
    TEST_WINDOW_IDS.with_borrow(|ids| ids[&native_window.window_id().as_u64()].clone())
}

/// Run the command a click on an editor contribution names, in the window the
/// click happened in.
pub(crate) fn run_contribution_action(
    action: super::editor::EditorContributionAction,
    cx: &mut App,
) {
    let Some(window) = cx
        .try_global::<ApplicationWindows>()
        .and_then(|windows| windows.in_native_window(action.window))
    else {
        return;
    };
    window.update(cx, |window, cx| window.run_contribution_action(&action, cx));
}

/// The native window an open window is shown in, without reading the window.
pub(crate) fn native_window(window: EntityId, cx: &App) -> Option<AnyWindowHandle> {
    cx.global::<ApplicationWindows>().native_window(window)
}

/// Where an open lands: beside the captured tab, or in the center when the
/// open was captured without one.
#[derive(Clone)]
struct OpenTarget {
    window: WeakEntity<KnotWindow>,
    workbench: WeakEntity<Workbench>,
    tab: Option<TargetTab>,
}

impl OpenTarget {
    fn from_command(target: &ProductCommandTarget) -> Self {
        Self {
            window: target.window.clone(),
            workbench: target.workbench.clone(),
            tab: target.tab,
        }
    }

    /// A target at a live tab of `workbench`.
    fn at(
        window: WeakEntity<KnotWindow>,
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
            window,
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

/// The extension command that is the command palette's Actions tab.
const SEARCH_EVERYWHERE_COMMAND: &str = "workspace.search-everywhere";

pub(crate) struct KnotWindow {
    workbench: Entity<Workbench>,
    native_window: Option<AnyWindowHandle>,
    tab_drag: std::rc::Rc<RefCell<tab_drag::TabDragState>>,
    dock_resize: Option<DockResizeDrag>,
    slot_resize: Option<SlotResizeDrag>,
    /// The open right-click menu of a tool stripe button.
    stripe_menu: Option<crate::app::context_menu::ContextMenu>,
    /// The open right-click menu of a tab header.
    surface_menu: Option<crate::app::context_menu::ContextMenu>,
    /// Whether a stripe button is being dragged, which shows empty stripes.
    stripe_dragging: std::rc::Rc<std::cell::Cell<bool>>,
    split_resize: Option<SplitResizeDrag>,
    recent_walk: Option<knot_model::workbench::RecentWalk>,
    /// The pane rendered over the whole workbench, while it stays focused.
    maximized: Option<PaneId>,
    /// Per tool window, the pane that was maximized when the window was
    /// hidden; showing the window again maximizes it.
    hidden_maximized: Vec<(ToolFrameId, PaneId)>,
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
    notices: notifications::NoticePopups,
    application_focus: Vec<(FocusHandle, Subscription, Subscription)>,
    contributed_views: BTreeMap<String, ContributedView>,
    /// The container tool windows contributed by extensions, by stable ID,
    /// with the command that opens a tab in each.
    contributed_containers: BTreeMap<String, Option<String>>,
    _contributed_view_release: Option<Subscription>,
    _window_closed: Option<Subscription>,
    _extension_host_release: Option<Subscription>,
    open_generation: u64,
    tasks: Vec<Task<()>>,
    extension_report_open: bool,
    extension_report_subscription: Option<Subscription>,
    view_state: view_state::ViewStateTracking,
    /// Creation order across windows; native window identities are reused.
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

/// The layer of a window that has focus: the focused pane's active
/// surface, the command palette, the transient interaction, or an open
/// context menu.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Focus {
    #[default]
    Workbench,
    Palette,
    Transient,
    /// An open context menu, which takes the keys the keymap leaves.
    Popup,
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

/// The divider between a side's two tool windows being dragged: the first
/// window's share of the side at the start, and the painted extent along the
/// split.
#[derive(Clone, Copy)]
struct SlotResizeDrag {
    side: DockLocation,
    start_pointer: Point<Pixels>,
    start_share: f32,
    extent: f32,
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

impl KnotWindow {
    /// Open a window showing `workbench`, without a native window.
    fn new(workbench: Entity<Workbench>, cx: &mut Context<Self>) -> Self {
        cx.global::<ApplicationWindows>()
            .register(cx.weak_entity(), &workbench);
        document_watch::install(cx);
        #[cfg(test)]
        {
            let stored = Self::stored_tool_layout(cx);
            workbench.update(cx, |workbench, _| {
                let id = workbench.register_container(
                    crate::app::terminal::TERMINAL_TOOL_ID,
                    "Terminal",
                    crate::app::terminal::TERMINAL_SLOT,
                );
                workbench.place_registered_tool(id, stored.as_ref(), false);
            });
        }
        Self {
            workbench,
            native_window: None,
            tab_drag: Default::default(),
            dock_resize: None,
            slot_resize: None,
            stripe_menu: None,
            surface_menu: None,
            stripe_dragging: Default::default(),
            split_resize: None,
            recent_walk: None,
            maximized: None,
            hidden_maximized: Vec::new(),
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
            notices: Default::default(),
            application_focus: Vec::new(),
            contributed_views: BTreeMap::new(),
            contributed_containers: BTreeMap::new(),
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

    /// Bring the native window to the front once the current update ends.
    fn activate_window(&self, cx: &mut Context<Self>) {
        let Some(native_window) = self.native_window else {
            #[cfg(any(test, feature = "test-support"))]
            {
                cx.global::<ApplicationWindows>().activate(cx.entity_id());
                super::extensions::host::application_changed(cx);
            }
            return;
        };
        cx.defer(move |cx| {
            let _ = native_window.update(cx, |_, window, _| window.activate_window());
        });
    }

    /// The modifier keys held in the native window; none without one.
    fn modifiers(&self, cx: &mut App) -> Modifiers {
        self.native_window
            .and_then(|native_window| {
                native_window
                    .update(cx, |_, window, _| window.modifiers())
                    .ok()
            })
            .unwrap_or_default()
    }

    /// Ask with a native prompt in the native window once the current
    /// update ends, so a caller may hold the window or its native window.
    pub(super) fn prompt(
        &self,
        level: PromptLevel,
        title: impl Into<String>,
        detail: Option<String>,
        buttons: Vec<PromptButton>,
        cx: &mut App,
    ) -> PromptAnswer {
        prompt(self.native_window, level, title.into(), detail, buttons, cx)
    }

    /// Orders windows by creation.
    pub(crate) fn creation_order(&self) -> u64 {
        self.created
    }

    /// Show the window in `native_window`, which focuses what the window has
    /// focused. Closing the native window ends the window: its live frames and
    /// pending transient interaction go, while other windows keep theirs.
    pub(super) fn attach_window(&mut self, native_window: &mut Window, cx: &mut Context<Self>) {
        if self.native_window.is_some() {
            return;
        }
        let handle = native_window.window_handle();
        self.native_window = Some(handle);
        cx.global::<ApplicationWindows>()
            .attach(cx.entity_id(), handle);
        #[cfg(any(test, feature = "test-support"))]
        TEST_WINDOW_IDS.with_borrow_mut(|ids| {
            ids.insert(handle.window_id().as_u64(), window_id(cx.entity_id()))
        });
        let window = cx.weak_entity();
        self._window_closed = Some(cx.on_window_closed(move |cx, _| {
            let window = window.clone();
            cx.defer(move |cx| {
                let Some(window) = window.upgrade() else {
                    return;
                };
                if cx.windows().contains(&handle) {
                    return;
                }
                window.update(cx, |this, cx| this.end(cx));
            });
        }));
        self.follow_focus(cx);
    }

    /// Close the window without its native window.
    fn end(&mut self, cx: &mut Context<Self>) {
        cx.global::<ApplicationWindows>().close(cx.entity_id());
        #[cfg(any(test, feature = "test-support"))]
        if let Some(headless) = cx.try_global::<HeadlessWindows>() {
            let id = cx.entity_id();
            headless
                .0
                .borrow_mut()
                .retain(|window| window.entity_id() != id);
        }
        let ids = self
            .contributed_views
            .keys()
            .chain(self.contributed_containers.keys())
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            self.detach_contributed_view(&id, cx);
        }
        self.finish_transient(TransientOutcome::Cancelled, cx);
    }

    /// Close the native window, which ends the window, or end a window
    /// without one.
    fn close_window(&mut self, cx: &mut Context<Self>) -> Result<(), ProtectedCloseFailure> {
        let Some(native_window) = self.native_window else {
            self.end(cx);
            return Ok(());
        };
        cx.update_window(native_window, |_, window, cx| {
            tab_drag::cancel_source_window(window, cx);
            window.remove_window();
        })
        .map_err(|_| ProtectedCloseFailure::InvalidTarget)
    }

    /// Show the window in `native_window` and report it to extensions.
    fn observe_extension_host(&mut self, native_window: &mut Window, cx: &mut Context<Self>) {
        self.attach_window(native_window, cx);
        self.observe_window_bounds(native_window, cx);
        self.application_activation = Some(cx.observe_window_activation(
            native_window,
            |_, window, cx| {
                super::extensions::host::application_changed(cx);
                if window.is_window_active() {
                    cx.global::<ApplicationWindows>().activate(cx.entity_id());
                    document_watch::recheck_all(cx);
                }
            },
        ));
        self._appearance_observer = Some(
            native_window.observe_window_appearance(|_, cx| super::themes::selection_changed(cx)),
        );
        // The bar and settings change only what the window paints, so they
        // redraw it without notifying the window, which extensions observe.
        self._status_bar_observers = vec![
            cx.observe_global_in::<super::status_bar::ApplicationStatusBar>(
                native_window,
                |_, window, _| window.refresh(),
            ),
            cx.observe_global_in::<super::settings::ApplicationSettings>(
                native_window,
                |this, window, cx| this.sync_notices(window, cx),
            ),
            cx.observe_global_in::<super::notifications::ApplicationNotifications>(
                native_window,
                |this, window, cx| this.sync_notices(window, cx),
            ),
        ];
        self.sync_notices(native_window, cx);
        self.observe_application(cx);
    }

    /// Report the window to extensions as it changes and show their
    /// contributed frames, which needs no native window.
    pub(crate) fn observe_application(&mut self, cx: &mut Context<Self>) {
        #[cfg(test)]
        if cx.has_global::<tools::TestToolInEveryWindow>() {
            self.ensure_test_tool(false, cx);
        }
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
                let frames = std::mem::take(&mut this.contributed_views)
                    .into_keys()
                    .chain(std::mem::take(&mut this.contributed_containers).into_keys());
                for id in frames {
                    if let Some(frame) = this.workbench.read(cx).tool_by_stable_id(&id) {
                        this.workbench
                            .update(cx, |workbench, _| workbench.unregister_tool(frame));
                    }
                }
            }));
            self.sync_contributed_views(cx);
        }
    }

    /// Make this window the active one while no native window is active, as
    /// focusing its native window would.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn activate(&mut self, cx: &mut Context<Self>) {
        cx.global::<ApplicationWindows>().activate(cx.entity_id());
        super::extensions::host::application_changed(cx);
    }

    fn observe_application_focus(&mut self, native_window: &mut Window, cx: &mut Context<Self>) {
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
            if handle.contains_focused(native_window, cx) {
                self.focus_view(&handle, cx);
            }
            self.application_focus.push((
                handle.clone(),
                cx.on_focus_in(&handle, native_window, {
                    let handle = handle.clone();
                    move |this, _, cx| {
                        this.focus_view(&handle, cx);
                        super::extensions::host::application_changed(cx);
                        this.schedule_view_state(cx);
                    }
                }),
                cx.on_blur(&handle, native_window, |_, _, cx| {
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
        // A menu inside the view holds focus there, so the layer stays.
        if self.focus != Focus::Popup {
            self.focus = Focus::Workbench;
        }
        self.workbench.update(cx, |workbench, _| {
            workbench.focus_pane(pane);
        });
        if document.is_some() {
            self.last_focused_document = document;
            self.last_focused_editor_tab = Some(tab);
        }
        self.follow_focus(cx);
    }

    /// The name a terminal session has without a title, which default labels
    /// are made unique against.
    fn terminal_session_name(id: TerminalSessionId, cx: &App) -> Option<String> {
        let sessions = cx.global::<ApplicationTerminalSessions>().0.borrow();
        sessions
            .sessions
            .get(&id)
            .map(|session| session.read(cx).name().to_owned())
    }

    /// The label of a terminal session's tab.
    fn terminal_session_label(id: TerminalSessionId, cx: &App) -> Option<String> {
        let sessions = cx.global::<ApplicationTerminalSessions>().0.borrow();
        sessions
            .sessions
            .get(&id)
            .map(|session| session.read(cx).label().to_owned())
    }

    /// Name a terminal session's tab, winning over its program's title.
    fn set_terminal_name(id: TerminalSessionId, name: String, cx: &mut App) {
        let session = cx
            .global::<ApplicationTerminalSessions>()
            .0
            .borrow()
            .sessions
            .get(&id)
            .cloned();
        if let Some(session) = session {
            session.update(cx, |session, _| session.rename(name));
        }
    }

    fn terminal_facts(id: TerminalSessionId, cx: &App) -> crate::app::window_layout::TerminalFacts {
        crate::app::window_layout::TerminalFacts {
            directory: Self::terminal_session_directory(id, cx),
            name: Self::terminal_session_explicit_name(id, cx),
        }
    }

    fn terminal_session_explicit_name(id: TerminalSessionId, cx: &App) -> Option<String> {
        let sessions = cx.global::<ApplicationTerminalSessions>().0.borrow();
        sessions
            .sessions
            .get(&id)
            .and_then(|session| session.read(cx).explicit_name().map(str::to_owned))
    }

    fn terminal_session_directory(id: TerminalSessionId, cx: &App) -> Option<std::path::PathBuf> {
        let sessions = cx.global::<ApplicationTerminalSessions>().0.borrow();
        sessions
            .sessions
            .get(&id)
            .and_then(|session| session.read(cx).current_directory())
    }

    /// The directory a terminal starts in when its opener names none: the
    /// first local workspace root, else the home directory.
    fn default_terminal_directory(&self) -> Option<std::path::PathBuf> {
        self.root_uris()
            .iter()
            .find_map(|root| root.as_url().to_file_path().ok())
            .filter(|directory| directory.is_dir())
            .or_else(|| directories::BaseDirs::new().map(|base| base.home_dir().to_owned()))
    }

    pub(crate) fn application_window(
        &self,
        window: EntityId,
        buffers: &std::collections::HashMap<DocumentId, knot_protocol::BufferHandle>,
        cx: &App,
    ) -> knot_protocol::WindowInfo {
        use knot_protocol::{
            PaneInfo, PaneTarget, PresentationInfo, PresentationTarget, SurfaceKind, WindowInfo,
            WindowTarget,
        };
        let target = WindowTarget {
            window: window_id(window),
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
                            directory: match tab.surface_id() {
                                TabSurfaceId::Terminal(id) => {
                                    Self::terminal_session_directory(id, cx).and_then(|path| {
                                        url::Url::from_directory_path(path)
                                            .ok()
                                            .map(|url| url.to_string())
                                    })
                                }
                                TabSurfaceId::Document(_) | TabSurfaceId::Tool(_) => None,
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
            last_focused_document: self
                .last_focused_document
                .and_then(|id| documents.get(id))
                .map(|document| document.identity().to_string()),
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
        self.create_terminal_in(self.default_terminal_directory(), cx)
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
        let taken = self
            .workbench
            .read(cx)
            .tabs()
            .filter_map(|tab| match tab.surface_id() {
                TabSurfaceId::Terminal(id) => Self::terminal_session_name(id, cx),
                _ => None,
            })
            .collect::<Vec<_>>();
        let name = knot_model::workbench::free_label(session.read(cx).name(), &taken);
        session.update(cx, |session, _| session.set_default_name(name));
        let view = cx.new(|cx| TerminalView::new(session.clone(), cx));
        // The subscription lives as long as the session, whichever window
        // holds its tab.
        App::subscribe(cx, &session, move |_, event, cx| match event {
            TerminalSessionEvent::Bell => Self::ring_terminal_bell(id, cx),
            TerminalSessionEvent::Notification { title, body } => {
                Self::raise_terminal_notice(id, title.clone(), body.clone(), cx)
            }
        })
        .detach();
        cx.global::<ApplicationTerminalSessions>()
            .0
            .borrow_mut()
            .sessions
            .insert(id, session);
        (id, view)
    }

    /// Mark the tab of terminal `id` in the window that holds it.
    fn ring_terminal_bell(id: TerminalSessionId, cx: &mut App) {
        for window in windows_by_activation(cx) {
            let workbench = window.read(cx).workbench.clone();
            if workbench.update(cx, |workbench, _| workbench.mark_bell(id)) {
                window.update(cx, |_, cx| cx.notify());
            }
        }
    }

    /// Raise a notice a terminal program asked for in the window that holds
    /// it. Without a title of its own, the notice carries the tab's label.
    fn raise_terminal_notice(id: TerminalSessionId, title: String, body: String, cx: &mut App) {
        let Some(window) = windows_by_activation(cx).into_iter().find(|window| {
            window
                .read(cx)
                .workbench
                .read(cx)
                .terminal_tab(id)
                .is_some()
        }) else {
            return;
        };
        let title = if title.is_empty() {
            Self::terminal_session_label(id, cx).unwrap_or_default()
        } else {
            title
        };
        let action = NoticeAction {
            title: "Show Terminal".into(),
            command: knot_model::status_bar::StatusCommand {
                name: crate::app::commands::dispatch::SHOW_TERMINAL_COMMAND.into(),
                arguments: commands::TerminalArgument::argument(id),
            },
        };
        window.update(cx, |window, cx| {
            window.raise_notice(Severity::Info, title, Some(body), vec![action], cx)
        });
    }

    /// Show a terminal's tab, revealing its tool window, and focus it.
    pub(super) fn reveal_terminal(
        &mut self,
        id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) -> Result<(), knot_protocol::CommandOutcome> {
        let (pane, tab) = self
            .workbench
            .read(cx)
            .terminal_tab(id)
            .ok_or(knot_protocol::CommandOutcome::InvalidTarget)?;
        self.activate_tab(pane, tab, cx);
        self.publish_tool_layout(cx);
        Ok(())
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

    /// Return focus to the focused pane's active surface when a context menu
    /// closes, unless focus has moved on to another layer.
    pub(super) fn close_popup_focus(&mut self, cx: &mut Context<Self>) {
        if self.focus == Focus::Popup {
            self.focus_active_surface(cx);
        }
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
            // The menu holds native focus itself.
            Focus::Popup => None,
        }
    }

    /// Have the attached native window focus what has Knot-side focus once the
    /// current update ends, so a caller may hold the window.
    fn follow_focus(&mut self, cx: &mut Context<Self>) {
        let Some(native_window) = self.native_window else {
            // Without a native window nothing reports the focus change back.
            #[cfg(any(test, feature = "test-support"))]
            self.record_focused_tab(cx);
            return;
        };
        let window = cx.weak_entity();
        cx.defer(move |cx| {
            let Some(handle) = window
                .upgrade()
                .and_then(|window| window.read(cx).focused_handle(cx))
            else {
                return;
            };
            let _ = native_window.update(cx, |_, window, cx| {
                // Focus inside the view, such as in its menu, stays there.
                if !handle.contains_focused(window, cx) {
                    window.focus(&handle, cx);
                }
            });
        });
    }

    /// Remember the focused editor tab, as a native window's focus event does.
    #[cfg(any(test, feature = "test-support"))]
    fn record_focused_tab(&mut self, cx: &App) {
        if self.focus != Focus::Workbench {
            return;
        }
        let Some(pane) = self.workbench.read(cx).focused_pane() else {
            return;
        };
        if let Some(document) = pane.active_tab().document_id() {
            self.last_focused_document = Some(document);
            self.last_focused_editor_tab = Some(pane.active_tab_id());
        }
    }

    /// The focused pane's active tab, with whether its view has focus.
    pub(crate) fn capture_command_target(
        &mut self,
        cx: &mut Context<Self>,
    ) -> ProductCommandTarget {
        let workbench = self.workbench.read(cx);
        let focused = matches!(self.focus, Focus::Workbench | Focus::Popup);
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
            window: cx.entity().downgrade(),
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

    /// Run the command of a clicked contribution against the view that was
    /// clicked, with the contribution's current `range` added to its
    /// arguments.
    fn run_contribution_action(
        &mut self,
        action: &super::editor::EditorContributionAction,
        cx: &mut Context<Self>,
    ) {
        let Some(focus) = action.focus.upgrade() else {
            return;
        };
        let workbench = self.workbench.read(cx);
        let clicked = workbench.panes().iter().find_map(|pane| {
            pane.tabs().iter().find_map(|tab| {
                let view = workbench.command_view(tab)?;
                (view.focus_handle(cx) == focus).then(|| (pane.id(), tab, view))
            })
        });
        let Some((pane, tab, view)) = clicked else {
            return;
        };
        let target = ProductCommandTarget {
            window: cx.entity().downgrade(),
            workbench: self.workbench.downgrade(),
            tab: Some(TargetTab {
                pane,
                tab: tab.id(),
                surface: tab.surface_id(),
                presentation: tab.presentation(),
            }),
            focused: true,
            view: match &view {
                super::workbench::CommandView::Extension(extension) => {
                    Some(extension.instance_id(cx))
                }
                _ => None,
            },
        };
        let mut arguments = action.command.arguments.clone();
        arguments.insert(
            "range".to_owned(),
            knot_protocol::CommandArgumentValue::Object(
                [
                    (
                        "startByteOffset".to_owned(),
                        knot_protocol::CommandArgumentValue::Number(
                            action.range.start_byte_offset as f64,
                        ),
                    ),
                    (
                        "endByteOffset".to_owned(),
                        knot_protocol::CommandArgumentValue::Number(
                            action.range.end_byte_offset as f64,
                        ),
                    ),
                ]
                .into(),
            ),
        );
        let command = knot_protocol::Command {
            name: action.command.name.as_str().into(),
            arguments: knot_protocol::CommandArgumentValue::Object(arguments),
        };
        self.dispatch_command(command, target, cx);
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

    pub(crate) fn dispatch_source(
        &mut self,
        command: knot_protocol::Command,
        cx: &mut Context<Self>,
    ) {
        let target = self.capture_command_target(cx);
        self.dispatch_command(command, target, cx);
    }

    #[cfg(test)]
    fn open_command_palette(&mut self, cx: &mut Context<Self>) {
        let origin = self.capture_command_target(cx);
        self.open_command_palette_from(origin, cx);
    }

    /// The keymap view kind of a captured target's focused tab; contributed
    /// frames and native tools share this path instead of a window-level
    /// special case.
    fn target_view_kind(target: &ProductCommandTarget, cx: &App) -> Option<String> {
        target
            .command_view(cx)
            .filter(|_| target.focused)
            .map(|view| Self::command_view_kind(&view, cx))
    }

    fn command_view_kind(view: &super::workbench::CommandView, cx: &App) -> String {
        match view {
            super::workbench::CommandView::Editor(view) => {
                if view.read(cx).editor().read(cx).bars().focused().is_some() {
                    "bar".to_owned()
                } else {
                    "editor".to_owned()
                }
            }
            super::workbench::CommandView::Terminal(_) => "terminal".to_owned(),
            super::workbench::CommandView::Extension(view) => view.kind(cx),
        }
    }

    /// Runs Search Everywhere on its Actions tab when that command is
    /// registered, else opens the fallback palette.
    pub(crate) fn show_command_palette_from(
        &mut self,
        origin: ProductCommandTarget,
        cx: &mut Context<Self>,
    ) {
        let search = Self::command_dispatcher(cx)
            .read(cx)
            .catalog()
            .borrow()
            .resolve(SEARCH_EVERYWHERE_COMMAND)
            .is_ok();
        if !search {
            self.open_command_palette_from(origin, cx);
            return;
        }
        let command = knot_protocol::Command {
            name: SEARCH_EVERYWHERE_COMMAND.into(),
            arguments: knot_protocol::CommandArgumentValue::Object(
                [(
                    "tab".to_owned(),
                    knot_protocol::CommandArgumentValue::String("actions".to_owned()),
                )]
                .into(),
            ),
        };
        self.dispatch_command(command, origin, cx);
    }

    fn open_command_palette_from(&mut self, origin: ProductCommandTarget, cx: &mut Context<Self>) {
        let definitions = Self::command_dispatcher(cx)
            .read(cx)
            .definitions()
            .collect::<Vec<_>>();
        let palette = CommandPalette::new(definitions, origin, cx);
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
                this.dispatch_holding_keys(command, palette.origin(), cx);
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
        let accepted = matches!(
            outcome,
            TransientOutcome::Picker(_) | TransientOutcome::Input(..)
        );
        active.settle(outcome);
        if active.focus.is_some() {
            if accepted && let Some(invocation) = active.owner.and_then(|owner| owner.invocation) {
                self.hold_keys_after_accepting(invocation, active.restore_focus, cx);
            }
            self.set_focus(active.restore_focus, cx);
        }
        cx.notify();
    }

    /// Hold later input until the command whose interaction was accepted
    /// settles, so keys typed after Enter reach what it opens. The held view
    /// is the one focus returns to.
    fn hold_keys_after_accepting(
        &mut self,
        invocation: knot_protocol::CommandInvocationId,
        restore_focus: Focus,
        cx: &mut Context<Self>,
    ) {
        let focus = FocusTarget {
            layer: restore_focus,
            ..self.focus_target(cx)
        };
        let editor = match (
            restore_focus,
            self.workbench.read(cx).focused_command_view(),
        ) {
            (Focus::Workbench, Some(super::workbench::CommandView::Editor(view))) => {
                Some(view.read(cx).editor().clone())
            }
            _ => None,
        };
        if !self.retarget_picker_hold(focus, editor.clone(), cx) {
            self.hold_keys_for_command(invocation, focus, editor, cx);
        }
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

    /// As [`Self::begin_picker`], with scope tabs.
    pub(crate) fn begin_tabbed_picker(
        &mut self,
        owner: TransientOwner,
        title: String,
        items: Vec<knot_protocol::PickerItem>,
        (tabs, initial_tab): (Vec<knot_protocol::PickerTab>, usize),
        highlights: Option<tokio::sync::watch::Sender<Option<String>>>,
        cx: &mut Context<Self>,
    ) -> Result<tokio::sync::oneshot::Receiver<TransientOutcome>, TransientError> {
        self.present_entries(
            Some(owner),
            title,
            items,
            (tabs, initial_tab),
            highlights,
            cx,
        )
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
        self.present_entries(owner, title, items, (Vec::new(), 0), highlights, cx)
    }

    fn present_entries(
        &mut self,
        owner: Option<TransientOwner>,
        title: String,
        items: Vec<knot_protocol::PickerItem>,
        tabs: (Vec<knot_protocol::PickerTab>, usize),
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
                selected: item.selected,
                tabs: item.tabs,
                ..PickerEntry::new(item.label)
            })
            .collect();
        self.present_picker(owner, title, (Some(entries), tabs), highlights, None, cx)
            .map(|(receiver, _)| receiver)
    }

    /// Present a lifecycle-authorized picker whose rows the owner supplies:
    /// each query with the active tab's ID, starting with the empty query,
    /// is published on `queries` and answered through the returned view.
    pub(crate) fn begin_queried_picker(
        &mut self,
        owner: TransientOwner,
        title: String,
        tabs: (Vec<knot_protocol::PickerTab>, usize),
        highlights: tokio::sync::watch::Sender<Option<String>>,
        queries: tokio::sync::watch::Sender<Option<PickerAsk>>,
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
            (None, tabs),
            Some(highlights),
            Some(queries),
            cx,
        )
    }

    /// Present a picker over `entries`, or with supplied rows when there are
    /// none, with scope `tabs`. Entries carry the IDs that outcomes and highlights name.
    fn present_picker(
        &mut self,
        owner: Option<TransientOwner>,
        title: String,
        (entries, (tabs, initial_tab)): (
            Option<Vec<PickerEntry>>,
            (Vec<knot_protocol::PickerTab>, usize),
        ),
        highlights: Option<tokio::sync::watch::Sender<Option<String>>>,
        queries: Option<tokio::sync::watch::Sender<Option<PickerAsk>>>,
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
        if !tabs.is_empty() {
            view.update(cx, |picker, cx| {
                picker.set_tabs(
                    tabs.into_iter()
                        .map(|tab| PickerTab {
                            id: tab.id,
                            label: tab.label,
                            placeholder: tab.placeholder,
                        })
                        .collect(),
                    initial_tab,
                    cx,
                )
            });
        }
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
            let picker = view.read(cx);
            queries.send_replace(Some((picker.query(cx), picker.active_tab_id())));
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
                    PickerEvent::Queried(query, tab) => {
                        if let Some(queries) = &queries {
                            queries.send_replace(Some((query.clone(), tab.clone())));
                        }
                        return;
                    }
                    PickerEvent::Pending => {
                        if let Some(invocation) = owner.and_then(|owner| owner.invocation) {
                            let focus = this.focus_target(cx);
                            this.hold_keys_for_command(invocation, focus, None, cx);
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
        spec: InputSpec,
        changes: Option<tokio::sync::watch::Sender<Option<String>>>,
        cx: &mut Context<Self>,
    ) -> Result<
        (
            tokio::sync::oneshot::Receiver<TransientOutcome>,
            WeakEntity<InputView>,
        ),
        TransientError,
    > {
        self.begin_owned_input(Some(owner), spec, changes, cx)
    }

    /// Present an input, owned by an extension or by the application.
    pub(crate) fn begin_owned_input(
        &mut self,
        owner: Option<TransientOwner>,
        spec: InputSpec,
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
        let view = cx.new(|cx| InputView::new(spec, changes, cx));
        let weak = view.downgrade();
        let focus = view.read(cx).focus_handle(cx);
        let subscription = cx.subscribe(&view, |this: &mut Self, _view, event: &InputEvent, cx| {
            this.finish_transient(
                match event {
                    InputEvent::Accepted { value, highlighted } => {
                        TransientOutcome::Input(value.clone(), *highlighted)
                    }
                    InputEvent::Dismissed => TransientOutcome::Dismissed,
                },
                cx,
            );
        });
        let (completion, receiver) = tokio::sync::oneshot::channel();
        self.transient = Some(ActiveTransient {
            owner,
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
        owner: Option<TransientOwner>,
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
            owner,
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
                Area::Tool(_) => Ok(workbench.center_pane()),
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

    /// Opens a focused terminal starting in `directory`, or the home
    /// directory. In a container tool window it opens beside the terminal in
    /// `pane` when that is in the container, else in its last focused pane; in
    /// the center, beside `pane` or in the center pane.
    fn new_terminal_in_pane(
        &mut self,
        pane: Option<PaneId>,
        location: &operations::TerminalLocation,
        directory: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) -> Option<OpenTarget> {
        enum Destination {
            Pane(PaneId),
            Tool(ToolFrameId),
            Center,
        }
        let destination = match location {
            operations::TerminalLocation::Container(stable_id) => {
                let workbench = self.workbench.read(cx);
                let tool = workbench.tool_by_stable_id(stable_id)?;
                let beside = pane.filter(|pane| {
                    workbench.pane_tool(*pane) == Some(tool)
                        && workbench.pane(*pane).is_some_and(|pane| {
                            matches!(pane.active_tab().surface_id(), TabSurfaceId::Terminal(_))
                        })
                });
                match beside {
                    Some(pane) => Destination::Pane(pane),
                    None => Destination::Tool(tool),
                }
            }
            operations::TerminalLocation::Center => match self.center_destination(pane, cx).ok()? {
                Some(pane) => Destination::Pane(pane),
                None => Destination::Center,
            },
        };
        let directory = directory.or_else(|| self.default_terminal_directory());
        let (id, view) = self.create_terminal_in(directory, cx);
        let Some((pane, tab)) = self.workbench.update(cx, |workbench, _| match destination {
            Destination::Pane(pane) => workbench
                .open_terminal_tab(pane, id, view)
                .map(|tab| (pane, tab)),
            Destination::Tool(tool) => workbench.open_terminal_in_tool(tool, id, view),
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
                .or_else(|| self.default_terminal_directory());
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

    /// Raise a notice in this window for an outcome nothing else on screen
    /// shows.
    pub(super) fn raise_notice(
        &self,
        severity: Severity,
        title: impl Into<String>,
        detail: Option<String>,
        actions: Vec<NoticeAction>,
        cx: &mut Context<Self>,
    ) {
        raise_notice(
            window_id(cx.entity_id()),
            severity,
            title.into(),
            detail,
            actions,
            cx,
        );
    }

    pub(crate) fn toggle_extension_report(&mut self, cx: &mut Context<Self>) {
        self.extension_report_open = !self.extension_report_open;
        cx.notify();
    }
}

/// The index of the button a prompt was answered with; an error when the
/// window has no native window or it closed first.
type PromptAnswer = tokio::sync::oneshot::Receiver<usize>;

/// Ask with a native prompt in the window's native window once the current
/// update ends, so a caller may hold the window or its native window.
fn prompt(
    native_window: Option<AnyWindowHandle>,
    level: PromptLevel,
    title: String,
    detail: Option<String>,
    buttons: Vec<PromptButton>,
    cx: &mut App,
) -> PromptAnswer {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    if let Some(native_window) = native_window {
        cx.defer(move |cx| {
            let Ok(answer) = native_window.update(cx, |_, window, cx| {
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

/// Raise a notice from Knot in the window with public ID `window`. The detail
/// becomes the message under the title; without one the title is the message.
pub(super) fn raise_notice(
    window: String,
    severity: Severity,
    title: String,
    detail: Option<String>,
    actions: Vec<NoticeAction>,
    cx: &mut App,
) {
    let (title, message) = match detail {
        Some(detail) => (Some(title), detail),
        None => (None, title),
    };
    crate::app::notifications::raise(
        NoticeSpec {
            message,
            title,
            severity,
            auto_dismiss: None,
            actions,
            target: Some(window),
            source: "Knot".into(),
        },
        cx,
    );
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
