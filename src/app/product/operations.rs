//! Destination admission and semantic product operations for extension requests.

use super::super::extensions::host::handlers::{
    AccessError, CloseError, OpenError, WorkspaceError,
};
use super::*;
use crate::app::documents::DocumentDirty as _;
use knot_protocol::ExtensionKey;
use knot_protocol::{
    CommandInvocationId, DocumentDestination, PaneTarget, PresentationTarget, WindowTarget,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone)]
pub(crate) struct OperationCaller {
    pub(crate) host: WeakEntity<super::super::extensions::host::ProductExtensionHost>,
    pub(crate) key: ExtensionKey,
    pub(crate) invocation: Option<CommandInvocationId>,
}

impl OperationCaller {
    pub(super) fn is_live(&self, cx: &App) -> bool {
        self.host
            .upgrade()
            .is_some_and(|host| host.read(cx).operation_is_live(self.key, self.invocation))
    }
}

#[derive(Default)]
pub(super) struct OpenOptions {
    pub(super) replace_target: bool,
    /// Present a newly shown document as the pane's preview tab.
    pub(super) preview: bool,
    /// Leave the focus where it is instead of moving it to the opened tab.
    pub(super) keep_focus: bool,
    /// Open the document without presenting it in any tab.
    pub(super) load_only: bool,
    pub(super) kind: Option<ResourceKind>,
    pub(super) caller: Option<OperationCaller>,
}

fn window_target(destination: &DocumentDestination) -> &WindowTarget {
    match destination {
        DocumentDestination::Window(window) => window,
        DocumentDestination::Pane(pane) => &pane.window,
        DocumentDestination::View(view) => &view.pane.window,
    }
}

/// The open session a window target names.
pub(super) fn capture_window(
    target: &WindowTarget,
    cx: &App,
) -> Result<Entity<KnotWindow>, AccessError> {
    cx.global::<ApplicationWindows>()
        .find(&target.window)
        .ok_or(AccessError::InvalidTarget)
}

pub(super) fn workspace_info(
    window: WindowTarget,
    snapshot: &super::super::workspace::WorkspaceSnapshot,
) -> knot_protocol::WorkspaceInfo {
    use knot_protocol::{WorkspaceInfo, WorkspaceRootInfo};
    WorkspaceInfo {
        window,
        generation: snapshot.generation(),
        roots: snapshot
            .roots()
            .iter()
            .map(|root| WorkspaceRootInfo {
                id: root.id.to_string(),
                uri: root.uri.to_string(),
            })
            .collect(),
    }
}

fn capture_destination(
    destination: &DocumentDestination,
    cx: &mut App,
) -> Result<(Entity<KnotWindow>, OpenTarget), AccessError> {
    let window = capture_window(window_target(destination), cx)?;
    let state = window.read(cx);
    let workbench = state.workbench.read(cx);
    let (pane, tab) = match destination {
        // A window with no focused pane opens in its center.
        DocumentDestination::Window(_) => match workbench.focused_pane() {
            Some(pane) => (pane, pane.active_tab()),
            None => {
                return Ok((
                    window.clone(),
                    OpenTarget {
                        window: window.downgrade(),
                        workbench: state.workbench.downgrade(),
                        tab: None,
                    },
                ));
            }
        },
        DocumentDestination::Pane(target)
        | DocumentDestination::View(PresentationTarget { pane: target, .. }) => {
            if target.workbench != state.workbench.entity_id().as_u64().to_string() {
                return Err(AccessError::InvalidTarget);
            }
            let pane = workbench
                .panes()
                .iter()
                .find(|pane| pane.id().value().to_string() == target.pane)
                .ok_or(AccessError::InvalidTarget)?;
            let tab = match destination {
                DocumentDestination::View(target) => pane
                    .tabs()
                    .iter()
                    .find(|tab| tab.id().value().to_string() == target.tab)
                    .ok_or(AccessError::InvalidTarget)?,
                _ => pane.active_tab(),
            };
            (pane, tab)
        }
    };
    if let DocumentDestination::View(target) = destination
        && tab.presentation().value().to_string() != target.presentation
    {
        return Err(AccessError::InvalidTarget);
    }
    let target = OpenTarget::at(
        window.downgrade(),
        &state.workbench,
        pane.id(),
        tab.id(),
        cx,
    );
    Ok((window.clone(), target))
}

