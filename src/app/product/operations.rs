//! Destination admission and semantic product operations for extension requests.

use super::super::extensions::host::handlers::{
    AccessError, CloseError, OpenError, WorkspaceError,
};
use super::*;
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
    fn public_target(&self) -> PresentationTarget {
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

/// What a diff's modified side shows.
pub(crate) enum DiffModified {
    Snapshot(crate::app::diff_view::SnapshotSpec),
    /// The open document with this public identity.
    Document {
        id: String,
        label: String,
    },
}

/// Open a diff in the center of the destination's window.
pub(crate) fn open_diff(
    destination: DocumentDestination,
    original: crate::app::diff_view::SnapshotSpec,
    modified: DiffModified,
    cx: &mut App,
) -> Result<PresentationTarget, OpenError> {
    use crate::app::diff_view::ModifiedSource;

    let (window, _) = capture_destination(&destination, cx)?;
    let modified = match modified {
        DiffModified::Snapshot(spec) => ModifiedSource::Snapshot(spec),
        DiffModified::Document { id, label } => {
            let document = document_by_identity(&id, cx)?;
            let model = KnotWindow::documents(cx)
                .read(cx)
                .get(document)
                .ok_or(OpenError::DocumentNotFound)?
                .model()
                .clone();
            ModifiedSource::Document {
                id: document,
                label,
                model,
            }
        }
    };
    Ok(window
        .update(cx, |window, cx| window.open_diff(original, modified, cx))
        .public_target())
}

/// Replace the text of a snapshot side of the diff tab `target` names.
pub(crate) fn set_diff_text(
    target: &PresentationTarget,
    side: knot_model::diff::DiffSide,
    text: &str,
    cx: &mut App,
) -> Result<(), AccessError> {
    let (_, open) = capture_destination(&DocumentDestination::View(target.clone()), cx)?;
    let workbench = open.workbench.upgrade().ok_or(AccessError::InvalidTarget)?;
    let tab = open.tab.ok_or(AccessError::InvalidTarget)?;
    let view = {
        let workbench = workbench.read(cx);
        workbench
            .tab(tab.pane, tab.tab)
            .and_then(|tab| workbench.diff_view(tab))
            .cloned()
            .ok_or(AccessError::InvalidTarget)?
    };
    if view.update(cx, |view, cx| view.replace_text(side, text, cx)) {
        Ok(())
    } else {
        Err(AccessError::InvalidTarget)
    }
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

/// An exact document tab presentation retained beyond one command. It never
/// resolves to a replacement tab, or to the tab after a move.
#[derive(Clone)]
pub(crate) struct EditorTab(OpenTarget);

impl EditorTab {
    /// The tab's editor, while this exact presentation is live.
    pub(crate) fn editor(&self, cx: &App) -> Option<Entity<crate::app::editor::Editor>> {
        let target = &self.0;
        let workbench = target.workbench.upgrade()?;
        if !window_is_live(&target.window, &workbench, cx)
            || !target.matches_presentation(&workbench, cx)
        {
            return None;
        }
        let workbench = workbench.read(cx);
        let captured = target.tab?;
        match workbench.command_view(workbench.tab(captured.pane, captured.tab)?)? {
            crate::app::workbench::CommandView::Editor(view) => {
                Some(view.read(cx).editor().clone())
            }
            _ => None,
        }
    }
}

pub(crate) fn capture_editor_tab(
    target: &PresentationTarget,
    cx: &mut App,
) -> Result<EditorTab, AccessError> {
    let (_, target) = capture_destination(&DocumentDestination::View(target.clone()), cx)?;
    let tab = EditorTab(target);
    tab.editor(cx).ok_or(AccessError::InvalidTarget)?;
    Ok(tab)
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
                            OpenCommit::Workspace(_) => Err(OpenError::ResourceWrongKind),
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