impl OpenTarget {
    /// The public identity of the tab an open landed in.
    pub(super) fn public_target(&self) -> PresentationTarget {
        let tab = self.tab.expect("opened targets name their tab");
        PresentationTarget {
            pane: PaneTarget {
                window: WindowTarget {
                    window: window_id(self.window.entity_id()),
                },
                workbench: self.workbench.entity_id().as_u64().to_string(),
                pane: tab.pane.value().to_string(),
            },
            tab: tab.tab.value().to_string(),
            presentation: tab.presentation.value().to_string(),
        }
    }
}

impl OpenFailure {
    fn host_error(self) -> OpenError {
        match self {
            Self::InvalidTarget => OpenError::Access(AccessError::InvalidTarget),
            Self::Cancelled => OpenError::Access(AccessError::Cancelled),
            Self::Resource(error) => resource_error(error),
        }
    }
}

pub(crate) fn resource_error(error: super::super::open::OpenResourceError) -> OpenError {
    use super::super::open::OpenResourceError;
    match error {
        OpenResourceError::InvalidUtf8 { .. } => OpenError::InvalidUtf8,
        OpenResourceError::Provider(error) => match error {
            ResourceError::InvalidUri { .. } => OpenError::InvalidUri,
            ResourceError::NotFound { .. } => OpenError::ResourceNotFound,
            ResourceError::WrongKind { .. } => OpenError::ResourceWrongKind,
            ResourceError::AlreadyExists { .. } => OpenError::ResourceAlreadyExists,
            _ => OpenError::ProviderFailure,
        },
    }
}

/// Open documents follow a rename the application performed.
pub(crate) fn follow_rename(
    from: &super::super::resource::ResourceUri,
    to: &super::super::resource::ResourceUri,
    cx: &mut App,
) {
    let documents = cx.global::<ApplicationDocuments>().0.clone();
    documents.update(cx, |documents, cx| {
        documents.follow_rename(from, to, |uri| super::resource_title(uri).into(), cx)
    });
}

/// Open documents learn that the application deleted their file.
pub(crate) fn follow_delete(uri: &super::super::resource::ResourceUri, cx: &mut App) {
    let documents = cx.global::<ApplicationDocuments>().0.clone();
    documents.update(cx, |documents, cx| documents.follow_delete(uri, cx));
}

/// Resolve a user-selected path to a normalized, existing directory URI.
pub(crate) fn resolve_directory(
    path: std::path::PathBuf,
    completion: impl FnOnce(Result<super::super::resource::ResourceUri, WorkspaceError>) + 'static,
    cx: &mut App,
) {
    let providers = KnotWindow::filesystems(cx);
    cx.spawn(async move |_| {
        let result = async {
            let request = OpenRequest::from_path(&path, Path::new("/"))
                .map_err(|_| WorkspaceError::Resource(OpenError::InvalidUri))?;
            match super::super::open::load_resource_kind(
                providers,
                request.uri().clone(),
                Some(ResourceKind::Directory),
            )
            .await
            {
                Ok(OpenResource::Directory(uri)) => Ok(uri),
                Ok(_) => Err(WorkspaceError::Resource(OpenError::ResourceWrongKind)),
                Err(error) => Err(WorkspaceError::Resource(
                    OpenFailure::Resource(error).host_error(),
                )),
            }
        }
        .await;
        completion(result);
    })
    .detach();
}

/// Resolves an extension-supplied terminal directory: an existing local
/// directory, checked through the registered providers.
pub(crate) fn resolve_terminal_directory(
    uri: String,
    completion: impl FnOnce(Result<std::path::PathBuf, OpenError>) + 'static,
    cx: &mut App,
) -> Result<(), OpenError> {
    let uri =
        super::super::resource::ResourceUri::parse(&uri).map_err(|_| OpenError::InvalidUri)?;
    if uri.scheme() != "file" {
        return Err(OpenError::InvalidUri);
    }
    let providers = KnotWindow::filesystems(cx);
    cx.spawn(async move |_| {
        let result = match super::super::open::load_resource_kind(
            providers,
            uri,
            Some(ResourceKind::Directory),
        )
        .await
        {
            Ok(OpenResource::Directory(uri)) => uri
                .as_url()
                .to_file_path()
                .map_err(|()| OpenError::InvalidUri),
            Ok(_) => Err(OpenError::ResourceWrongKind),
            Err(error) => Err(OpenFailure::Resource(error).host_error()),
        };
        completion(result);
    })
    .detach();
    Ok(())
}

/// Where a new terminal opens.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TerminalLocation {
    /// Beside the terminal it is opened from when that is in the container
    /// tool window with this stable ID, else in the container's last focused
    /// pane, else as its first tab.
    Container(String),
    /// In the last focused center pane.
    Center,
}

/// Opens a focused terminal at `destination`, starting in `directory` or, when
/// it is omitted, in the home directory.
pub(crate) fn open_terminal(
    destination: DocumentDestination,
    directory: Option<std::path::PathBuf>,
    location: &TerminalLocation,
    cx: &mut App,
) -> Result<PresentationTarget, AccessError> {
    let (window, target) = capture_destination(&destination, cx)?;
    window.update(cx, |window, cx| {
        window
            .new_terminal_in_pane(target.tab.map(|tab| tab.pane), location, directory, cx)
            .map(|target| target.public_target())
            .ok_or(AccessError::InvalidTarget)
    })
}

/// Open a composite view of `layout` in the center of the destination's
/// window, returning its target, the window's ID, and the contributed views
/// it holds.
pub(crate) fn open_composite(
    destination: DocumentDestination,
    tab: crate::app::composite_view::CompositeTab,
    layout: crate::app::view_layout::GroupSpec,
    preview: bool,
    cx: &mut App,
) -> Result<
    (
        PresentationTarget,
        String,
        Vec<crate::app::workbench::ExtensionView>,
    ),
    OpenError,
> {
    if let Some(link) = &tab.link
        && !link.joins(|key| layout.has_editor_key(key))
    {
        return Err(AccessError::InvalidTarget.into());
    }
    let (window, _) = capture_destination(&destination, cx)?;
    let id = super::window_id(window.entity_id());
    let (target, hosted) = window.update(cx, |window, cx| {
        window.open_composite(tab, layout, preview, cx)
    });
    Ok((target.public_target(), id, hosted))
}

/// Make the preview tab of the composite view `target` names regular.
pub(crate) fn keep_composite(target: &PresentationTarget, cx: &mut App) -> Result<(), OpenError> {
    let (window, open) = capture_destination(&DocumentDestination::View(target.clone()), cx)?;
    let location = open.tab.ok_or(AccessError::InvalidTarget)?;
    window.update(cx, |window, cx| {
        window.workbench.update(cx, |workbench, _| {
            workbench.promote_preview(location.pane, location.tab)
        });
        cx.notify();
    });
    Ok(())
}

/// Make the tab of the composite view `target` names the active one of its
/// pane, and focus it.
pub(crate) fn activate_composite(
    target: &PresentationTarget,
    cx: &mut App,
) -> Result<(), OpenError> {
    let (window, open) = capture_destination(&DocumentDestination::View(target.clone()), cx)?;
    let location = open.tab.ok_or(AccessError::InvalidTarget)?;
    window.update(cx, |window, cx| {
        window.activate_tab(location.pane, location.tab, cx)
    });
    Ok(())
}

/// Replace what the tab of the composite view `target` names shows, and its
/// layout when given. Returns the window's ID and the contributed views the
/// new layout added.
///
/// A document the new layout drops stays open while another tab shows it. One
/// whose view this was refuses the change when it has unsaved changes, and is
/// closed otherwise.
pub(crate) fn update_composite(
    target: &PresentationTarget,
    tab: crate::app::composite_view::CompositeTab,
    layout: Option<crate::app::view_layout::GroupSpec>,
    cx: &mut App,
) -> Result<(String, Vec<crate::app::workbench::ExtensionView>), OpenError> {
    let (window, open) = capture_destination(&DocumentDestination::View(target.clone()), cx)?;
    let workbench = open.workbench.upgrade().ok_or(AccessError::InvalidTarget)?;
    let location = open.tab.ok_or(AccessError::InvalidTarget)?;
    let new_link = tab.link.clone();
    let (view, dropped) = {
        let workbench = workbench.read(cx);
        let tab = workbench
            .tab(location.pane, location.tab)
            .ok_or(AccessError::InvalidTarget)?;
        let view = workbench
            .composite_view(tab)
            .cloned()
            .ok_or(AccessError::InvalidTarget)?;
        if let Some(link) = &new_link
            && !link.joins(|key| match &layout {
                Some(layout) => layout.has_editor_key(key),
                None => view.read(cx).has_editor_key(key, cx),
            })
        {
            return Err(AccessError::InvalidTarget.into());
        }
        let kept = layout.as_ref().map(|layout| layout.documents());
        let dropped: Vec<DocumentId> = workbench
            .shown_documents(tab)
            .into_iter()
            .filter(|document| kept.as_ref().is_some_and(|kept| !kept.contains(document)))
            .collect();
        (view, dropped)
    };
    // The tab is each of these documents' only view when no other shows it.
    let finals: Vec<DocumentId> = dropped
        .into_iter()
        .filter(|document| cx.global::<ApplicationWindows>().view_count(*document, cx) == 1)
        .collect();
    let documents = KnotWindow::documents(cx);
    if finals.iter().any(|document| {
        documents
            .read(cx)
            .get(*document)
            .is_some_and(|document| document.is_dirty(cx))
    }) {
        return Err(OpenError::DocumentUnsaved);
    }
    let id = super::window_id(window.entity_id());
    let hosted = match layout {
        Some(layout) => window.update(cx, |window, cx| {
            window.update_composite_layout(location.pane, location.tab, layout, cx)
        }),
        None => Vec::new(),
    };
    for document in finals {
        documents.update(cx, |documents, cx| {
            documents.remove(document, cx);
        });
    }
    view.update(cx, |view, cx| view.set_tab(tab, cx));
    window.update(cx, |_, cx| cx.notify());
    crate::app::extensions::host::application_changed(cx);
    Ok((id, hosted))
}

/// Replace the layout and toolbar of the layout tool window `view` of the
/// window `window`, each when given. Returns the contributed views the new
/// layout added.
pub(crate) fn update_layout_view(
    window: &str,
    view: &Entity<crate::app::layout_view::LayoutView>,
    layout: Option<crate::app::view_layout::GroupSpec>,
    toolbar: Option<Vec<crate::app::view_layout::ToolbarButton>>,
    cx: &mut App,
) -> Vec<crate::app::workbench::ExtensionView> {
    let Some(window) = super::find_window(window, cx) else {
        return Vec::new();
    };
    window.update(cx, |window, cx| {
        window.update_layout_view(view, layout, toolbar, cx)
    })
}

/// Close the tab `presentation` names with the usual prompts, for an owner
/// that is gone.
pub(crate) fn close_presentation_unattended(presentation: PresentationTarget, cx: &mut App) {
    let Ok((window, target)) = capture_destination(&DocumentDestination::View(presentation), cx)
    else {
        return;
    };
    let origin = ProductCommandTarget {
        window: window.downgrade(),
        workbench: target.workbench,
        tab: target.tab,
        focused: true,
        view: None,
        leaf: None,
    };
    KnotWindow::begin_protected_operation(
        ProtectedCloseRequest {
            kind: ProtectedCloseKind::Tab,
            origin,
            caller: None,
            discard: false,
        },
        Box::new(|_, _| {}),
        cx,
    );
}

/// An exact editor view retained beyond one command. It resolves only while
/// the view is in a tab's view tree, and never follows focus or a
/// replacement.
#[derive(Clone)]
pub(crate) struct EditorViewHandle {
    window: WeakEntity<KnotWindow>,
    view: String,
}

impl EditorViewHandle {
    /// The ID of the window the view is in.
    pub(crate) fn window_id(&self) -> Option<String> {
        Some(super::window_id(self.window.upgrade()?.entity_id()))
    }

    /// The view's editor, while the view is live.
    pub(crate) fn editor(&self, cx: &App) -> Option<Entity<crate::app::editor::Editor>> {
        let window = self.window.upgrade()?;
        let workbench = window.read(cx).workbench.clone();
        if !window_is_live(&self.window, &workbench, cx) {
            return None;
        }
        let workbench = workbench.read(cx);
        workbench
            .panes()
            .iter()
            .flat_map(|pane| pane.tabs())
            .filter_map(|tab| workbench.view_node(tab, cx))
            .find_map(|root| {
                root.descendants()
                    .into_iter()
                    .find(|node| node.id.as_u64().to_string() == self.view)
                    .and_then(|node| node.editor.clone())
            })
            .map(|view| view.read(cx).editor().clone())
    }
}

/// Bind the editor view `view` of `window` for the caller.
pub(crate) fn capture_editor_view(
    window: &WindowTarget,
    view: &str,
    cx: &mut App,
) -> Result<EditorViewHandle, AccessError> {
    let handle = EditorViewHandle {
        window: capture_window(window, cx)?.downgrade(),
        view: view.to_owned(),
    };
    handle.editor(cx).ok_or(AccessError::InvalidTarget)?;
    Ok(handle)
}

/// The keymap view kind of the tab a presentation target names.
pub(crate) fn view_kind(target: &PresentationTarget, cx: &mut App) -> Result<String, AccessError> {
    let (window, open) = capture_destination(&DocumentDestination::View(target.clone()), cx)?;
    let state = window.read(cx);
    let workbench = state.workbench.read(cx);
    let tab = open.tab.expect("a view destination names its tab");
    let view = workbench
        .tab(tab.pane, tab.tab)
        .and_then(|tab| workbench.command_view(tab))
        .ok_or(AccessError::InvalidTarget)?;
    Ok(KnotWindow::command_view_kind(&view, cx))
}

pub(crate) fn validate_presentation(
    target: &PresentationTarget,
    cx: &mut App,
) -> Result<(), AccessError> {
    capture_destination(&DocumentDestination::View(target.clone()), cx).map(|_| ())
}

pub(crate) fn open_document(
    uri: String,
    destination: DocumentDestination,
    preview: bool,
    focus: bool,
    caller: OperationCaller,
    completion: impl FnOnce(Result<(DocumentId, PresentationTarget), OpenError>, &mut App) + 'static,
    cx: &mut App,
) -> Result<(), OpenError> {
    let uri =
        super::super::resource::ResourceUri::parse(&uri).map_err(|_| OpenError::InvalidUri)?;
    let (window, target) = capture_destination(&destination, cx)?;
    window.update(cx, |window, cx| {
        window.start_resource_operation(
            OpenRequest::from_uri_for_product(uri),
            target,
            OpenOptions {
                kind: Some(ResourceKind::File),
                preview,
                keep_focus: !focus,
                caller: Some(caller),
                ..Default::default()
            },
            move |result, cx| {
                completion(
                    result
                        .map_err(OpenFailure::host_error)
                        .and_then(|commit| match commit {
                            OpenCommit::Document { document, target } => {
                                Ok((document, target.public_target()))
                            }
                            OpenCommit::Workspace(_) | OpenCommit::Loaded(_) => {
                                Err(OpenError::ResourceWrongKind)
                            }
                        }),
                    cx,
                )
            },
            cx,
        )
    });
    Ok(())
}

/// Open a file's document through the provider without presenting it. An
/// already open document is reused. The caller is expected to show it: it
/// closes with the last view that shows it.
pub(crate) fn load_document(
    uri: String,
    destination: DocumentDestination,
    caller: OperationCaller,
    completion: impl FnOnce(Result<DocumentId, OpenError>, &mut App) + 'static,
    cx: &mut App,
) -> Result<(), OpenError> {
    let uri =
        super::super::resource::ResourceUri::parse(&uri).map_err(|_| OpenError::InvalidUri)?;
    let (window, target) = capture_destination(&destination, cx)?;
    window.update(cx, |window, cx| {
        window.start_resource_operation(
            OpenRequest::from_uri_for_product(uri),
            target,
            OpenOptions {
                kind: Some(ResourceKind::File),
                load_only: true,
                caller: Some(caller),
                ..Default::default()
            },
            move |result, cx| {
                completion(
                    result
                        .map_err(OpenFailure::host_error)
                        .and_then(|commit| match commit {
                            OpenCommit::Loaded(document) => Ok(document),
                            _ => Err(OpenError::ResourceWrongKind),
                        }),
                    cx,
                )
            },
            cx,
        )
    });
    Ok(())
}

pub(crate) fn create_untitled(
    destination: DocumentDestination,
    cx: &mut App,
) -> Result<(DocumentId, PresentationTarget), OpenError> {
    let (window, target) = capture_destination(&destination, cx)?;
    window.update(cx, |window, cx| {
        let (document, target) = window
            .new_document_in_pane(target.tab.map(|tab| tab.pane), cx)
            .ok_or(OpenError::Access(AccessError::InvalidTarget))?;
        Ok((document, target.public_target()))
    })
}

pub(crate) fn present_document(
    id: String,
    destination: DocumentDestination,
    cx: &mut App,
) -> Result<(DocumentId, PresentationTarget), OpenError> {
    let (window, target) = capture_destination(&destination, cx)?;
    let document = KnotWindow::documents(cx)
        .read(cx)
        .documents()
        .find(|document| document.identity().to_string() == id)
        .map(|document| document.id())
        .ok_or(OpenError::DocumentNotFound)?;
    let target = window.update(cx, |window, cx| {
        window.next_open_generation();
        let target = window.present_open_document(document, &target, false, false, cx);
        window.focus_active_surface(cx);
        cx.notify();
        target
    });
    Ok((document, target.public_target()))
}

/// Restore one of `owner`'s retained recovery captures through the
/// application's registered providers.
pub(crate) fn restore_retained(
    owner: String,
    capture_key: String,
    cx: &mut App,
) -> Task<Result<super::super::recovery::RestoreOutcome, super::super::recovery::RestoreError>> {
    let providers = KnotWindow::filesystems(cx);
    super::super::recovery::restore_retained(owner, capture_key, providers, cx)
}

pub(crate) fn replace_roots(
    roots: Vec<String>,
    window_target: WindowTarget,
    caller: OperationCaller,
    completion: impl FnOnce(Result<knot_protocol::WorkspaceInfo, WorkspaceError>) + 'static,
    cx: &mut App,
) -> Result<(), WorkspaceError> {
    let roots = roots
        .into_iter()
        .map(|uri| {
            super::super::resource::ResourceUri::parse(&uri)
                .map_err(|_| WorkspaceError::Resource(OpenError::InvalidUri))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let window = capture_window(&window_target, cx)?;
    let shell_id = window.entity_id();
    window.update(cx, |window, cx| {
        let generation = window.next_open_generation();
        let workspace = window.workspace.snapshot();
        let providers = KnotWindow::filesystems(cx);
        let task = cx.spawn(async move |this, cx| {
            let validated = async {
                let mut validated = Vec::new();
                for uri in roots {
                    match super::super::open::load_resource_kind(
                        providers.clone(),
                        uri,
                        Some(ResourceKind::Directory),
                    )
                    .await
                    {
                        Ok(OpenResource::Directory(uri)) => validated.push(uri),
                        Ok(_) => {
                            return Err(WorkspaceError::Resource(OpenError::ResourceWrongKind));
                        }
                        Err(error) => {
                            return Err(WorkspaceError::Resource(
                                OpenFailure::Resource(error).host_error(),
                            ));
                        }
                    }
                }
                Ok(validated)
            }
            .await;
            let mut completion = Some(completion);
            if this
                .update(cx, |window, cx| {
                    let result = if !caller.is_live(cx) {
                        Err(WorkspaceError::Resource(OpenError::Access(
                            AccessError::Cancelled,
                        )))
                    } else if window.open_generation != generation
                        || !window.workspace.is_current(&workspace)
                        || !cx.global::<ApplicationWindows>().is_open(shell_id)
                    {
                        Err(WorkspaceError::Resource(OpenError::Access(
                            AccessError::InvalidTarget,
                        )))
                    } else {
                        validated.map(|roots| {
                            let snapshot = window.replace_workspace_roots(roots, cx);
                            workspace_info(window_target, &snapshot)
                        })
                    };
                    completion.take().unwrap()(result);
                })
                .is_err()
            {
                completion.take().unwrap()(Err(WorkspaceError::Resource(OpenError::Access(
                    AccessError::InvalidTarget,
                ))));
            }
        });
        window.tasks.push(task);
    });
    Ok(())
}

pub(crate) fn close_presentation(
    presentation: PresentationTarget,
    discard: bool,
    caller: OperationCaller,
    completion: impl FnOnce(Result<knot_protocol::PresentationCloseOutcome, CloseError>) + 'static,
    cx: &mut App,
) -> Result<(), CloseError> {
    let (window, target) = capture_destination(&DocumentDestination::View(presentation), cx)?;
    let workbench = target
        .workbench
        .upgrade()
        .ok_or(CloseError::Access(AccessError::InvalidTarget))?;
    let workbench_state = workbench.read(cx);
    target
        .tab
        .and_then(|tab| workbench_state.tab(tab.pane, tab.tab))
        .and_then(|tab| workbench_state.command_view(tab))
        .ok_or(CloseError::Access(AccessError::InvalidTarget))?;
    let origin = ProductCommandTarget {
        window: window.downgrade(),
        workbench: target.workbench,
        tab: target.tab,
        focused: true,
        view: None,
        leaf: None,
    };
    super::super::commands::dispatch::break_captured_history_group(&origin, cx);
    KnotWindow::begin_protected_operation(
        ProtectedCloseRequest {
            kind: ProtectedCloseKind::Tab,
            origin,
            caller: Some(caller),
            discard,
        },
        Box::new(move |result, _| {
            completion(result.map_err(|error| match error {
                ProtectedCloseFailure::InvalidTarget => {
                    CloseError::Access(AccessError::InvalidTarget)
                }
                ProtectedCloseFailure::InProgress => CloseError::OperationInProgress,
                ProtectedCloseFailure::Cancelled => CloseError::Access(AccessError::Cancelled),
                ProtectedCloseFailure::PersistenceFailed { .. } => CloseError::PersistenceFailed,
            }));
        }),
        cx,
    );
    Ok(())
}

/// The live document with the given public identity.
fn document_by_identity(id: &str, cx: &App) -> Result<DocumentId, OpenError> {
    KnotWindow::documents(cx)
        .read(cx)
        .documents()
        .find(|document| document.identity().to_string() == id)
        .map(|document| document.id())
        .ok_or(OpenError::DocumentNotFound)
}

/// The document named by `identity` and the buffer holding its text.
pub(crate) fn document_model(
    identity: &str,
    cx: &App,
) -> Result<(DocumentId, Entity<crate::app::model::Buffer>), OpenError> {
    let document = document_by_identity(identity, cx)?;
    let model = KnotWindow::documents(cx)
        .read(cx)
        .get(document)
        .ok_or(OpenError::DocumentNotFound)?
        .model()
        .clone();
    Ok((document, model))
}

/// Make the document's text its file's again; see [`document_watch::reload`].
pub(crate) fn reload_document(
    id: &str,
    completion: impl FnOnce(Result<(), OpenError>, &mut App) + 'static,
    cx: &mut App,
) -> Result<(), OpenError> {
    let document = document_by_identity(id, cx)?;
    super::document_watch::reload(document, completion, cx);
    Ok(())
}

/// Save every dirty document that has a file, without a dialog or cleanup;
/// untitled documents are skipped. Completes with the first failure, once
/// every save has finished.
pub(crate) fn save_all(
    completion: impl FnOnce(Result<(), OpenError>, &mut App) + 'static,
    cx: &mut App,
) {
    use crate::app::documents::DocumentDirty;
    let ids: Vec<String> = KnotWindow::documents(cx)
        .read(cx)
        .documents()
        .filter(|document| document.resource_uri().is_some() && document.is_dirty(cx))
        .map(|document| document.identity().to_string())
        .collect();
    let pending = Rc::new(Cell::new(ids.len()));
    let failure = Rc::new(RefCell::new(None));
    let completion = Rc::new(RefCell::new(Some(completion)));
    let finish = {
        let (pending, failure, completion) = (pending.clone(), failure.clone(), completion.clone());
        move |result: Result<(), OpenError>, cx: &mut App| {
            if let Err(error) = result {
                failure.borrow_mut().get_or_insert(error);
            }
            pending.set(pending.get() - 1);
            if pending.get() == 0 {
                let result = failure.borrow_mut().take().map_or(Ok(()), Err);
                if let Some(completion) = completion.borrow_mut().take() {
                    completion(result, cx);
                }
            }
        }
    };
    if ids.is_empty() {
        if let Some(completion) = completion.borrow_mut().take() {
            completion(Ok(()), cx);
        }
        return;
    }
    for id in ids {
        let finish = finish.clone();
        if let Err(error) = save_document(&id, None, finish.clone(), cx) {
            finish(Err(error), cx);
        }
    }
}

/// Save a document without a dialog: in place, or, with `uri`, to that file,
/// replacing what is there, which the document then names.
pub(crate) fn save_document(
    id: &str,
    uri: Option<String>,
    completion: impl FnOnce(Result<(), OpenError>, &mut App) + 'static,
    cx: &mut App,
) -> Result<(), OpenError> {
    let documents = KnotWindow::documents(cx);
    let document = document_by_identity(id, cx)?;
    let (model, title, state) = {
        let document = documents
            .read(cx)
            .get(document)
            .ok_or(OpenError::DocumentNotFound)?;
        (
            document.model().clone(),
            document.title().to_owned(),
            document.state().clone(),
        )
    };
    let explicit = uri.is_some();
    let target = match uri {
        Some(uri) => {
            super::super::resource::ResourceUri::parse(&uri).map_err(|_| OpenError::InvalidUri)?
        }
        None => state
            .resource_uri()
            .cloned()
            .ok_or(OpenError::ResourceNotFound)?,
    };
    if documents
        .read(cx)
        .document_for_resource(&target)
        .is_some_and(|owner| owner != document)
    {
        return Err(OpenError::ResourceAlreadyExists);
    }
    let title = if explicit {
        super::resource_title(&target)
    } else {
        title
    };
    let (text, revision) = model.read_with(cx, |model, _| (model.text(), model.revision()));
    let capture = documents
        .update(cx, |documents, _| {
            documents.begin_persistence(document, &model)
        })
        .ok_or(OpenError::DocumentNotFound)?;
    let providers = KnotWindow::filesystems(cx);
    cx.spawn(async move |cx| {
        let written =
            super::save::write_document_text(providers, target, text, state, explicit).await;
        cx.update(|cx| {
            let result = match written {
                Ok((uri, version)) => {
                    let committed = documents.update(cx, |documents, cx| {
                        documents.finish_persistence(
                            document,
                            &model,
                            &capture,
                            (title, uri),
                            (version, revision),
                            cx,
                        )
                    });
                    committed.then_some(()).ok_or(OpenError::DocumentNotFound)
                }
                Err(error) => Err(resource_error(error.into())),
            };
            completion(result, cx);
        });
    })
    .detach();
    Ok(())
}
