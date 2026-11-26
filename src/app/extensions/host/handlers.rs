//! Declared application capabilities and their foreground access context.
//!
//! A `host_operation` attribute pairs the async function's owned context,
//! arguments, semantic success and error types, and automatically links its
//! descriptor. Implement `HostArguments` using recognized fields from
//! `ArgumentReader`; implement `HostSuccess` for the result representation. Reuse
//! shared payloads and compose local errors with `HostError`'s transparent form.
//! Every public failure variant declares an explicit stable code and diagnostic
//! message; named fields become structured details.
//!
//! Access models only through short foreground closures returning owned values.
//! Retain captured authority across native awaits and revalidate it before commit.
//! Keep V8 conversion in the host adapters and update the checked TypeScript
//! facade/declarations when exposing or changing a public capability.

use super::*;
use crate::app::extensions::storage::{OwnerStorage, StorageError};
use gpui::WeakEntity;

mod process;
use knot_protocol::declarations::{
    ArgumentReader, DecodeError, EncodedValue, HostArguments, HostError, HostSuccess, encode_json,
    host_operation,
};

#[derive(Clone)]
pub(crate) struct HostContext {
    host: WeakEntity<ProductExtensionHost>,
    application: gpui::AsyncApp,
    request: knot_protocol::RequestContext,
}

impl HostContext {
    pub(crate) fn new(
        request: knot_protocol::RequestContext,
        cx: &mut Context<ProductExtensionHost>,
    ) -> Self {
        Self {
            host: cx.entity().downgrade(),
            application: cx.to_async(),
            request,
        }
    }

    fn foreground<R: 'static>(
        &self,
        access: impl FnOnce(&mut ProductExtensionHost, &mut Context<ProductExtensionHost>) -> R,
    ) -> Result<R, AccessError> {
        self.host
            .update(&mut self.application.clone(), |host, cx| {
                if !host.operation_is_live(
                    ExtensionKey::new(self.request.extension, self.request.lifecycle),
                    self.request.invocation,
                ) {
                    return Err(AccessError::Cancelled);
                }
                let result = access(host, cx);
                host.drain_command_events(cx);
                Ok(result)
            })
            .map_err(|_| AccessError::Cancelled)?
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, HostError)]
pub(crate) enum AccessError {
    #[error(code = "Cancelled", message = "Cancelled")]
    Cancelled,
    #[error(code = "InvalidTarget", message = "InvalidTarget")]
    InvalidTarget,
    #[error(code = "ContextUnavailable", message = "ContextUnavailable")]
    ContextUnavailable,
    /// Raised by the JavaScript ownership objects themselves; native handlers
    /// never construct it, but it is part of the declared public codes.
    #[allow(dead_code)]
    #[error(code = "OwnershipDisposed", message = "OwnershipDisposed")]
    OwnershipDisposed,
}

/// Failures of an extension transient interaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq, HostError)]
pub(crate) enum TransientError {
    #[error(code = "OperationInProgress", message = "OperationInProgress")]
    OperationInProgress,
    #[error(code = "ContextUnavailable", message = "ContextUnavailable")]
    ContextUnavailable,
    #[error(code = "InvalidTarget", message = "InvalidTarget")]
    InvalidTarget,
    #[error(code = "Cancelled", message = "Cancelled")]
    Cancelled,
}

impl From<AccessError> for TransientError {
    fn from(error: AccessError) -> Self {
        match error {
            AccessError::Cancelled | AccessError::OwnershipDisposed => Self::Cancelled,
            AccessError::InvalidTarget => Self::InvalidTarget,
            AccessError::ContextUnavailable => Self::ContextUnavailable,
        }
    }
}

/// Failures of the `workspaces.pickFolder` interaction and its path validation.
#[derive(Debug, HostError)]
pub(crate) enum FolderPickerError {
    #[error(transparent)]
    Transient(TransientError),
    #[error(transparent)]
    Workspace(WorkspaceError),
    #[error(
        code = "ProviderFailure",
        message = "the folder dialog failed: {message}"
    )]
    Dialog { message: String },
}

impl From<TransientError> for FolderPickerError {
    fn from(error: TransientError) -> Self {
        Self::Transient(error)
    }
}

impl From<AccessError> for FolderPickerError {
    fn from(error: AccessError) -> Self {
        Self::Transient(error.into())
    }
}

impl From<WorkspaceError> for FolderPickerError {
    fn from(error: WorkspaceError) -> Self {
        Self::Workspace(error)
    }
}

impl From<OpenError> for FolderPickerError {
    fn from(error: OpenError) -> Self {
        Self::Workspace(error.into())
    }
}

#[derive(Debug, HostError)]
pub(crate) enum OpenError {
    #[error(transparent)]
    Access(AccessError),
    #[error(code = "InvalidUri", message = "InvalidUri")]
    InvalidUri,
    #[error(code = "InvalidUtf8", message = "InvalidUtf8")]
    InvalidUtf8,
    #[error(code = "ResourceNotFound", message = "ResourceNotFound")]
    ResourceNotFound,
    #[error(code = "ResourceWrongKind", message = "ResourceWrongKind")]
    ResourceWrongKind,
    #[error(code = "ResourceAlreadyExists", message = "ResourceAlreadyExists")]
    ResourceAlreadyExists,
    #[error(code = "ProviderFailure", message = "ProviderFailure")]
    ProviderFailure,
    #[error(code = "BufferClosed", message = "BufferClosed")]
    BufferClosed,
    #[error(code = "DocumentNotFound", message = "DocumentNotFound")]
    DocumentNotFound,
    /// The change would drop the final view of a document with unsaved
    /// changes.
    #[error(code = "DocumentUnsaved", message = "DocumentUnsaved")]
    DocumentUnsaved,
}

impl From<AccessError> for OpenError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClipboardArguments {
    text: String,
}

impl HostArguments for ClipboardArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            text: reader.text(2, "clipboard text must be text")?,
        })
    }
}

#[host_operation("writeClipboardText")]
async fn write_clipboard(
    context: HostContext,
    arguments: ClipboardArguments,
) -> Result<(), AccessError> {
    context
        .foreground(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(arguments.text)))
}

/// The application clipboard's text, or `None` when it holds none.
#[host_operation("readClipboardText")]
async fn read_clipboard(context: HostContext, _: ()) -> Result<Option<String>, AccessError> {
    context.foreground(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OpenArguments {
    uri: String,
    target: Option<knot_protocol::DocumentDestination>,
    preview: bool,
    focus: bool,
}

impl HostArguments for OpenArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            uri: reader.text(2, "resource URI must be text")?,
            target: reader.destination(3)?,
            preview: reader
                .optional_boolean(4, "preview must be a boolean")?
                .unwrap_or(false),
            focus: reader
                .optional_boolean(5, "focus must be a boolean")?
                .unwrap_or(true),
        })
    }
}

#[host_operation("openDocument")]
async fn open_document(
    context: HostContext,
    arguments: OpenArguments,
) -> Result<knot_protocol::DocumentPresentation, OpenError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let host = context.host.clone();
    let request = context.request;
    context.foreground(move |state, cx| {
        let destination = state.document_destination(&request, arguments.target, cx)?;
        product::operations::open_document(
            arguments.uri,
            destination,
            arguments.preview,
            arguments.focus,
            product::operations::OperationCaller {
                host,
                key: ExtensionKey::new(request.extension, request.lifecycle),
                invocation: request.invocation,
            },
            move |result, _| {
                let _ = sender.send(result);
            },
            cx,
        )
    })??;
    let (document, target) = receiver.await.map_err(|_| AccessError::Cancelled)??;
    context.foreground(move |host, cx| {
        product::operations::validate_presentation(&target, cx)?;
        host.document_presentation(document, target, cx)
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TerminalArguments {
    pub cwd: Option<String>,
    pub target: Option<knot_protocol::DocumentDestination>,
    pub location: product::operations::TerminalLocation,
}

impl HostArguments for TerminalArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            cwd: reader.optional_text(2, "terminal directory must be text")?,
            target: reader.destination(3)?,
            location: match reader
                .optional_text(4, "terminal location must be text")?
                .as_deref()
            {
                None | Some("editor") => product::operations::TerminalLocation::Center,
                Some(view) => product::operations::TerminalLocation::Container(view.to_owned()),
            },
        })
    }
}

#[host_operation("openTerminal")]
async fn open_terminal(
    context: HostContext,
    arguments: TerminalArguments,
) -> Result<knot_protocol::PresentationInfo, OpenError> {
    let request = context.request;
    let destination = context
        .foreground(|host, cx| host.document_destination(&request, arguments.target, cx))??;
    let directory = match arguments.cwd {
        Some(uri) => {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            context.foreground(move |_, cx| {
                product::operations::resolve_terminal_directory(
                    uri,
                    move |result| {
                        let _ = sender.send(result);
                    },
                    cx,
                )
            })??;
            Some(receiver.await.map_err(|_| AccessError::Cancelled)??)
        }
        None => None,
    };
    let location = arguments.location;
    context.foreground(move |host, cx| {
        let target = product::operations::open_terminal(destination, directory, &location, cx)?;
        host.presentation_info(&target, cx)
            .ok_or(OpenError::from(AccessError::InvalidTarget))
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocumentLookup {
    pub id: String,
}

impl HostArguments for DocumentLookup {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            id: reader.text(2, "document identity must be text")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WindowLookup {
    pub target: knot_protocol::WindowTarget,
}

impl HostArguments for WindowLookup {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            target: knot_protocol::WindowTarget {
                window: reader.text(2, "window target must be text")?,
            },
        })
    }
}

#[host_operation("listDocuments")]
pub(crate) async fn list_documents(
    context: HostContext,
    _: (),
) -> Result<Vec<knot_protocol::DocumentInfo>, AccessError> {
    context.foreground(|host, cx| host.application_snapshot(cx).documents)
}

#[host_operation("getDocument")]
pub(crate) async fn get_document(
    context: HostContext,
    arguments: DocumentLookup,
) -> Result<Option<knot_protocol::DocumentInfo>, AccessError> {
    context.foreground(|host, cx| {
        host.application_snapshot(cx)
            .documents
            .into_iter()
            .find(|document| document.id == arguments.id)
    })
}

#[host_operation("listWindows")]
pub(crate) async fn list_windows(
    context: HostContext,
    _: (),
) -> Result<Vec<knot_protocol::WindowInfo>, AccessError> {
    context.foreground(|host, cx| host.application_snapshot(cx).windows)
}

#[host_operation("getWindow")]
pub(crate) async fn get_window(
    context: HostContext,
    arguments: WindowLookup,
) -> Result<Option<knot_protocol::WindowInfo>, AccessError> {
    context.foreground(|host, cx| {
        host.application_snapshot(cx)
            .windows
            .into_iter()
            .find(|window| window.target == arguments.target)
    })
}

#[host_operation("getWorkspace")]
pub(crate) async fn get_workspace(
    context: HostContext,
    arguments: WindowLookup,
) -> Result<Option<knot_protocol::WorkspaceInfo>, AccessError> {
    context.foreground(|host, cx| {
        host.application_snapshot(cx)
            .windows
            .into_iter()
            .find(|window| window.target == arguments.target)
            .map(|window| window.workspace)
    })
}

#[host_operation("observeWorkbench")]
pub(crate) async fn observe_workbench(
    context: HostContext,
    _: (),
) -> Result<knot_protocol::WorkbenchObservation, AccessError> {
    let key = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context.foreground(|host, cx| {
        host.refresh_application_observation(cx);
        host.application_observation.subscribe(key)
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WorkbenchSubscription {
    pub subscription: knot_protocol::WorkbenchSubscriptionId,
}

impl HostArguments for WorkbenchSubscription {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            subscription: knot_protocol::WorkbenchSubscriptionId::new(reader.integer(2)?),
        })
    }
}

#[host_operation("unobserveWorkbench")]
pub(crate) async fn unobserve_workbench(
    context: HostContext,
    arguments: WorkbenchSubscription,
) -> Result<(), AccessError> {
    let key = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context.foreground(|host, _| {
        // A subscription the host already released is as good as disposed.
        host.application_observation
            .unsubscribe(arguments.subscription, key);
        Ok(())
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DestinationArguments {
    pub target: Option<knot_protocol::DocumentDestination>,
}

impl HostArguments for DestinationArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            target: reader.destination(2)?,
        })
    }
}

#[host_operation("createUntitled")]
pub(crate) async fn create_untitled(
    context: HostContext,
    arguments: DestinationArguments,
) -> Result<knot_protocol::DocumentPresentation, OpenError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let destination = host.document_destination(&request, arguments.target, cx)?;
        let (document, target) = product::operations::create_untitled(destination, cx)?;
        host.document_presentation(document, target, cx)
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PresentArguments {
    pub id: String,
    pub target: Option<knot_protocol::DocumentDestination>,
}

impl HostArguments for PresentArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            id: reader.text(2, "document identity must be text")?,
            target: reader.destination(3)?,
        })
    }
}

#[host_operation("presentDocument")]
pub(crate) async fn present_document(
    context: HostContext,
    arguments: PresentArguments,
) -> Result<knot_protocol::DocumentPresentation, OpenError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let destination = host.document_destination(&request, arguments.target, cx)?;
        let (document, target) =
            product::operations::present_document(arguments.id, destination, cx)?;
        host.document_presentation(document, target, cx)
    })?
}

#[derive(Debug, HostError)]
pub(crate) enum WorkspaceError {
    #[error(transparent)]
    Resource(OpenError),
}

impl From<AccessError> for WorkspaceError {
    fn from(error: AccessError) -> Self {
        Self::Resource(error.into())
    }
}

impl From<OpenError> for WorkspaceError {
    fn from(error: OpenError) -> Self {
        Self::Resource(error)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WorkspaceArguments {
    pub roots: Vec<String>,
    pub target: Option<knot_protocol::WindowTarget>,
}

fn read_window(
    reader: &mut dyn ArgumentReader,
    index: usize,
) -> Result<Option<knot_protocol::WindowTarget>, DecodeError> {
    Ok(reader
        .optional_text(index, "window target must be text")?
        .map(|window| knot_protocol::WindowTarget { window }))
}

impl HostArguments for WorkspaceArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            roots: reader.text_list(2)?,
            target: read_window(reader, 3)?,
        })
    }
}

#[host_operation("replaceWorkspaceRoots")]
pub(crate) async fn replace_workspace_roots(
    context: HostContext,
    arguments: WorkspaceArguments,
) -> Result<knot_protocol::WorkspaceInfo, WorkspaceError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let request = context.request;
    let caller = product::operations::OperationCaller {
        host: context.host.clone(),
        key: ExtensionKey::new(request.extension, request.lifecycle),
        invocation: request.invocation,
    };
    context.foreground(|host, cx| {
        let target = match arguments.target {
            Some(target) => target,
            None => host.operation_window(&request, cx)?,
        };
        product::operations::replace_roots(
            arguments.roots,
            target,
            caller,
            move |result| {
                let _ = sender.send(result);
            },
            cx,
        )
    })??;
    let result = receiver.await.map_err(|_| AccessError::Cancelled)??;
    context.foreground(|_, _| result).map_err(Into::into)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FolderArguments {
    pub uri: String,
    pub target: Option<knot_protocol::WindowTarget>,
}

impl HostArguments for FolderArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            uri: reader.text(2, "resource URI must be text")?,
            target: read_window(reader, 3)?,
        })
    }
}

#[host_operation("openFolder")]
pub(crate) async fn open_folder(
    context: HostContext,
    arguments: FolderArguments,
) -> Result<knot_protocol::WorkspaceInfo, WorkspaceError> {
    replace_workspace_roots(
        context,
        WorkspaceArguments {
            roots: vec![arguments.uri],
            target: arguments.target,
        },
    )
    .await
}

#[derive(Debug, HostError)]
pub(crate) enum CloseError {
    #[error(transparent)]
    Access(AccessError),
    #[error(code = "OperationInProgress", message = "OperationInProgress")]
    OperationInProgress,
    #[error(code = "PersistenceFailed", message = "PersistenceFailed")]
    PersistenceFailed,
}

impl From<AccessError> for CloseError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ViewTargetArguments {
    pub target: knot_protocol::PresentationTarget,
}

impl HostArguments for ViewTargetArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        match reader.destination(2)? {
            Some(knot_protocol::DocumentDestination::View(target)) => Ok(Self { target }),
            _ => Err(DecodeError::Type("close requires an exact view target")),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CloseArguments {
    target: knot_protocol::PresentationTarget,
    discard: bool,
}

impl HostArguments for CloseArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let ViewTargetArguments { target } = ViewTargetArguments::decode(reader)?;
        Ok(Self {
            target,
            discard: reader
                .optional_boolean(3, "discard must be a boolean")?
                .unwrap_or(false),
        })
    }
}

#[host_operation("closePresentation")]
pub(crate) async fn close_presentation(
    context: HostContext,
    arguments: CloseArguments,
) -> Result<knot_protocol::PresentationCloseOutcome, CloseError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let caller = product::operations::OperationCaller {
        host: context.host.clone(),
        key: ExtensionKey::new(context.request.extension, context.request.lifecycle),
        invocation: context.request.invocation,
    };
    context.foreground(|_, cx| {
        product::operations::close_presentation(
            arguments.target,
            arguments.discard,
            caller,
            move |result| {
                let _ = sender.send(result);
            },
            cx,
        )
    })??;
    let result = receiver.await.map_err(|_| AccessError::Cancelled)??;
    context.foreground(|_, _| result).map_err(Into::into)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CompositeKindArguments {
    kind: String,
}

impl HostArguments for CompositeKindArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            kind: reader.text(2, "composite kind must be text")?,
        })
    }
}

/// Reserve a composite view kind for the calling lifecycle.
#[host_operation("registerComposite")]
pub(crate) async fn register_composite(
    context: HostContext,
    arguments: CompositeKindArguments,
) -> Result<(), SemanticError> {
    let owner = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context.foreground(|host, _| host.register_composite_kind(owner, arguments.kind))?
}

/// Release a kind and close the instances of it that are open.
#[host_operation("unregisterComposite")]
pub(crate) async fn unregister_composite(
    context: HostContext,
    arguments: CompositeKindArguments,
) -> Result<(), AccessError> {
    let owner = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context.foreground(|host, cx| host.unregister_composite_kind(owner, &arguments.kind, cx))?;
    Ok(())
}

/// The tab of a composite view: title, icon as JSON, resource URI, toolbar as
/// JSON, and scroll link as JSON or null.
fn decode_composite_tab(
    reader: &mut dyn ArgumentReader,
    first: usize,
) -> Result<composites::CompositeTabSpec, DecodeError> {
    let icon = reader
        .optional_text(first + 1, "composite icon must be text or null")?
        .map(|icon| {
            serde_json::from_str(&icon).map_err(|_| DecodeError::Type("composite icon is invalid"))
        })
        .transpose()?;
    Ok(super::composites::CompositeTabSpec {
        title: reader.text(first, "composite title must be text")?,
        icon,
        resource: reader.optional_text(first + 2, "composite resource must be text or null")?,
        toolbar: serde_json::from_str(&reader.text(first + 3, "composite toolbar must be text")?)
            .map_err(|_| DecodeError::Type("composite toolbar is invalid"))?,
        link: reader
            .optional_text(first + 4, "composite scroll link must be text or null")?
            .map(|link| {
                serde_json::from_str(&link)
                    .map_err(|_| DecodeError::Type("composite scroll link is invalid"))
            })
            .transpose()?,
    })
}

/// A composite layout, sent as JSON.
fn decode_composite_layout(
    json: &str,
    popover: bool,
) -> Result<super::composites::LayoutWire, DecodeError> {
    let layout: super::composites::LayoutWire =
        serde_json::from_str(json).map_err(|_| DecodeError::Type("composite layout is invalid"))?;
    layout.check(popover).map_err(DecodeError::Range)?;
    Ok(layout)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OpenCompositeArguments {
    target: Option<knot_protocol::DocumentDestination>,
    kind: String,
    tab: super::composites::CompositeTabSpec,
    layout: super::composites::LayoutWire,
}

impl HostArguments for OpenCompositeArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            target: reader.destination(2)?,
            kind: reader.text(3, "composite kind must be text")?,
            tab: decode_composite_tab(reader, 4)?,
            layout: decode_composite_layout(
                &reader.text(9, "composite layout must be text")?,
                false,
            )?,
        })
    }
}

/// Open a composite view of a kind the caller registered in the center of the
/// destination's window. The caller's lifecycle owns the tab.
#[host_operation("openComposite")]
pub(crate) async fn open_composite(
    context: HostContext,
    arguments: OpenCompositeArguments,
) -> Result<knot_protocol::PresentationInfo, OpenError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let destination = host.document_destination(&request, arguments.target, cx)?;
        let owner = ExtensionKey::new(request.extension, request.lifecycle);
        let target = host.open_composite(
            owner,
            destination,
            arguments.kind,
            arguments.tab,
            arguments.layout,
            cx,
        )?;
        host.presentation_info(&target, cx)
            .ok_or(OpenError::from(AccessError::InvalidTarget))
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UpdateCompositeArguments {
    target: knot_protocol::PresentationTarget,
    tab: super::composites::CompositeTabSpec,
    layout: Option<super::composites::LayoutWire>,
}

impl HostArguments for UpdateCompositeArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let ViewTargetArguments { target } = ViewTargetArguments::decode(reader)?;
        Ok(Self {
            target,
            tab: decode_composite_tab(reader, 3)?,
            layout: reader
                .optional_text(8, "composite layout must be text or null")?
                .map(|json| decode_composite_layout(&json, false))
                .transpose()?,
        })
    }
}

/// Replace the title, icon, and resource of a composite view the caller
/// opened, and its layout when given.
#[host_operation("updateComposite")]
pub(crate) async fn update_composite(
    context: HostContext,
    arguments: UpdateCompositeArguments,
) -> Result<(), OpenError> {
    let owner = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context.foreground(|host, cx| {
        host.update_composite(
            owner,
            &arguments.target,
            arguments.tab,
            arguments.layout,
            cx,
        )
    })?
}

pub(crate) use crate::app::extensions::buffers::BufferError;
use knot_protocol::{
    BufferHandle, BufferSubscriptionId, ByteRange, DiffHunk, DiffResult, EditorContribution,
    FindResult, LineRange, ReplaceResult, TextEdit, TextSnapshot, WordKind, WordSegment,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SnapshotArguments {
    pub buffer: BufferHandle,
    pub range: Option<ByteRange>,
}
impl HostArguments for SnapshotArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            buffer: BufferHandle::new(reader.integer(2)?),
            range: reader.range(3)?,
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EditArguments {
    pub buffer: BufferHandle,
    pub edits: Vec<TextEdit>,
    pub if_revision: u64,
}
impl HostArguments for EditArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            buffer: BufferHandle::new(reader.integer(2)?),
            edits: reader.edits(3)?,
            if_revision: reader.integer(4)?,
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BufferArguments {
    pub buffer: BufferHandle,
}
impl HostArguments for BufferArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            buffer: BufferHandle::new(reader.integer(2)?),
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SubscriptionArguments {
    pub subscription: BufferSubscriptionId,
}
impl HostArguments for SubscriptionArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            subscription: BufferSubscriptionId::new(reader.integer(2)?),
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContributionArguments {
    pub buffer: BufferHandle,
    pub contributions: Vec<EditorContribution>,
    pub if_revision: u64,
}
impl HostArguments for ContributionArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            buffer: BufferHandle::new(reader.integer(2)?),
            contributions: reader.contributions(3)?,
            if_revision: reader.integer(4)?,
        })
    }
}

#[host_operation("snapshot")]
pub(crate) async fn snapshot(
    context: HostContext,
    arguments: SnapshotArguments,
) -> Result<TextSnapshot, BufferError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let commands = &host.commands;
        host.buffers.snapshot(
            &request,
            arguments,
            || commands.is_cancelled(request.invocation),
            cx,
        )
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LineArguments {
    pub buffer: BufferHandle,
    pub value: u64,
}
impl HostArguments for LineArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            buffer: BufferHandle::new(reader.integer(2)?),
            value: reader.integer(3)?,
        })
    }
}

#[host_operation("lineCount")]
pub(crate) async fn line_count(
    context: HostContext,
    arguments: BufferArguments,
) -> Result<u64, BufferError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let commands = &host.commands;
        host.buffers.read_model(
            &request,
            arguments.buffer,
            || commands.is_cancelled(request.invocation),
            cx,
            |model| Ok(model.line_count() as u64),
        )
    })?
}

/// A 0-based line's range without its `\n` or `\r\n` terminator.
#[host_operation("lineRange")]
pub(crate) async fn line_range(
    context: HostContext,
    arguments: LineArguments,
) -> Result<ByteRange, BufferError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let commands = &host.commands;
        host.buffers.read_model(
            &request,
            arguments.buffer,
            || commands.is_cancelled(request.invocation),
            cx,
            |model| {
                let range = model
                    .line_content_range(arguments.value)
                    .ok_or(BufferError::InvalidRange)?;
                Ok(ByteRange {
                    start_byte_offset: range.start,
                    end_byte_offset: range.end,
                })
            },
        )
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LinesArguments {
    pub buffer: BufferHandle,
    pub first: u64,
    pub last: u64,
}
impl HostArguments for LinesArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            buffer: BufferHandle::new(reader.integer(2)?),
            first: reader.integer(3)?,
            last: reader.integer(4)?,
        })
    }
}

/// Lines `first` through `last` (0-based, inclusive), each with its text
/// without terminator and its byte start.
#[host_operation("lines")]
pub(crate) async fn lines(
    context: HostContext,
    arguments: LinesArguments,
) -> Result<Vec<LineItem>, BufferError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let commands = &host.commands;
        host.buffers.read_model(
            &request,
            arguments.buffer,
            || commands.is_cancelled(request.invocation),
            cx,
            |model| {
                let lines = model
                    .lines_between(arguments.first, arguments.last)
                    .ok_or(BufferError::InvalidRange)?;
                Ok(lines.into_iter().map(LineItem::from).collect())
            },
        )
    })?
}

/// The 0-based line holding a byte offset on a character boundary.
#[host_operation("lineAt")]
pub(crate) async fn line_at(
    context: HostContext,
    arguments: LineArguments,
) -> Result<u64, BufferError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let commands = &host.commands;
        host.buffers.read_model(
            &request,
            arguments.buffer,
            || commands.is_cancelled(request.invocation),
            cx,
            |model| {
                let line = model
                    .line_at_offset(arguments.value)
                    .ok_or(BufferError::InvalidRange)?;
                Ok(line as u64)
            },
        )
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FindArguments {
    pub buffer: BufferHandle,
    pub query: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    pub per_line: bool,
    pub range: Option<ByteRange>,
    pub limit: u64,
}
impl HostArguments for FindArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            buffer: BufferHandle::new(reader.integer(2)?),
            query: reader.text(3, "find query must be a string")?,
            case_sensitive: reader
                .optional_boolean(4, "caseSensitive must be a boolean")?
                .unwrap_or(false),
            whole_word: reader
                .optional_boolean(5, "wholeWord must be a boolean")?
                .unwrap_or(false),
            regex: reader
                .optional_boolean(6, "regex must be a boolean")?
                .unwrap_or(false),
            range: reader.range(7)?,
            limit: reader.integer(8)?,
            per_line: reader
                .optional_boolean(9, "perLine must be a boolean")?
                .unwrap_or(false),
        })
    }
}

/// The matches of a query, in order, with the revision searched.
#[host_operation("find")]
pub(crate) async fn find(
    context: HostContext,
    arguments: FindArguments,
) -> Result<FindResult, BufferError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let commands = &host.commands;
        host.buffers.read_model(
            &request,
            arguments.buffer,
            || commands.is_cancelled(request.invocation),
            cx,
            |model| {
                let options = knot_model::buffer::FindOptions {
                    case_sensitive: arguments.case_sensitive,
                    whole_word: arguments.whole_word,
                    regex: arguments.regex,
                    per_line: arguments.per_line,
                };
                let limit = usize::try_from(arguments.limit).unwrap_or(usize::MAX);
                let found = model.find(&arguments.query, options, arguments.range, limit)?;
                Ok(FindResult {
                    revision: model.revision(),
                    matches: found
                        .ranges
                        .into_iter()
                        .map(|range| ByteRange {
                            start_byte_offset: range.start,
                            end_byte_offset: range.end,
                        })
                        .collect(),
                    more: found.more,
                })
            },
        )
    })?
}

/// One side of a diff: text, or a buffer read natively.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DiffSide {
    Text(String),
    Buffer(BufferHandle),
}

fn decode_diff_side(
    reader: &mut dyn ArgumentReader,
    index: usize,
) -> Result<DiffSide, DecodeError> {
    match reader.optional_text(index, "diff text must be a string")? {
        Some(text) => Ok(DiffSide::Text(text)),
        None => Ok(DiffSide::Buffer(BufferHandle::new(
            reader.integer(index + 1)?,
        ))),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DiffArguments {
    pub old: DiffSide,
    pub new: DiffSide,
    pub ignore_whitespace: bool,
}
impl HostArguments for DiffArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            old: decode_diff_side(reader, 2)?,
            new: decode_diff_side(reader, 4)?,
            ignore_whitespace: reader
                .optional_boolean(6, "ignoreWhitespace must be a boolean")?
                .unwrap_or(false),
        })
    }
}

/// The line hunks that turn one text or buffer into another. Buffer sides are
/// copied on the foreground; the comparison runs on the background executor.
#[host_operation("diff")]
pub(crate) async fn diff(
    context: HostContext,
    arguments: DiffArguments,
) -> Result<DiffResult, BufferError> {
    let request = context.request;
    let (old, new) = context.foreground(|host, cx| {
        let commands = &host.commands;
        let mut read = |side: DiffSide| match side {
            DiffSide::Text(text) => Ok((text, None)),
            DiffSide::Buffer(buffer) => host.buffers.read_model(
                &request,
                buffer,
                || commands.is_cancelled(request.invocation),
                cx,
                |model| Ok((model.text(), Some(model.revision()))),
            ),
        };
        let old = read(arguments.old)?;
        let new = read(arguments.new)?;
        Ok::<_, BufferError>((old, new))
    })??;
    let options = knot_model::diff::DiffOptions {
        ignore_whitespace: arguments.ignore_whitespace,
    };
    let (old_text, old_revision) = old;
    let (new_text, new_revision) = new;
    let hunks = context
        .application
        .background_executor()
        .spawn(async move { knot_model::diff::line_hunks(&old_text, &new_text, options) })
        .await;
    // Fails when the invocation was cancelled while the comparison ran.
    context.foreground(|_, _| ())?;
    let range = |range: std::ops::Range<u32>| LineRange {
        start: range.start,
        end: range.end,
    };
    Ok(DiffResult {
        hunks: hunks
            .into_iter()
            .map(|hunk| DiffHunk {
                old: range(hunk.old),
                new: range(hunk.new),
            })
            .collect(),
        old_revision,
        new_revision,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReplaceArguments {
    pub buffer: BufferHandle,
    pub query: String,
    pub replacement: String,
    pub options: knot_model::buffer::FindOptions,
    pub range: Option<ByteRange>,
    pub limit: u64,
    pub if_revision: Option<u64>,
}
impl HostArguments for ReplaceArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            buffer: BufferHandle::new(reader.integer(2)?),
            query: reader.text(3, "replace query must be a string")?,
            replacement: reader.text(4, "replacement must be a string")?,
            options: knot_model::buffer::FindOptions {
                case_sensitive: reader
                    .optional_boolean(5, "caseSensitive must be a boolean")?
                    .unwrap_or(false),
                whole_word: reader
                    .optional_boolean(6, "wholeWord must be a boolean")?
                    .unwrap_or(false),
                regex: reader
                    .optional_boolean(7, "regex must be a boolean")?
                    .unwrap_or(false),
                per_line: reader
                    .optional_boolean(11, "perLine must be a boolean")?
                    .unwrap_or(false),
            },
            range: reader.range(8)?,
            limit: reader.integer(9)?,
            if_revision: reader.optional_integer(10)?,
        })
    }
}

/// Replace the matches of a query in one commit: one undo step and one change
/// notification. The result carries the revision after it, the number of
/// replacements, and whether more matches remained.
#[host_operation("replace")]
pub(crate) async fn replace(
    context: HostContext,
    arguments: ReplaceArguments,
) -> Result<ReplaceResult, BufferError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let commands = &host.commands;
        host.buffers.replace_matches(
            &request,
            arguments,
            || commands.is_cancelled(request.invocation),
            cx,
        )
    })?
}

/// The word segment at an offset, or `None` on a line without text.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WordResult(pub Option<WordSegment>);
impl HostSuccess for WordResult {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(self.0)
    }
}

/// The word, separator run, or whitespace run at a byte offset on a
/// character boundary.
#[host_operation("wordAt")]
pub(crate) async fn word_at(
    context: HostContext,
    arguments: LineArguments,
) -> Result<WordResult, BufferError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let commands = &host.commands;
        host.buffers.read_model(
            &request,
            arguments.buffer,
            || commands.is_cancelled(request.invocation),
            cx,
            |model| {
                let offset = model
                    .text_offset(arguments.value)
                    .ok_or(BufferError::InvalidRange)?;
                Ok(WordResult(knot_model::editor::word_at(model, offset).map(
                    |(range, class)| WordSegment {
                        range: ByteRange {
                            start_byte_offset: range.start,
                            end_byte_offset: range.end,
                        },
                        kind: match class {
                            knot_model::editor::WordClass::Word => WordKind::Word,
                            knot_model::editor::WordClass::Separator => WordKind::Separator,
                            knot_model::editor::WordClass::Whitespace => WordKind::Space,
                        },
                    },
                )))
            },
        )
    })?
}

/// A position that follows edits, or `None` once its text is gone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MarkPosition(pub Option<u64>);
impl HostSuccess for MarkPosition {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(self.0)
    }
}

/// Create a lifecycle-owned mark at a byte offset on a character boundary.
#[host_operation("createMark")]
pub(crate) async fn create_mark(
    context: HostContext,
    arguments: LineArguments,
) -> Result<u64, BufferError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let commands = &host.commands;
        let offset = usize::try_from(arguments.value).map_err(|_| BufferError::InvalidRange)?;
        host.buffers.create_mark(
            &request,
            arguments.buffer,
            offset,
            || commands.is_cancelled(request.invocation),
            cx,
        )
    })?
}

/// The current byte offset of a mark, or null once its text was deleted.
#[host_operation("markPosition")]
pub(crate) async fn mark_position(
    context: HostContext,
    arguments: LineArguments,
) -> Result<MarkPosition, BufferError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let commands = &host.commands;
        let source = crate::app::model::ContributionSource {
            extension: request.extension,
            lifecycle: request.lifecycle,
        };
        host.buffers.read_model(
            &request,
            arguments.buffer,
            || commands.is_cancelled(request.invocation),
            cx,
            |model| {
                Ok(MarkPosition(
                    model
                        .mark_position(source, arguments.value)
                        .map(|offset| offset as u64),
                ))
            },
        )
    })?
}

/// Remove a mark; one already gone is not an error.
#[host_operation("disposeMark")]
pub(crate) async fn dispose_mark(
    context: HostContext,
    arguments: LineArguments,
) -> Result<(), BufferError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let commands = &host.commands;
        host.buffers.dispose_mark(
            &request,
            arguments.buffer,
            arguments.value,
            || commands.is_cancelled(request.invocation),
            cx,
        )
    })?
}

#[host_operation("applyEdits")]
pub(crate) async fn apply_edits(
    context: HostContext,
    arguments: EditArguments,
) -> Result<u64, BufferError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let commands = &host.commands;
        host.buffers.apply_edits(
            &request,
            arguments,
            || commands.is_cancelled(request.invocation),
            cx,
        )
    })?
}

#[host_operation("subscribeBufferChanges")]
pub(crate) async fn subscribe_buffer_changes(
    context: HostContext,
    arguments: BufferArguments,
) -> Result<BufferSubscriptionId, BufferError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let commands = &host.commands;
        host.buffers.subscribe_buffer_changes(
            &request,
            arguments,
            || commands.is_cancelled(request.invocation),
            cx,
        )
    })?
}

#[host_operation("unsubscribeBufferChanges")]
pub(crate) async fn unsubscribe_buffer_changes(
    context: HostContext,
    arguments: SubscriptionArguments,
) -> Result<(), BufferError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let commands = &host.commands;
        host.buffers.unsubscribe_buffer_changes(
            &request,
            arguments,
            || commands.is_cancelled(request.invocation),
            cx,
        )
    })?
}

#[host_operation("replaceEditorContributions")]
pub(crate) async fn replace_editor_contributions(
    context: HostContext,
    arguments: ContributionArguments,
) -> Result<(), BufferError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let commands = &host.commands;
        host.buffers.replace_editor_contributions(
            &request,
            arguments,
            || commands.is_cancelled(request.invocation),
            cx,
        )
    })?
}

#[host_operation("disposeEditorContributions")]
pub(crate) async fn dispose_editor_contributions(
    context: HostContext,
    arguments: BufferArguments,
) -> Result<(), BufferError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let commands = &host.commands;
        host.buffers.dispose_editor_contributions(
            &request,
            arguments,
            || commands.is_cancelled(request.invocation),
            cx,
        )
    })?
}

use crate::app::extensions::semantics::{SemanticError, ViewPresentation};
use knot_protocol::{
    ConfirmationResult, FolderPickerResult, InputCompletion, InputResult, PickerItem, PickerResult,
    PickerRow, PickerTab, ProviderIdentity, TreeAction, TreeProviderRegistrationId,
    ViewContentKind,
};

use crate::app::transient::{InputSpec, PromptText, TransientOutcome, TransientOwner};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TreeArguments {
    pub id: String,
    pub title: String,
    pub content: ViewContentKind,
    pub actions: Vec<TreeAction>,
    pub location: crate::app::workbench::Slot,
    pub order: Option<u64>,
    pub visible: bool,
    pub language: Option<String>,
    /// Whether a text frame shows the editor gutter.
    pub gutter: bool,
    /// A container's command that opens a tab in it.
    pub new_tab: Option<String>,
    /// A tree's command that runs when a row is dropped onto another.
    pub drop: Option<String>,
    /// Whether composites host the view instead of a tool window.
    pub embedded: bool,
}
impl HostArguments for TreeArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let location = match reader.text(4, "view placement must be text")?.as_str() {
            "leftTop" => crate::app::workbench::Slot::LeftTop,
            "leftBottom" => crate::app::workbench::Slot::LeftBottom,
            "rightTop" => crate::app::workbench::Slot::RightTop,
            "rightBottom" => crate::app::workbench::Slot::RightBottom,
            "bottomLeft" => crate::app::workbench::Slot::BottomLeft,
            "bottomRight" => crate::app::workbench::Slot::BottomRight,
            _ => return Err(DecodeError::Type("invalid view placement")),
        };
        let content = match reader.text(7, "view content kind must be text")?.as_str() {
            "tree" => ViewContentKind::Tree,
            "text" => ViewContentKind::Text,
            "container" => ViewContentKind::Container,
            _ => return Err(DecodeError::Type("invalid view content kind")),
        };
        let actions = serde_json::to_value(reader.command_arguments(8)?)
            .ok()
            .and_then(|value| serde_json::from_value::<Vec<TreeAction>>(value).ok())
            .ok_or(DecodeError::Type("invalid view actions"))?;
        Ok(Self {
            id: reader.text(2, "view id must be text")?,
            title: reader.text(3, "view title must be text")?,
            content,
            actions,
            location,
            order: reader.optional_integer(5)?,
            visible: reader.integer(6)? != 0,
            language: reader.optional_text(9, "view language must be text or null")?,
            gutter: reader.integer(10)? != 0,
            new_tab: reader.optional_text(11, "view new tab command must be text or null")?,
            drop: reader.optional_text(12, "view drop command must be text or null")?,
            embedded: reader.integer(13)? != 0,
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TreeRegistrationArguments {
    pub registration: TreeProviderRegistrationId,
}
impl HostArguments for TreeRegistrationArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            registration: TreeProviderRegistrationId::new(reader.integer(2)?),
        })
    }
    fn provider_mutation(&self) -> Option<ProviderIdentity> {
        Some(ProviderIdentity::Tree(self.registration))
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TreeInvalidationArguments {
    pub registration: TreeProviderRegistrationId,
    pub parent_id: Option<String>,
    pub window: Option<String>,
}
impl HostArguments for TreeInvalidationArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            registration: TreeProviderRegistrationId::new(reader.integer(2)?),
            parent_id: reader.optional_text(3, "UnsupportedOperation")?,
            window: reader.optional_text(4, "window target must be text")?,
        })
    }
    fn provider_mutation(&self) -> Option<ProviderIdentity> {
        Some(ProviderIdentity::Tree(self.registration))
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TreeRevealArguments {
    pub registration: TreeProviderRegistrationId,
    pub id_path: Vec<String>,
    pub window: Option<String>,
}
impl HostArguments for TreeRevealArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            registration: TreeProviderRegistrationId::new(reader.integer(2)?),
            id_path: reader.text_list(3)?,
            window: reader.optional_text(4, "window target must be text")?,
        })
    }
    fn provider_mutation(&self) -> Option<ProviderIdentity> {
        Some(ProviderIdentity::Tree(self.registration))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReplaceViewTextArguments {
    pub registration: TreeProviderRegistrationId,
    pub text: String,
    pub window: Option<String>,
}
impl HostArguments for ReplaceViewTextArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            registration: TreeProviderRegistrationId::new(reader.integer(2)?),
            text: reader.text(3, "view text must be text")?,
            window: reader.optional_text(4, "window target must be text")?,
        })
    }
    fn provider_mutation(&self) -> Option<ProviderIdentity> {
        Some(ProviderIdentity::Tree(self.registration))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ViewBadgeArguments {
    pub registration: TreeProviderRegistrationId,
    pub badge: Option<crate::app::workbench::ToolBadge>,
}
impl HostArguments for ViewBadgeArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        use crate::app::workbench::ToolBadge;
        let badge = match reader
            .optional_text(3, "view badge must be text or null")?
            .as_deref()
        {
            None => None,
            Some("info") => Some(ToolBadge::Info),
            Some("error") => Some(ToolBadge::Error),
            Some(_) => return Err(DecodeError::Type("invalid view badge")),
        };
        Ok(Self {
            registration: TreeProviderRegistrationId::new(reader.integer(2)?),
            badge,
        })
    }
    fn provider_mutation(&self) -> Option<ProviderIdentity> {
        Some(ProviderIdentity::Tree(self.registration))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ViewOperationArguments {
    pub registration: TreeProviderRegistrationId,
    pub window: Option<String>,
}
impl HostArguments for ViewOperationArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            registration: TreeProviderRegistrationId::new(reader.integer(2)?),
            window: reader.optional_text(3, "window target must be text")?,
        })
    }
    fn provider_mutation(&self) -> Option<ProviderIdentity> {
        Some(ProviderIdentity::Tree(self.registration))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PickerArguments {
    pub title: String,
    pub items: Vec<PickerItem>,
    pub target: Option<String>,
}
impl HostArguments for PickerArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let items = serde_json::to_value(reader.command_arguments(3)?)
            .ok()
            .and_then(|value| serde_json::from_value::<Vec<PickerItem>>(value).ok())
            .ok_or(DecodeError::Type("invalid picker items"))?;
        Ok(Self {
            title: reader.text(2, "picker title must be text")?,
            items,
            target: reader.optional_text(4, "window target must be text")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InputArguments {
    pub title: String,
    pub value: Option<String>,
    pub prompt: Option<String>,
    pub target: Option<String>,
    pub history: Vec<String>,
    pub placeholder: Option<String>,
}
impl HostArguments for InputArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            title: reader.text(2, "input title must be text")?,
            value: reader.optional_text(3, "input value must be text or null")?,
            prompt: reader.optional_text(4, "input prompt must be text or null")?,
            target: reader.optional_text(5, "window target must be text")?,
            history: reader.text_list(6)?,
            placeholder: reader.optional_text(7, "input placeholder must be text or null")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ConfirmationArguments {
    pub title: String,
    pub message: Option<String>,
    pub confirm_label: Option<String>,
    pub cancel_label: Option<String>,
    pub target: Option<String>,
}
impl HostArguments for ConfirmationArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            title: reader.text(2, "confirmation title must be text")?,
            message: reader.optional_text(3, "confirmation message must be text or null")?,
            confirm_label: reader.optional_text(4, "confirm label must be text or null")?,
            cancel_label: reader.optional_text(5, "cancel label must be text or null")?,
            target: reader.optional_text(6, "window target must be text")?,
        })
    }
}

fn transient_owner(request: &knot_protocol::RequestContext) -> TransientOwner {
    TransientOwner {
        extension: request.extension,
        lifecycle: request.lifecycle,
        invocation: request.invocation,
    }
}

#[host_operation("showPicker")]
pub(crate) async fn show_picker(
    context: HostContext,
    arguments: PickerArguments,
) -> Result<PickerResult, TransientError> {
    let request = context.request;
    let owner = transient_owner(&request);
    let PickerArguments {
        title,
        items,
        target,
    } = arguments;
    let receiver = context.foreground(|host, cx| {
        host.with_transient_window(
            &request,
            target.as_deref(),
            move |window, cx| window.begin_picker(owner, title, items, None, cx),
            cx,
        )
    })??;
    match receiver.await {
        Ok(TransientOutcome::Picker(id)) => Ok(PickerResult::Selected { id }),
        Ok(TransientOutcome::Dismissed) => Ok(PickerResult::Dismissed),
        _ => Err(TransientError::Cancelled),
    }
}

/// The values a surface passes its callback for an answer, and what becomes
/// of each answer.
struct Answered<Q> {
    values: Option<tokio::sync::watch::Receiver<Q>>,
    /// Whether a value is passed while an earlier call still runs.
    concurrent: bool,
    argument: fn(&Q) -> serde_json::Value,
    apply: ApplyAnswer<Q>,
}

/// Shows a callback's answer as JSON text for a value, or `None` when the
/// call failed.
type ApplyAnswer<Q> = Box<dyn FnMut(&HostContext, &Q, Option<String>)>;

/// Settles with a surface's outcome while calling the callback the command
/// passed with it (the `session` the bootstrap numbered) as tasks of the
/// command's package. A new value of `answered` is passed to the callback at
/// once when the source is concurrent, and otherwise once the previous call
/// has returned; its answer goes to `apply` only when no later value was
/// passed meanwhile, as `None` when the call failed. Each new highlight is
/// passed once the previous one's call has returned. A failing call stops
/// further calls, unless its answer was dropped for a later one; values of a
/// serial source then still go to `apply` as `None`, so a field waiting for
/// an answer to its text gets one. Concurrent answers still pending when the surface settles
/// are dropped, but the outcome waits for the calls of a serial source and
/// for a highlight's, so a preview never lands after the command resumes.
async fn follow_surface<Q: Clone>(
    context: &HostContext,
    session: u64,
    mut completion: tokio::sync::oneshot::Receiver<TransientOutcome>,
    answered: Answered<Q>,
    mut highlights: Option<tokio::sync::watch::Receiver<Option<String>>>,
) -> Result<TransientOutcome, TransientError> {
    use futures::{FutureExt as _, StreamExt as _, future::LocalBoxFuture};
    use std::task::Poll;

    enum Next<Q> {
        Settled(Result<TransientOutcome, tokio::sync::oneshot::error::RecvError>),
        Answer(u64, Q, Option<String>),
        Highlighted(Option<String>),
        Answered(bool),
        Highlight(bool),
    }
    let Answered {
        values: mut answered,
        concurrent,
        argument,
        mut apply,
    } = answered;
    let key = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    let call = |argument: serde_json::Value| -> LocalBoxFuture<'static, Option<String>> {
        let execution = context
            .foreground(|host, _| {
                host.pool.call_surface(
                    key,
                    knot_protocol::SurfaceCallbackRequest {
                        session,
                        argument: argument.to_string(),
                    },
                )
            })
            .ok()
            .and_then(Result::ok);
        async move { execution?.await.ok()?.ok() }.boxed_local()
    };
    let mut answers = futures::stream::FuturesUnordered::new();
    let mut highlight: Option<LocalBoxFuture<'static, Option<String>>> = None;
    let mut generation = 0;
    let mut failed = false;
    let outcome = loop {
        let next = {
            let answering = !concurrent && !answers.is_empty();
            let mut answer = std::pin::pin!(async {
                match answered
                    .as_mut()
                    .filter(|_| !(failed && concurrent) && !answering)
                {
                    Some(answered) => answered.changed().await.is_ok(),
                    None => std::future::pending().await,
                }
            });
            let highlighting = highlight.is_some();
            let mut highlighted = std::pin::pin!(async {
                match highlights.as_mut().filter(|_| !failed && !highlighting) {
                    Some(highlights) => highlights.changed().await.is_ok(),
                    None => std::future::pending().await,
                }
            });
            std::future::poll_fn(|task| {
                if let Poll::Ready(outcome) = std::pin::Pin::new(&mut completion).poll(task) {
                    return Poll::Ready(Next::Settled(outcome));
                }
                if let Some(Poll::Ready(result)) =
                    highlight.as_mut().map(|call| call.poll_unpin(task))
                {
                    return Poll::Ready(Next::Highlighted(result));
                }
                if let Poll::Ready(Some((generation, value, result))) =
                    answers.poll_next_unpin(task)
                {
                    return Poll::Ready(Next::Answer(generation, value, result));
                }
                if let Poll::Ready(open) = answer.as_mut().poll(task) {
                    return Poll::Ready(Next::Answered(open));
                }
                highlighted.as_mut().poll(task).map(Next::Highlight)
            })
            .await
        };
        match next {
            Next::Settled(outcome) => break outcome,
            Next::Highlighted(result) => {
                highlight = None;
                failed |= result.is_none();
            }
            Next::Answer(answered, value, result) => {
                if answered == generation {
                    failed |= result.is_none();
                    apply(context, &value, result);
                }
            }
            Next::Answered(false) => answered = None,
            Next::Answered(true) => {
                let value = answered
                    .as_mut()
                    .expect("answered values")
                    .borrow_and_update()
                    .clone();
                if failed {
                    apply(context, &value, None);
                    continue;
                }
                generation += 1;
                let pending = call(argument(&value));
                answers.push(pending.map(move |result| (generation, value, result)));
            }
            Next::Highlight(false) => highlights = None,
            Next::Highlight(true) => {
                let id = highlights
                    .as_mut()
                    .expect("highlights")
                    .borrow_and_update()
                    .clone();
                highlight = Some(call(serde_json::json!({ "kind": "highlight", "id": id })));
            }
        }
    };
    if let Some(highlight) = highlight {
        highlight.await;
    }
    if !concurrent {
        while answers.next().await.is_some() {}
    }
    outcome.map_err(|_| TransientError::Cancelled)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FollowPickerArguments {
    picker: PickerArguments,
    /// The callback session the bootstrap numbered for this picker.
    session: u64,
    /// Whether the callback supplies the rows for each query.
    queried: bool,
    /// Whether the callback follows the highlighted item.
    highlights: bool,
    tabs: Vec<PickerTab>,
    /// The index of the tab that starts active.
    initial_tab: usize,
}
impl HostArguments for FollowPickerArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            picker: PickerArguments::decode(reader)?,
            session: reader.integer(5)?,
            queried: reader.integer(6)? != 0,
            highlights: reader.integer(7)? != 0,
            tabs: serde_json::to_value(reader.command_arguments(8)?)
                .ok()
                .and_then(|value| serde_json::from_value::<Vec<PickerTab>>(value).ok())
                .ok_or(DecodeError::Type("invalid picker tabs"))?,
            initial_tab: reader.integer(9)? as usize,
        })
    }
}

/// Open a picker with tabs, or whose callback follows its highlight or
/// supplies the rows for each query, and settle with its outcome.
#[host_operation("followPicker")]
pub(crate) async fn follow_picker(
    context: HostContext,
    arguments: FollowPickerArguments,
) -> Result<PickerResult, TransientError> {
    let request = context.request;
    let owner = transient_owner(&request);
    let FollowPickerArguments {
        picker:
            PickerArguments {
                title,
                items,
                target,
            },
        session,
        queried,
        highlights,
        tabs,
        initial_tab,
    } = arguments;
    let (sender, changes) = tokio::sync::watch::channel(None);
    let (query_sender, queries) = tokio::sync::watch::channel(None);
    let (completion, picker) = context.foreground(|host, cx| {
        host.with_transient_window(
            &request,
            target.as_deref(),
            move |window, cx| {
                if queried {
                    let (completion, picker) = window.begin_queried_picker(
                        owner,
                        title,
                        (tabs, initial_tab),
                        sender,
                        query_sender,
                        cx,
                    )?;
                    Ok((completion, Some(picker)))
                } else {
                    let completion = window.begin_tabbed_picker(
                        owner,
                        title,
                        items,
                        (tabs, initial_tab),
                        Some(sender),
                        cx,
                    )?;
                    Ok((completion, None))
                }
            },
            cx,
        )
    })??;
    let answered = Answered {
        values: picker.is_some().then_some(queries),
        concurrent: true,
        argument: |ask: &Option<crate::app::picker::PickerAsk>| {
            let (query, tab) = ask.clone().unwrap_or_default();
            serde_json::json!({ "kind": "query", "query": query, "tab": tab })
        },
        apply: Box::new(
            move |context: &HostContext,
                  ask: &Option<crate::app::picker::PickerAsk>,
                  rows: Option<String>| {
                let (Some(picker), Some(Ok(rows))) = (
                    picker.as_ref().and_then(WeakEntity::upgrade),
                    rows.map(|rows| serde_json::from_str::<Vec<PickerRow>>(&rows)),
                ) else {
                    return;
                };
                let (query, tab) = ask.clone().unwrap_or_default();
                let entries = rows.into_iter().map(picker_entry).collect();
                let _ = context.foreground(|_, cx| {
                    picker.update(cx, |picker, cx| {
                        picker.supply(&query, tab.as_deref(), entries, cx)
                    })
                });
            },
        ),
    };
    let highlights = highlights.then_some(changes);
    match follow_surface(&context, session, completion, answered, highlights).await? {
        TransientOutcome::Picker(id) => Ok(PickerResult::Selected { id }),
        TransientOutcome::Dismissed => Ok(PickerResult::Dismissed),
        _ => Err(TransientError::Cancelled),
    }
}

fn picker_entry(row: PickerRow) -> crate::app::picker::PickerEntry {
    let PickerRow {
        item,
        label_matches,
        description_matches,
    } = row;
    let ranges = |text: &str, matches: Option<Vec<[usize; 2]>>| {
        matches.map_or_else(Vec::new, |matches| {
            knot_model::matching::byte_ranges(text, &matches)
        })
    };
    crate::app::picker::PickerEntry {
        id: Some(item.id),
        label_matches: ranges(&item.label, label_matches),
        description_matches: item
            .description
            .as_deref()
            .map_or_else(Vec::new, |text| ranges(text, description_matches)),
        description: item.description,
        detail: item.detail,
        hint: item.hint,
        icon: item.icon,
        tabs: item.tabs,
        ..crate::app::picker::PickerEntry::new(item.label)
    }
}

/// What a callback following a text field answers for one value.
#[derive(Default, serde::Deserialize)]
struct FieldFeedback {
    message: Option<String>,
    #[serde(default)]
    items: Vec<InputCompletion>,
}

/// The answers of a field's callback to `changes`, serially: each value is
/// passed as JSON text, and `feedback` shows the answer for the value it
/// answers, or no message and completions when the callback failed.
fn field_answers(
    changes: tokio::sync::watch::Receiver<Option<String>>,
    feedback: impl Fn(&HostContext, &str, FieldFeedback) + 'static,
) -> Answered<Option<String>> {
    Answered {
        values: Some(changes),
        concurrent: false,
        argument: |value: &Option<String>| {
            serde_json::Value::from(value.clone().unwrap_or_default())
        },
        apply: Box::new(move |context, value, answer| {
            let answer = match answer {
                Some(answer) => serde_json::from_str::<FieldFeedback>(&answer).unwrap_or_default(),
                None => FieldFeedback::default(),
            };
            feedback(context, value.as_deref().unwrap_or_default(), answer);
        }),
    }
}

#[host_operation("showInput")]
pub(crate) async fn show_input(
    context: HostContext,
    arguments: InputArguments,
) -> Result<InputResult, TransientError> {
    let request = context.request;
    let owner = transient_owner(&request);
    let InputArguments {
        title,
        value,
        prompt,
        target,
        history,
        placeholder,
    } = arguments;
    let (receiver, _) = context.foreground(|host, cx| {
        host.with_transient_window(
            &request,
            target.as_deref(),
            move |window, cx| {
                window.begin_input(
                    owner,
                    InputSpec {
                        title,
                        initial: value.unwrap_or_default(),
                        prompt,
                        history,
                        placeholder,
                    },
                    None,
                    cx,
                )
            },
            cx,
        )
    })??;
    match receiver.await {
        Ok(TransientOutcome::Input(value, highlighted)) => {
            Ok(InputResult::Accepted { value, highlighted })
        }
        Ok(TransientOutcome::Dismissed) => Ok(InputResult::Dismissed),
        _ => Err(TransientError::Cancelled),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FollowInputArguments {
    input: InputArguments,
    /// The callback session the bootstrap numbered for this input.
    session: u64,
}
impl HostArguments for FollowInputArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            input: InputArguments::decode(reader)?,
            session: reader.integer(8)?,
        })
    }
}

/// Open an input whose callback answers each value with a message and
/// completions, and settle with its outcome.
#[host_operation("followInput")]
pub(crate) async fn follow_input(
    context: HostContext,
    arguments: FollowInputArguments,
) -> Result<InputResult, TransientError> {
    let request = context.request;
    let owner = transient_owner(&request);
    let FollowInputArguments {
        input:
            InputArguments {
                title,
                value,
                prompt,
                target,
                history,
                placeholder,
            },
        session,
    } = arguments;
    let (sender, changes) = tokio::sync::watch::channel(None);
    let (completion, input) = context.foreground(|host, cx| {
        host.with_transient_window(
            &request,
            target.as_deref(),
            move |window, cx| {
                window.begin_input(
                    owner,
                    InputSpec {
                        title,
                        initial: value.unwrap_or_default(),
                        prompt,
                        history,
                        placeholder,
                    },
                    Some(sender),
                    cx,
                )
            },
            cx,
        )
    })??;
    let answered = field_answers(changes, move |context, value, feedback| {
        let Some(input) = input.upgrade() else { return };
        let _ = context.foreground(|_, cx| {
            input.update(cx, |input, cx| {
                if input.text(cx) == value {
                    input.set_feedback(feedback.message, feedback.items, cx);
                }
            })
        });
    });
    match follow_surface(&context, session, completion, answered, None).await? {
        TransientOutcome::Input(value, highlighted) => {
            Ok(InputResult::Accepted { value, highlighted })
        }
        TransientOutcome::Dismissed => Ok(InputResult::Dismissed),
        _ => Err(TransientError::Cancelled),
    }
}

/// Failures of an inline edit of a tree row.
#[derive(Debug, HostError)]
pub(crate) enum TreeEditError {
    #[error(transparent)]
    Semantic(SemanticError),
    #[error(transparent)]
    Transient(TransientError),
    #[error(code = "TreeItemNotFound", message = "TreeItemNotFound")]
    ItemNotFound,
}

impl From<SemanticError> for TreeEditError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

impl From<TransientError> for TreeEditError {
    fn from(error: TransientError) -> Self {
        Self::Transient(error)
    }
}

impl From<AccessError> for TreeEditError {
    fn from(error: AccessError) -> Self {
        Self::Semantic(error.into())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TreeEditArguments {
    pub registration: TreeProviderRegistrationId,
    /// Whether the edit makes a new row rather than renaming one.
    pub new_row: bool,
    /// The row to rename, or the parent of the new row.
    pub id: String,
    pub value: String,
    /// UTF-16 code unit range of `value` to select; all when omitted.
    pub selection: Option<[usize; 2]>,
    pub icon: Option<knot_protocol::IconRef>,
    /// The callback session that validates each value, if any.
    pub session: Option<u64>,
    pub window: Option<String>,
}

impl HostArguments for TreeEditArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let selection = serde_json::to_value(reader.command_arguments(6)?)
            .ok()
            .and_then(|value| serde_json::from_value::<Option<[f64; 2]>>(value).ok())
            .ok_or(DecodeError::Type("edit selection must be two offsets"))?
            .map(|[start, end]| [start as usize, end as usize]);
        let icon = serde_json::to_value(reader.command_arguments(7)?)
            .ok()
            .and_then(|value| serde_json::from_value::<Option<knot_protocol::IconRef>>(value).ok())
            .ok_or(DecodeError::Type("invalid edit icon"))?;
        Ok(Self {
            registration: TreeProviderRegistrationId::new(reader.integer(2)?),
            new_row: reader.integer(3)? != 0,
            id: reader.text(4, "tree item ID must be text")?,
            value: reader
                .optional_text(5, "edit value must be text or null")?
                .unwrap_or_default(),
            selection,
            icon,
            session: reader.optional_integer(8)?,
            window: reader.optional_text(9, "window target must be text")?,
        })
    }
}

/// Edit a row's label or a new row inline, with a callback that validates
/// each value, and settle with the edit's outcome.
#[host_operation("editTreeItem")]
pub(crate) async fn edit_tree_item(
    context: HostContext,
    arguments: TreeEditArguments,
) -> Result<InputResult, TreeEditError> {
    let request = context.request;
    let session = arguments.session;
    let (edit, tree) =
        context.foreground(|host, cx| host.begin_tree_edit(&request, arguments, cx))??;
    let outcome = match session {
        Some(session) => {
            let answered = field_answers(edit.changes, move |context, value, feedback| {
                let Some(tree) = tree.upgrade() else { return };
                let _ = context.foreground(|_, cx| {
                    tree.update(cx, |tree, cx| {
                        tree.set_edit_feedback(Some(value), feedback.message, cx)
                    })
                });
            });
            follow_surface(&context, session, edit.completion, answered, None).await?
        }
        None => edit
            .completion
            .await
            .map_err(|_| TransientError::Cancelled)?,
    };
    match outcome {
        TransientOutcome::Input(value, highlighted) => {
            Ok(InputResult::Accepted { value, highlighted })
        }
        TransientOutcome::Dismissed => Ok(InputResult::Dismissed),
        _ => Err(TransientError::Cancelled.into()),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MatchArguments {
    pub query: String,
    pub candidates: Vec<String>,
    pub styles: Option<Vec<String>>,
}
impl HostArguments for MatchArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let candidates = serde_json::to_value(reader.command_arguments(3)?)
            .ok()
            .and_then(|value| serde_json::from_value::<Vec<String>>(value).ok())
            .ok_or(DecodeError::Type("match candidates must be text"))?;
        let styles = serde_json::to_value(reader.command_arguments(4)?)
            .ok()
            .and_then(|value| serde_json::from_value::<Option<Vec<String>>>(value).ok())
            .ok_or(DecodeError::Type("match styles must be text"))?;
        Ok(Self {
            query: reader.text(2, "match query must be text")?,
            candidates,
            styles,
        })
    }
}

/// Match candidates against a query in the user's `completion.styles`, or the
/// styles the caller names, and return the matches best first.
#[host_operation("matchCandidates")]
async fn match_candidates(
    context: HostContext,
    arguments: MatchArguments,
) -> Result<Vec<knot_protocol::MatchedCandidate>, AccessError> {
    use knot_model::matching::{Matcher, Query, styles_from_names, utf16_ranges};

    let MatchArguments {
        query,
        candidates,
        styles,
    } = arguments;
    let names = match styles {
        Some(names) => names,
        None => context.foreground(|_, cx| {
            crate::app::settings::strings(crate::app::settings::COMPLETION_STYLES, cx)
        })?,
    };
    let query = Query::new(&query, &styles_from_names(&names));
    Ok(Matcher::new()
        .rank(&query, &candidates)
        .into_iter()
        .map(|(index, found)| knot_protocol::MatchedCandidate {
            index,
            ranges: utf16_ranges(&candidates[index], &found.ranges),
        })
        .collect())
}

#[host_operation("showConfirmation")]
pub(crate) async fn show_confirmation(
    context: HostContext,
    arguments: ConfirmationArguments,
) -> Result<ConfirmationResult, TransientError> {
    let request = context.request;
    let owner = transient_owner(&request);
    let ConfirmationArguments {
        title,
        message,
        confirm_label,
        cancel_label,
        target,
    } = arguments;
    let receiver = context.foreground(|host, cx| {
        host.with_transient_window(
            &request,
            target.as_deref(),
            move |window, cx| {
                window.begin_confirmation(
                    Some(owner),
                    PromptText { title, message },
                    confirm_label.unwrap_or_else(|| "OK".to_owned()),
                    cancel_label.unwrap_or_else(|| "Cancel".to_owned()),
                    cx,
                )
            },
            cx,
        )
    })??;
    match receiver.await {
        Ok(TransientOutcome::Confirmed) => Ok(ConfirmationResult::Confirmed),
        Ok(TransientOutcome::Dismissed) => Ok(ConfirmationResult::Dismissed),
        _ => Err(TransientError::Cancelled),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FolderPickerArguments {
    pub target: Option<String>,
}

impl HostArguments for FolderPickerArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            target: reader.optional_text(2, "window target must be text")?,
        })
    }
}

#[host_operation("pickFolder")]
pub(crate) async fn pick_folder(
    context: HostContext,
    arguments: FolderPickerArguments,
) -> Result<FolderPickerResult, FolderPickerError> {
    let request = context.request;
    let owner = transient_owner(&request);
    let receiver = context.foreground(|host, cx| {
        host.with_transient_window(
            &request,
            arguments.target.as_deref(),
            move |window, cx| window.begin_folder_picker(Some(owner), cx),
            cx,
        )
    })??;
    let path = match receiver.await {
        Ok(TransientOutcome::Folder(path)) => path,
        Ok(TransientOutcome::Dismissed) => return Ok(FolderPickerResult::Dismissed),
        Ok(TransientOutcome::Failed(message)) => return Err(FolderPickerError::Dialog { message }),
        _ => return Err(TransientError::Cancelled.into()),
    };
    let (sender, response) = tokio::sync::oneshot::channel();
    context.foreground(|_, cx| {
        product::operations::resolve_directory(
            path,
            move |result| {
                let _ = sender.send(result);
            },
            cx,
        )
    })?;
    let result = response.await.map_err(|_| TransientError::Cancelled)?;
    let uri = context.foreground(|_, _| result)??;
    Ok(FolderPickerResult::Selected {
        uri: uri.to_string(),
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FileUriArguments {
    uri: String,
}

impl HostArguments for FileUriArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            uri: reader.text(2, "resource URI must be text")?,
        })
    }
}

fn file_kind(kind: knot_model::filesystem::ResourceKind) -> knot_protocol::FileKind {
    match kind {
        knot_model::filesystem::ResourceKind::File => knot_protocol::FileKind::File,
        knot_model::filesystem::ResourceKind::Directory => knot_protocol::FileKind::Directory,
    }
}

#[host_operation("readFileText")]
async fn read_file_text(
    context: HostContext,
    arguments: FileUriArguments,
) -> Result<String, OpenError> {
    let providers = context.foreground(|_, cx| KnotWindow::filesystems(cx))?;
    knot_model::files::read_text(providers, &arguments.uri)
        .await
        .map_err(product::operations::resource_error)
}

#[host_operation("listFiles")]
async fn list_files(
    context: HostContext,
    arguments: FileUriArguments,
) -> Result<Vec<knot_protocol::FileEntry>, OpenError> {
    let (providers, policy) = context.foreground(|_, cx| {
        (
            KnotWindow::filesystems(cx),
            crate::app::settings::files_ignore_policy(cx),
        )
    })?;
    let indexed = match indexed_uri(&context, &providers, &arguments.uri).await {
        Some((index, uri)) => index.list(&uri, &policy).await,
        None => None,
    };
    let entries = match indexed {
        Some(entries) => entries,
        None => knot_model::files::list(providers, &arguments.uri, &policy)
            .await
            .map_err(|error| product::operations::resource_error(error.into()))?,
    };
    Ok(entries
        .into_iter()
        .map(|listed| knot_protocol::FileEntry {
            uri: listed.entry.uri.to_string(),
            name: listed.entry.name,
            kind: file_kind(listed.entry.kind),
            ignored: listed.ignored,
        })
        .collect())
}

/// The most results a `files.search` returns when its caller names no limit.
const DEFAULT_SEARCH_LIMIT: u64 = 100;
/// The most results a `files.search` returns.
const MAXIMUM_SEARCH_LIMIT: u64 = 1000;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SearchFilesArguments {
    directories: Vec<String>,
    query: String,
    limit: Option<u64>,
}

impl HostArguments for SearchFilesArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            directories: reader.text_list(2)?,
            query: reader.text(3, "search query must be text")?,
            limit: reader.optional_integer(4)?,
        })
    }
}

#[host_operation("searchFiles")]
async fn search_files(
    context: HostContext,
    arguments: SearchFilesArguments,
) -> Result<knot_protocol::FileSearchResult, OpenError> {
    use knot_model::matching::{Query, styles_from_names, utf16_ranges};

    let (providers, policy, styles) = context.foreground(|_, cx| {
        (
            KnotWindow::filesystems(cx),
            crate::app::settings::files_ignore_policy(cx),
            crate::app::settings::strings(crate::app::settings::COMPLETION_STYLES, cx),
        )
    })?;
    let query = Query::new(&arguments.query, &styles_from_names(&styles));
    let limit = arguments
        .limit
        .unwrap_or(DEFAULT_SEARCH_LIMIT)
        .clamp(1, MAXIMUM_SEARCH_LIMIT) as usize;
    let mut search = knot_model::files::Search::new(&query, limit);
    for directory in &arguments.directories {
        let indexed = match indexed_uri(&context, &providers, directory).await {
            Some((index, uri)) => index.offer(&uri, &policy, &mut search).await,
            None => false,
        };
        if !indexed {
            knot_model::files::offer(providers.clone(), directory, &policy, &mut search)
                .await
                .map_err(|error| product::operations::resource_error(error.into()))?;
        }
    }
    let found = search.finish();
    Ok(knot_protocol::FileSearchResult {
        results: found
            .hits
            .into_iter()
            .map(|hit| knot_protocol::FileSearchHit {
                ranges: utf16_ranges(&hit.path, &hit.ranges),
                uri: hit.uri.to_string(),
                directory: hit.directory.to_string(),
                path: hit.path,
                name: hit.name,
            })
            .collect(),
        more: found.more,
    })
}

#[host_operation("isFileIgnored")]
async fn is_file_ignored(
    context: HostContext,
    arguments: FileUriArguments,
) -> Result<bool, OpenError> {
    let (providers, policy) = context.foreground(|_, cx| {
        (
            KnotWindow::filesystems(cx),
            crate::app::settings::files_ignore_policy(cx),
        )
    })?;
    let indexed = match indexed_uri(&context, &providers, &arguments.uri).await {
        Some((index, uri)) => index.is_ignored(&uri, &policy).await,
        None => None,
    };
    match indexed {
        Some(ignored) => Ok(ignored),
        None => knot_model::files::is_ignored(providers, &arguments.uri, &policy)
            .await
            .map_err(|error| product::operations::resource_error(error.into())),
    }
}

/// The index that answers for `uri` and the normalized URI, or `None` when no
/// leased root contains it and the disk must answer.
async fn indexed_uri(
    context: &HostContext,
    providers: &knot_model::filesystem::FileSystemProviderRegistry,
    uri: &str,
) -> Option<(
    Arc<knot_model::files::FileIndex>,
    knot_model::resource::ResourceUri,
)> {
    let uri = knot_model::files::normalize(providers, uri).await.ok()?;
    let index = context
        .foreground(|_, cx| crate::app::file_index::ApplicationFileIndex::covering(cx, &uri))
        .ok()??;
    Some((index, uri))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WatchFilesArguments {
    uri: String,
    recursive: bool,
    include_hidden: bool,
}

impl HostArguments for WatchFilesArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            uri: reader.text(2, "resource URI must be text")?,
            recursive: reader
                .optional_boolean(3, "watch recursive must be a boolean")?
                .unwrap_or(false),
            include_hidden: reader
                .optional_boolean(4, "watch includeHidden must be a boolean")?
                .unwrap_or(false),
        })
    }
}

#[host_operation("watchFiles")]
async fn watch_files(
    context: HostContext,
    arguments: WatchFilesArguments,
) -> Result<knot_protocol::WatchSubscriptionId, OpenError> {
    let providers = context.foreground(|_, cx| KnotWindow::filesystems(cx))?;
    let uri = knot_model::files::normalize(&providers, &arguments.uri)
        .await
        .map_err(|error| product::operations::resource_error(error.into()))?;
    let key = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    let (subscription, live) = context
        .foreground(|host, cx| {
            host.watch_files(
                uri,
                arguments.recursive,
                arguments.include_hidden,
                key,
                &providers,
                cx,
            )
        })?
        .map_err(|error| product::operations::resource_error(error.into()))?;
    // Resolving once live lets the caller read the disk without missing a
    // change.
    live.await;
    Ok(subscription)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WatchSubscription {
    subscription: knot_protocol::WatchSubscriptionId,
}

impl HostArguments for WatchSubscription {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            subscription: knot_protocol::WatchSubscriptionId::new(reader.integer(2)?),
        })
    }
}

#[host_operation("unwatchFiles")]
async fn unwatch_files(
    context: HostContext,
    arguments: WatchSubscription,
) -> Result<(), AccessError> {
    let key = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context.foreground(|host, _| {
        // A subscription the host already released is as good as disposed.
        host.unwatch_files(arguments.subscription, key);
        Ok(())
    })?
}

#[host_operation("statFile")]
async fn stat_file(
    context: HostContext,
    arguments: FileUriArguments,
) -> Result<Option<knot_protocol::FileKind>, OpenError> {
    let providers = context.foreground(|_, cx| KnotWindow::filesystems(cx))?;
    knot_model::files::stat(providers, &arguments.uri)
        .await
        .map(|kind| kind.map(file_kind))
        .map_err(|error| product::operations::resource_error(error.into()))
}

#[host_operation("createFile")]
async fn create_file(
    context: HostContext,
    arguments: FileUriArguments,
) -> Result<String, OpenError> {
    let providers = context.foreground(|_, cx| KnotWindow::filesystems(cx))?;
    let uri = knot_model::files::create_file(providers, &arguments.uri)
        .await
        .map_err(|error| product::operations::resource_error(error.into()))?;
    refresh_file_index(&context, std::slice::from_ref(&uri)).await;
    Ok(uri.to_string())
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WriteFileArguments {
    uri: String,
    text: String,
}

impl HostArguments for WriteFileArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            uri: reader.text(2, "resource URI must be text")?,
            text: reader.text(3, "file text must be text")?,
        })
    }
}

#[host_operation("writeFileText")]
async fn write_file_text(
    context: HostContext,
    arguments: WriteFileArguments,
) -> Result<String, OpenError> {
    let providers = context.foreground(|_, cx| KnotWindow::filesystems(cx))?;
    let uri = knot_model::files::write_text(providers, &arguments.uri, &arguments.text)
        .await
        .map_err(|error| product::operations::resource_error(error.into()))?;
    refresh_file_index(&context, std::slice::from_ref(&uri)).await;
    Ok(uri.to_string())
}

/// Bring the workspace file indexes up to date with a change the application
/// made, so a listing right afterwards sees it without waiting for the watch.
async fn refresh_file_index(context: &HostContext, changed: &[crate::app::resource::ResourceUri]) {
    let indexes = context
        .foreground(|_, cx| {
            changed
                .iter()
                .filter_map(|uri| crate::app::file_index::ApplicationFileIndex::covering(cx, uri))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for index in indexes {
        index.apply_changes(changed).await;
    }
}

#[host_operation("createDirectory")]
async fn create_directory(
    context: HostContext,
    arguments: FileUriArguments,
) -> Result<String, OpenError> {
    let providers = context.foreground(|_, cx| KnotWindow::filesystems(cx))?;
    let uri = knot_model::files::create_directory(providers, &arguments.uri)
        .await
        .map_err(|error| product::operations::resource_error(error.into()))?;
    refresh_file_index(&context, std::slice::from_ref(&uri)).await;
    Ok(uri.to_string())
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RenameFileArguments {
    from: String,
    to: String,
}

impl HostArguments for RenameFileArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            from: reader.text(2, "resource URI must be text")?,
            to: reader.text(3, "resource URI must be text")?,
        })
    }
}

#[host_operation("renameFile")]
async fn rename_file(
    context: HostContext,
    arguments: RenameFileArguments,
) -> Result<String, OpenError> {
    let providers = context.foreground(|_, cx| KnotWindow::filesystems(cx))?;
    let (from, to) = knot_model::files::rename(providers, &arguments.from, &arguments.to)
        .await
        .map_err(|error| product::operations::resource_error(error.into()))?;
    context.foreground(|_, cx| product::operations::follow_rename(&from, &to, cx))?;
    refresh_file_index(&context, &[from.clone(), to.clone()]).await;
    Ok(to.to_string())
}

#[host_operation("copyFile")]
async fn copy_file(
    context: HostContext,
    arguments: RenameFileArguments,
) -> Result<String, OpenError> {
    let providers = context.foreground(|_, cx| KnotWindow::filesystems(cx))?;
    let to = knot_model::files::copy(providers, &arguments.from, &arguments.to)
        .await
        .map_err(|error| product::operations::resource_error(error.into()))?;
    refresh_file_index(&context, std::slice::from_ref(&to)).await;
    Ok(to.to_string())
}

#[host_operation("revealFile")]
async fn reveal_file(context: HostContext, arguments: FileUriArguments) -> Result<(), OpenError> {
    let providers = context.foreground(|_, cx| KnotWindow::filesystems(cx))?;
    let uri = providers
        .normalize(
            crate::app::resource::ResourceUri::parse(&arguments.uri)
                .map_err(|_| OpenError::InvalidUri)?,
        )
        .await
        .map_err(|error| product::operations::resource_error(error.into()))?;
    let path = uri
        .as_url()
        .to_file_path()
        .map_err(|_| OpenError::ProviderFailure)?;
    context.foreground(|_, cx| cx.reveal_path(&path))?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DeleteFileArguments {
    uri: String,
    permanent: bool,
}

impl HostArguments for DeleteFileArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            uri: reader.text(2, "resource URI must be text")?,
            permanent: reader
                .optional_boolean(3, "delete permanent must be a boolean")?
                .unwrap_or(false),
        })
    }
}

#[host_operation("deleteFile")]
async fn delete_file(
    context: HostContext,
    arguments: DeleteFileArguments,
) -> Result<(), OpenError> {
    let providers = context.foreground(|_, cx| KnotWindow::filesystems(cx))?;
    let uri = knot_model::files::delete(providers, &arguments.uri, arguments.permanent)
        .await
        .map_err(|error| product::operations::resource_error(error.into()))?;
    context.foreground(|_, cx| product::operations::follow_delete(&uri, cx))?;
    refresh_file_index(&context, &[uri]).await;
    Ok(())
}

#[host_operation("registerView")]
pub(crate) async fn register_tree_provider(
    context: HostContext,
    arguments: TreeArguments,
) -> Result<TreeProviderRegistrationId, SemanticError> {
    let request = context.request;
    context.foreground(|host, cx| host.register_view(&request, arguments, cx))?
}

#[host_operation("replaceViewText")]
pub(crate) async fn replace_view_text(
    context: HostContext,
    arguments: ReplaceViewTextArguments,
) -> Result<(), SemanticError> {
    let request = context.request;
    context.foreground(|host, cx| host.semantics.replace_view_text(&request, arguments, cx))?
}

#[host_operation("invalidateView")]
pub(crate) async fn invalidate_tree_provider(
    context: HostContext,
    arguments: TreeInvalidationArguments,
) -> Result<(), SemanticError> {
    let request = context.request;
    context.foreground(|host, cx| {
        host.semantics
            .invalidate_tree_provider(&request, arguments, cx)
    })?
}

#[host_operation("revealViewItem")]
pub(crate) async fn reveal_tree_item(
    context: HostContext,
    arguments: TreeRevealArguments,
) -> Result<(), SemanticError> {
    let request = context.request;
    context.foreground(|host, cx| host.semantics.reveal_tree_item(&request, arguments, cx))?
}

#[host_operation("unregisterView")]
pub(crate) async fn unregister_tree_provider(
    context: HostContext,
    arguments: TreeRegistrationArguments,
) -> Result<(), SemanticError> {
    let request = context.request;
    context.foreground(|host, cx| host.unregister_view(&request, arguments, cx))?
}

#[host_operation("setViewBadge")]
pub(crate) async fn set_view_badge(
    context: HostContext,
    arguments: ViewBadgeArguments,
) -> Result<(), SemanticError> {
    let request = context.request;
    context.foreground(|host, cx| host.set_view_badge(&request, arguments, cx))?
}

#[host_operation("showView")]
pub(crate) async fn show_view(
    context: HostContext,
    arguments: ViewOperationArguments,
) -> Result<(), SemanticError> {
    let request = context.request;
    context.foreground(|host, cx| {
        host.present_view(
            &request,
            arguments.registration,
            arguments.window.as_deref(),
            ViewPresentation::Show,
            cx,
        )
    })?
}

#[host_operation("hideView")]
pub(crate) async fn hide_view(
    context: HostContext,
    arguments: ViewOperationArguments,
) -> Result<(), SemanticError> {
    let request = context.request;
    context.foreground(|host, cx| {
        host.present_view(
            &request,
            arguments.registration,
            arguments.window.as_deref(),
            ViewPresentation::Hide,
            cx,
        )
    })?
}

#[host_operation("focusView")]
pub(crate) async fn focus_view(
    context: HostContext,
    arguments: ViewOperationArguments,
) -> Result<(), SemanticError> {
    let request = context.request;
    context.foreground(|host, cx| {
        host.present_view(
            &request,
            arguments.registration,
            arguments.window.as_deref(),
            ViewPresentation::Focus,
            cx,
        )
    })?
}

#[host_operation("toggleView")]
pub(crate) async fn toggle_view(
    context: HostContext,
    arguments: ViewOperationArguments,
) -> Result<(), SemanticError> {
    let request = context.request;
    context.foreground(|host, cx| {
        host.present_view(
            &request,
            arguments.registration,
            arguments.window.as_deref(),
            ViewPresentation::Toggle,
            cx,
        )
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProviderRegistrationArguments {
    pub registration: knot_protocol::ProviderRegistrationId,
}
impl HostArguments for ProviderRegistrationArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            registration: knot_protocol::ProviderRegistrationId::new(reader.integer(2)?),
        })
    }
    fn provider_mutation(&self) -> Option<ProviderIdentity> {
        Some(ProviderIdentity::Named(self.registration))
    }
}

#[host_operation("unregisterProvider")]
pub(crate) async fn unregister_provider(
    context: HostContext,
    arguments: ProviderRegistrationArguments,
) -> Result<(), SemanticError> {
    let request = context.request;
    context.foreground(|host, cx| {
        host.semantics
            .unregister_provider(&request, arguments.registration)?;
        host.cancel_completion_provider(arguments.registration, cx);
        Ok(())
    })?
}

/// A contract as a package's `serve` or `client` describes it: the package
/// whose contract module defines it, its name, and that side's schema.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContractArguments {
    pub owner: String,
    pub name: String,
    pub schema: knot_model::contracts::ContractSchema,
}
impl HostArguments for ContractArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            owner: reader.text(2, "contract owner must be text")?,
            name: reader.text(3, "contract name must be text")?,
            schema: knot_model::contracts::ContractSchema::parse(
                &reader.text(4, "contract descriptor must be JSON text")?,
            )
            .map_err(|_| DecodeError::Type("invalid contract descriptor"))?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CallContractArguments {
    pub client: knot_protocol::ContractClientId,
    pub method: String,
    pub argument: String,
    pub window: Option<String>,
}
impl HostArguments for CallContractArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            client: knot_protocol::ContractClientId::new(reader.integer(2)?),
            method: reader.text(3, "contract method must be text")?,
            argument: reader.text(4, "contract argument must be JSON text")?,
            window: reader.optional_text(5, "window target must be text")?,
        })
    }
}

#[derive(Debug, HostError)]
pub(crate) enum ContractClientError {
    #[error(transparent)]
    Semantic(SemanticError),
    #[error(
        code = "ContractNotFound",
        message = "no package serves {owner} {name}"
    )]
    NotFound { owner: String, name: String },
}
impl From<AccessError> for ContractClientError {
    fn from(error: AccessError) -> Self {
        Self::Semantic(error.into())
    }
}
impl From<SemanticError> for ContractClientError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

/// Failures of a call through a contract client.
#[derive(Debug, HostError)]
pub(crate) enum ContractCallError {
    #[error(transparent)]
    Access(AccessError),
    #[error(
        code = "ContractNotFound",
        message = "the contract registration is no longer served"
    )]
    NotFound,
    #[error(
        code = "UnsupportedOperation",
        message = "a package calls the contracts it serves directly, not through a client"
    )]
    OwnContract,
    #[error(code = "ContractViolation", message = "{message}")]
    Violation { message: String },
    #[error(code = "ProviderFailure", message = "{message}")]
    Failure { message: String },
}
impl From<AccessError> for ContractCallError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

#[host_operation("serveContract")]
pub(crate) async fn serve_contract(
    context: HostContext,
    arguments: ContractArguments,
) -> Result<knot_protocol::ProviderRegistrationId, SemanticError> {
    let request = context.request;
    context.foreground(|host, _| {
        host.semantics.serve_contract(
            &request,
            knot_model::providers::ServedContract {
                owner: arguments.owner,
                name: arguments.name,
                schema: std::sync::Arc::new(arguments.schema),
            },
        )
    })?
}

/// Binds the caller to the latest registration serving the contract, which
/// its calls then address.
#[host_operation("contractClient")]
pub(crate) async fn contract_client(
    context: HostContext,
    arguments: ContractArguments,
) -> Result<knot_protocol::ContractClientId, ContractClientError> {
    let request = context.request;
    context.foreground(|host, _| {
        host.semantics
            .bind_contract_client(
                &request,
                &arguments.owner,
                &arguments.name,
                arguments.schema,
            )?
            .ok_or(ContractClientError::NotFound {
                owner: arguments.owner,
                name: arguments.name,
            })
    })?
}

/// Runs a method of the registration a client is bound to and answers with
/// its JSON result as text, checking the argument and the result against the
/// consumer's and the provider's schemas.
#[host_operation("callContract")]
pub(crate) async fn call_contract(
    context: HostContext,
    arguments: CallContractArguments,
) -> Result<String, ContractCallError> {
    let caller = context.request.extension;
    let method = arguments.method.clone();
    let (execution, binding, label) = context.foreground(move |host, _| {
        let (client, provider) = host
            .semantics
            .providers()
            .client(arguments.client, caller)
            .ok_or(ContractCallError::NotFound)?;
        if provider.extension == caller {
            return Err(ContractCallError::OwnContract);
        }
        let label = provider.contract.label();
        let argument = serde_json::from_str(&arguments.argument).map_err(|error| {
            ContractCallError::Failure {
                message: error.to_string(),
            }
        })?;
        client
            .binding
            .validate_argument(&arguments.method, &argument)
            .map_err(|violation| ContractCallError::Violation {
                message: format!("{label}: {violation}"),
            })?;
        let key = ExtensionKey::new(provider.extension, provider.lifecycle);
        let execution = host
            .pool
            .call_provider(
                key,
                knot_protocol::ProviderCallRequest {
                    registration: provider.id,
                    method: arguments.method,
                    argument: arguments.argument,
                    window: arguments.window,
                },
            )
            .map_err(|_| ContractCallError::NotFound)?;
        Ok((execution, client.binding.clone(), label))
    })??;
    let result = match execution.await {
        Ok(Ok(result)) => result,
        Ok(Err(message)) => return Err(ContractCallError::Failure { message }),
        Err(error) => {
            return Err(ContractCallError::Failure {
                message: error.to_string(),
            });
        }
    };
    let value = serde_json::from_str(&result).map_err(|error| ContractCallError::Failure {
        message: error.to_string(),
    })?;
    binding
        .validate_result(&method, &value)
        .map_err(|violation| ContractCallError::Violation {
            message: format!("{label}: {violation}"),
        })?;
    Ok(result)
}

use crate::app::commands::catalog::CommandMetadata;
use crate::app::commands::extension::CommandError;
use knot_protocol::{
    Command, CommandInvokeDispatch, CommandName, CommandOutcome, CommandRegistrationId,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RegisterCommandArguments {
    pub name: CommandName,
    pub metadata: CommandMetadata,
}
impl HostArguments for RegisterCommandArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            name: reader.text(2, "UnsupportedOperation")?.into(),
            metadata: CommandMetadata {
                title: reader.text(3, "UnsupportedOperation")?,
                description: reader.optional_text(4, "UnsupportedOperation")?,
                palette: reader.integer(5)? != 0,
            },
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RegisterViewArguments {
    pub view_kind: String,
    pub name: CommandName,
}
impl HostArguments for RegisterViewArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            view_kind: reader.text(2, "UnsupportedOperation")?,
            name: reader.text(3, "UnsupportedOperation")?.into(),
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommandRegistrationArguments {
    pub registration: CommandRegistrationId,
}
impl HostArguments for CommandRegistrationArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            registration: CommandRegistrationId::new(reader.integer(2)?),
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineArguments {
    pub invocation: CommandInvocationId,
    pub outcome: CommandOutcome,
}
impl HostArguments for InlineArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            invocation: CommandInvocationId::new(reader.integer(2)?),
            outcome: reader.outcome(3)?,
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InvokeArguments {
    pub command: Command,
}
impl HostArguments for InvokeArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            command: Command {
                name: reader.text(2, "UnsupportedOperation")?.into(),
                arguments: reader.command_arguments(3)?,
            },
        })
    }
}

#[host_operation("registerCommand")]
pub(crate) async fn register_command(
    context: HostContext,
    arguments: RegisterCommandArguments,
) -> Result<CommandRegistrationId, CommandError> {
    let request = context.request;
    context.foreground(|host, _| {
        host.commands
            .register_command(&request, arguments.name, arguments.metadata)
    })?
}
#[host_operation("registerViewCommand")]
pub(crate) async fn register_view_command(
    context: HostContext,
    arguments: RegisterViewArguments,
) -> Result<CommandRegistrationId, CommandError> {
    let request = context.request;
    context.foreground(|host, _| {
        host.commands
            .register_view_command(&request, arguments.view_kind, arguments.name)
    })?
}
#[host_operation("unregisterCommand")]
pub(crate) async fn unregister_command(
    context: HostContext,
    arguments: CommandRegistrationArguments,
) -> Result<(), CommandError> {
    let request = context.request;
    context.foreground(|host, _| {
        host.commands
            .unregister_command(&request, arguments.registration)
    })?
}
#[host_operation("completeInlineCommand")]
pub(crate) async fn complete_inline_command(
    context: HostContext,
    arguments: InlineArguments,
) -> Result<(), AccessError> {
    // A cancelled child still reports its outcome, which its parent waits
    // for, so this skips the liveness check that rejects cancelled callers.
    let request = context.request;
    context
        .host
        .update(&mut context.application.clone(), |host, cx| {
            let result =
                host.commands
                    .complete_inline(&request, arguments.invocation, arguments.outcome);
            host.drain_command_events(cx);
            result
        })
        .map_err(|_| AccessError::Cancelled)?
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ListCommandsArguments {
    view: Option<knot_protocol::PresentationTarget>,
}
impl HostArguments for ListCommandsArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        match reader.destination(2)? {
            None => Ok(Self { view: None }),
            Some(knot_protocol::DocumentDestination::View(view)) => Ok(Self { view: Some(view) }),
            Some(_) => Err(DecodeError::Type("commands.list requires a view target")),
        }
    }
}

#[host_operation("listCommands")]
async fn list_commands(
    context: HostContext,
    arguments: ListCommandsArguments,
) -> Result<Vec<knot_protocol::CommandInfo>, AccessError> {
    context.foreground(|_, cx| {
        let kind = match &arguments.view {
            Some(view) => Some(product::operations::view_kind(view, cx)?),
            None => None,
        };
        let bindings = cx
            .try_global::<crate::app::commands::keymaps::ApplicationKeymaps>()
            .map(|keymaps| keymaps.0.shown_bindings(kind.as_deref()))
            .unwrap_or_default();
        let dispatcher = cx.global::<crate::app::commands::dispatch::ApplicationProductCommands>();
        let mut commands = dispatcher
            .0
            .read(cx)
            .definitions()
            .filter(|definition| definition.palette)
            .map(|definition| knot_protocol::CommandInfo {
                key: bindings
                    .get(&definition.name)
                    .map(|key| crate::app::commands::keymaps::display_key(key)),
                name: definition.name.to_string(),
                title: definition.title,
                description: definition.description,
            })
            .collect::<Vec<_>>();
        commands.sort_by(|left, right| left.title.cmp(&right.title));
        Ok(commands)
    })?
}
#[host_operation("invokeCommand")]
pub(crate) async fn invoke_command(
    context: HostContext,
    arguments: InvokeArguments,
) -> Result<CommandInvokeDispatch, AccessError> {
    let request = context.request;
    context.foreground(|_, _| ())?;
    if request.invocation.is_none() {
        return Err(AccessError::ContextUnavailable);
    }
    let (sender, receiver) = tokio::sync::oneshot::channel();
    context.foreground(|_, cx| {
        cx.defer(move |cx| route_nested_command(request, arguments.command, sender, cx))
    })?;
    receiver.await.map_err(|_| AccessError::Cancelled)?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContextArguments {
    pub target: Option<CommandInvocationId>,
}
impl HostArguments for ContextArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            target: reader.optional_integer(2)?.map(CommandInvocationId::new),
        })
    }
}
#[derive(Debug, HostError)]
pub(crate) enum ContextError {
    #[error(transparent)]
    Access(AccessError),
    #[error(code = "UnsupportedOperation", message = "UnsupportedOperation")]
    UnsupportedOperation,
}
impl From<AccessError> for ContextError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

#[host_operation("activeBuffer")]
pub(crate) async fn active_buffer(
    context: HostContext,
    arguments: ContextArguments,
) -> Result<Option<BufferHandle>, AccessError> {
    let request = context.request;
    context.foreground(|host, cx| host.contextual_buffer(&request, arguments.target, cx))?
}
#[host_operation("selectedViewText")]
pub(crate) async fn selected_view_text(
    context: HostContext,
    arguments: ContextArguments,
) -> Result<Option<String>, ContextError> {
    let request = context.request;
    context.foreground(|host, cx| host.contextual_selected_text(&request, arguments.target, cx))?
}

#[host_operation("selectedTreeItem")]
pub(crate) async fn selected_tree_item(
    context: HostContext,
    arguments: ContextArguments,
) -> Result<Option<String>, ContextError> {
    let request = context.request;
    context
        .foreground(|host, cx| host.contextual_selected_tree_item(&request, arguments.target, cx))?
}

/// What an editor operation acts through: a command's captured target, or a
/// long-lived handle bound to one tab presentation. Handles travel as
/// `EDITOR_HANDLE_AUTHORITY + id`, above every invocation ID and still exact
/// in a JavaScript number.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditorAuthority {
    Command(CommandInvocationId),
    Handle(u64),
}

const EDITOR_HANDLE_AUTHORITY: u64 = 1 << 52;

impl EditorAuthority {
    pub(crate) fn handle_wire(id: u64) -> u64 {
        EDITOR_HANDLE_AUTHORITY + id
    }

    fn decode(reader: &mut dyn ArgumentReader, index: usize) -> Result<Self, DecodeError> {
        let value = reader.integer(index)?;
        Ok(match value.checked_sub(EDITOR_HANDLE_AUTHORITY) {
            Some(id) => Self::Handle(id),
            None => Self::Command(CommandInvocationId::new(value)),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EditorArguments {
    pub authority: EditorAuthority,
}
impl HostArguments for EditorArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            authority: EditorAuthority::decode(reader, 2)?,
        })
    }
}

/// The editor view an editor handle binds to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EditorViewArguments {
    pub target: knot_protocol::ViewNodeTarget,
}

impl HostArguments for EditorViewArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            target: knot_protocol::ViewNodeTarget {
                window: knot_protocol::WindowTarget {
                    window: reader.text(2, "window must be text")?,
                },
                view: reader.text(3, "view must be text")?,
            },
        })
    }
}

/// Bind a long-lived editor handle to the editor view a view node target
/// names, returning its authority.
#[host_operation("openEditor")]
pub(crate) async fn open_editor(
    context: HostContext,
    arguments: EditorViewArguments,
) -> Result<u64, AccessError> {
    let owner = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context.foreground(|host, cx| host.open_editor(owner, &arguments.target, cx))?
}

/// Start selection notifications for a handle.
#[host_operation("watchEditorSelection")]
pub(crate) async fn watch_editor_selection(
    context: HostContext,
    arguments: EditorArguments,
) -> Result<(), AccessError> {
    let request = context.request;
    context.foreground(|host, cx| host.watch_editor_selection(&request, arguments.authority, cx))?
}

/// The selection a notification carries; `None` once the handle is gone.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub(crate) struct AcknowledgedSelection(pub Option<EditorSelection>);
impl HostSuccess for AcknowledgedSelection {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(self)
    }
}

/// A selection notification's listener turn began: returns the selection;
/// see `SelectionWatch`.
#[host_operation("acknowledgeEditorSelection")]
pub(crate) async fn acknowledge_editor_selection(
    context: HostContext,
    arguments: SelectionArguments,
) -> Result<AcknowledgedSelection, AccessError> {
    let owner = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context.foreground(|host, cx| {
        AcknowledgedSelection(host.acknowledge_editor_selection(
            owner,
            arguments.authority,
            arguments.lines,
            cx,
        ))
    })
}

/// Release a handle. A handle the tab already invalidated is not an error.
#[host_operation("releaseEditor")]
pub(crate) async fn release_editor(
    context: HostContext,
    arguments: EditorArguments,
) -> Result<(), AccessError> {
    let owner = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context.foreground(|host, cx| {
        if let EditorAuthority::Handle(id) = arguments.authority {
            host.release_editor(owner, id, cx);
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UndoGroupArguments {
    pub group: u64,
}
impl HostArguments for UndoGroupArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            group: reader.integer(2)?,
        })
    }
}

/// Open an undo group on the editor's buffer for the caller's lifecycle,
/// returning its identity. Every commit to the buffer joins one undo step
/// until the group ends.
#[host_operation("editorBeginUndoGroup")]
pub(crate) async fn editor_begin_undo_group(
    context: HostContext,
    arguments: EditorArguments,
) -> Result<u64, EditorError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let owner = ExtensionKey::new(request.extension, request.lifecycle);
        host.begin_undo_group(owner, editor, arguments.authority, cx)
            .ok_or(EditorError::OperationInProgress)
    })?
}

/// End an undo group. A group that already ended is not an error.
#[host_operation("editorEndUndoGroup")]
pub(crate) async fn editor_end_undo_group(
    context: HostContext,
    arguments: UndoGroupArguments,
) -> Result<(), AccessError> {
    let owner = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context.foreground(|host, cx| host.end_undo_group(owner, arguments.group, cx))
}

/// Start recording the input the editor receives for the caller's lifecycle,
/// returning the recording's identity.
#[host_operation("editorBeginRecording")]
pub(crate) async fn editor_begin_recording(
    context: HostContext,
    arguments: EditorArguments,
) -> Result<u64, AccessError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let owner = ExtensionKey::new(request.extension, request.lifecycle);
        Ok(host.begin_recording(owner, editor, arguments.authority, cx))
    })?
}

/// The commands a finished recording collected, as JSON.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RecordedCommands(pub Vec<knot_model::editor::RecordedCommand>);
impl HostSuccess for RecordedCommands {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(
            self.0
                .into_iter()
                .map(|entry| {
                    serde_json::json!({ "command": entry.command, "arguments": entry.arguments })
                })
                .collect::<Vec<_>>(),
        )
    }
}

/// End a recording and return its entries. A recording that already ended
/// returns none.
#[host_operation("editorEndRecording")]
pub(crate) async fn editor_end_recording(
    context: HostContext,
    arguments: UndoGroupArguments,
) -> Result<RecordedCommands, AccessError> {
    let owner = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context
        .foreground(|host, cx| RecordedCommands(host.finish_recording(owner, arguments.group, cx)))
}

#[derive(Debug, HostError)]
pub(crate) enum EditorError {
    #[error(transparent)]
    Access(AccessError),
    #[error(code = "InvalidRange", message = "InvalidRange")]
    InvalidRange,
    #[error(code = "OperationInProgress", message = "OperationInProgress")]
    OperationInProgress,
    #[error(code = "RevisionConflict", message = "RevisionConflict")]
    RevisionConflict,
    #[error(code = "BufferClosed", message = "BufferClosed")]
    BufferClosed,
}
impl From<AccessError> for EditorError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

/// A selection's ends as UTF-8 byte offsets, equal for a bare caret, with the
/// caret's line and the lines around it when the read asked for them.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EditorSelection {
    pub anchor: usize,
    pub caret: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<LineItem>>,
}
impl HostSuccess for EditorSelection {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(self)
    }
}

/// One line of text and where it starts, in UTF-8 bytes.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LineItem {
    pub line: usize,
    pub start_byte_offset: usize,
    pub text: String,
}
impl From<knot_model::buffer::LineText> for LineItem {
    fn from(line: knot_model::buffer::LineText) -> Self {
        Self {
            line: line.line,
            start_byte_offset: line.start,
            text: line.text,
        }
    }
}

/// The editor's selection, with `lines` lines around the caret when asked.
pub(crate) fn read_selection(
    editor: &Entity<crate::app::editor::Editor>,
    lines: Option<u64>,
    cx: &App,
) -> EditorSelection {
    let editor = editor.read(cx);
    let (anchor, caret) = editor.selection(cx);
    let around = lines.map(|radius| {
        let radius = usize::try_from(radius).unwrap_or(usize::MAX);
        editor.model().read(cx).lines_around(caret, radius)
    });
    let (line, lines) = match around {
        Some((line, lines)) => (
            Some(line),
            Some(lines.into_iter().map(LineItem::from).collect()),
        ),
        None => (None, None),
    };
    EditorSelection {
        anchor,
        caret,
        line,
        lines,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SelectionArguments {
    pub authority: EditorAuthority,
    pub lines: Option<u64>,
}
impl HostArguments for SelectionArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            authority: EditorAuthority::decode(reader, 2)?,
            lines: reader.optional_integer(3)?,
        })
    }
}

#[host_operation("editorSelection")]
pub(crate) async fn editor_selection(
    context: HostContext,
    arguments: SelectionArguments,
) -> Result<EditorSelection, AccessError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        Ok(read_selection(&editor, arguments.lines, cx))
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreparePasteArguments {
    pub authority: EditorAuthority,
    pub text: String,
    pub range: Option<(u64, u64)>,
}
impl HostArguments for PreparePasteArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let authority = EditorAuthority::decode(reader, 2)?;
        let text = reader.text(3, "text must be text")?;
        let range = match (reader.optional_integer(4)?, reader.optional_integer(5)?) {
            (Some(start), Some(end)) => Some((start, end)),
            (None, None) => None,
            _ => return Err(DecodeError::Range("range needs a start and an end")),
        };
        Ok(Self {
            authority,
            text,
            range,
        })
    }
}

/// The text pasting `text` over a range of the captured editor inserts: the
/// language's paste provider's answer, or `text` when there is none. The
/// editor is not changed.
#[host_operation("editorPreparePaste")]
pub(crate) async fn editor_prepare_paste(
    context: HostContext,
    arguments: PreparePasteArguments,
) -> Result<String, EditorError> {
    let request = context.request;
    let text = arguments.text.clone();
    let call = context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let editor = editor.read(cx);
        let buffer = editor.model();
        let range = match arguments.range {
            Some((start, end)) => Some(
                buffer
                    .read(cx)
                    .checked_range(ByteRange {
                        start_byte_offset: start as usize,
                        end_byte_offset: end as usize,
                    })
                    .map_err(|_| EditorError::InvalidRange)?,
            ),
            None => None,
        };
        let Some(paste) = editor.paste_request(&arguments.text, range, cx) else {
            return Ok(None);
        };
        let documents = cx.global::<crate::app::documents::ApplicationDocuments>();
        let Some(document) = documents
            .0
            .read(cx)
            .documents()
            .find(|document| document.model() == buffer)
            .map(|document| document.identity().to_string())
        else {
            return Ok(None);
        };
        Ok::<_, EditorError>(host.paste_provider_call(&document, None, &paste))
    })??;
    let Some(execution) = call else {
        return Ok(text);
    };
    let answer = knot_model::editor::PasteRequest::answer(execution.settle().await);
    Ok(answer.unwrap_or(text))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SetSelectionArguments {
    pub authority: EditorAuthority,
    pub anchor: u64,
    pub caret: u64,
    pub reveal: crate::app::editor::Reveal,
}
impl HostArguments for SetSelectionArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            authority: EditorAuthority::decode(reader, 2)?,
            anchor: reader.integer(3)?,
            caret: reader.integer(4)?,
            reveal: match reader.text(5, "reveal must be text")?.as_str() {
                "nearest" => crate::app::editor::Reveal::Nearest,
                "center" => crate::app::editor::Reveal::Center,
                "always-center" => crate::app::editor::Reveal::AlwaysCenter,
                "top" => crate::app::editor::Reveal::Top,
                "bottom" => crate::app::editor::Reveal::Bottom,
                _ => {
                    return Err(DecodeError::Range(
                        "reveal must be nearest, center, always-center, top, or bottom",
                    ));
                }
            },
        })
    }
}

/// Move the captured editor's selection. Offsets must lie on character
/// boundaries within the text.
#[host_operation("editorSetSelection")]
pub(crate) async fn editor_set_selection(
    context: HostContext,
    arguments: SetSelectionArguments,
) -> Result<(), EditorError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let moved = editor.update(cx, |editor, cx| {
            editor.set_selection_checked(arguments.anchor, arguments.caret, arguments.reveal, cx)
        });
        moved.then_some(()).ok_or(EditorError::InvalidRange)
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SetKeyContextArguments {
    pub authority: EditorAuthority,
    pub name: String,
    pub active: bool,
}
impl HostArguments for SetKeyContextArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let authority = EditorAuthority::decode(reader, 2)?;
        let name = reader.text(3, "key context must be text")?;
        if name.is_empty() {
            return Err(DecodeError::Range("key context must not be empty"));
        }
        let active = reader
            .optional_boolean(4, "key context active must be a boolean")?
            .ok_or(DecodeError::Type("key context active must be a boolean"))?;
        Ok(Self {
            authority,
            name,
            active,
        })
    }
}

/// Set or clear the caller's key context on the captured editor. The
/// context lasts until cleared, the view closes, or the caller's lifecycle
/// ends.
#[host_operation("editorSetKeyContext")]
pub(crate) async fn editor_set_key_context(
    context: HostContext,
    arguments: SetKeyContextArguments,
) -> Result<(), AccessError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let owner = ExtensionKey::new(request.extension, request.lifecycle);
        host.set_editor_key_context(owner, editor, &arguments.name, arguments.active, cx);
        Ok(())
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SetDecorationsArguments {
    pub authority: EditorAuthority,
    pub items: Vec<knot_model::editor::ViewDecoration>,
    pub if_revision: u64,
}
impl HostArguments for SetDecorationsArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let authority = EditorAuthority::decode(reader, 2)?;
        let items = reader
            .contributions(3)?
            .into_iter()
            .map(|contribution| match contribution {
                EditorContribution {
                    range,
                    decoration,
                    gutter,
                    command: None,
                } if decoration.is_some() || gutter.is_some() => {
                    Ok(knot_model::editor::ViewDecoration {
                        range,
                        decoration,
                        gutter,
                    })
                }
                _ => Err(DecodeError::Type(
                    "view decorations need a decoration or a gutter mark, and no command",
                )),
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            authority,
            items,
            if_revision: reader.integer(4)?,
        })
    }
}

/// Replace the caller's decorations on the captured editor, which only that
/// view paints. They last until replaced, the view closes, or the caller's
/// lifecycle ends.
#[host_operation("editorSetDecorations")]
pub(crate) async fn editor_set_decorations(
    context: HostContext,
    arguments: SetDecorationsArguments,
) -> Result<(), EditorError> {
    use knot_model::buffer::ContributionError;
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let owner = ExtensionKey::new(request.extension, request.lifecycle);
        host.set_editor_decorations(owner, editor, &arguments.items, arguments.if_revision, cx)
            .map_err(|error| match error {
                ContributionError::RevisionConflict => EditorError::RevisionConflict,
                ContributionError::Closed | ContributionError::NotFound => {
                    AccessError::InvalidTarget.into()
                }
                ContributionError::InvalidRange => EditorError::InvalidRange,
            })
    })?
}

/// One block of rows, as the package sends it.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BlockRowsWire {
    line: usize,
    source: u64,
    start: usize,
    end: usize,
    color: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SetBlocksArguments {
    pub authority: EditorAuthority,
    blocks: Vec<BlockRowsWire>,
}
impl HostArguments for SetBlocksArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let authority = EditorAuthority::decode(reader, 2)?;
        let json = reader.text(3, "blocks must be text")?;
        let blocks: Vec<BlockRowsWire> =
            serde_json::from_str(&json).map_err(|_| DecodeError::Type("blocks are invalid"))?;
        if blocks.iter().any(|block| block.start > block.end) {
            return Err(DecodeError::Range(
                "a block's lines must not end before they start",
            ));
        }
        Ok(Self { authority, blocks })
    }
}

/// Replace the caller's block rows on the captured editor: lines of source
/// buffers shown above its lines. Only that view paints them; they last until
/// replaced, the view closes, or the caller's lifecycle ends.
#[host_operation("editorSetBlocks")]
pub(crate) async fn editor_set_blocks(
    context: HostContext,
    arguments: SetBlocksArguments,
) -> Result<(), EditorError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let owner = ExtensionKey::new(request.extension, request.lifecycle);
        let commands = &host.commands;
        let mut rows = Vec::with_capacity(arguments.blocks.len());
        for block in arguments.blocks {
            let source = host
                .buffers
                .model(
                    &request,
                    BufferHandle::new(block.source),
                    || commands.is_cancelled(request.invocation),
                    cx,
                )
                .map_err(|error| match error {
                    BufferError::BufferClosed => EditorError::BufferClosed,
                    _ => AccessError::Cancelled.into(),
                })?;
            rows.push(crate::app::editor::BlockRows {
                line: block.line,
                source,
                lines: block.start..block.end,
                key: block.color,
            });
        }
        host.set_editor_block_rows(owner, editor, rows, cx);
        Ok(())
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SetCursorStyleArguments {
    pub authority: EditorAuthority,
    pub style: Option<crate::app::editor::CaretShape>,
}
impl HostArguments for SetCursorStyleArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        use crate::app::editor::CaretShape;
        let authority = EditorAuthority::decode(reader, 2)?;
        let style = match reader
            .optional_text(3, "cursor style must be text or null")?
            .as_deref()
        {
            None => None,
            Some("bar") => Some(CaretShape::Bar),
            Some("block") => Some(CaretShape::Block),
            Some("underline") => Some(CaretShape::Underline),
            Some(_) => {
                return Err(DecodeError::Range(
                    "cursor style must be bar, block, or underline",
                ));
            }
        };
        Ok(Self { authority, style })
    }
}

/// Set or clear the caller's caret shape on the captured editor. The shape
/// lasts until cleared, the view closes, or the caller's lifecycle ends.
#[host_operation("editorSetCursorStyle")]
pub(crate) async fn editor_set_cursor_style(
    context: HostContext,
    arguments: SetCursorStyleArguments,
) -> Result<(), AccessError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let owner = ExtensionKey::new(request.extension, request.lifecycle);
        host.set_editor_caret_shape(owner, editor, arguments.style, cx);
        Ok(())
    })?
}

/// Failures of an editor bar operation.
#[derive(Debug, HostError)]
pub(crate) enum BarError {
    #[error(transparent)]
    Access(AccessError),
    #[error(code = "BarNotFound", message = "BarNotFound")]
    NotFound,
}
impl From<AccessError> for BarError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

/// What a bar shows, as `openBar` receives it.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BarSpecValue {
    #[serde(default)]
    fields: Vec<BarFieldValue>,
    #[serde(default)]
    toggles: Vec<BarToggleValue>,
    #[serde(default)]
    buttons: Vec<BarButtonValue>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BarCommandValue {
    name: String,
    arguments: knot_protocol::CommandArgumentValue,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BarToggleValue {
    id: String,
    icon: knot_protocol::IconRef,
    tooltip: Option<String>,
    active: bool,
    command: BarCommandValue,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BarButtonValue {
    id: String,
    icon: knot_protocol::IconRef,
    tooltip: Option<String>,
    command: BarCommandValue,
}

impl From<BarCommandValue> for knot_model::editor::bars::BarCommand {
    fn from(value: BarCommandValue) -> Self {
        Self {
            name: value.name,
            arguments: value.arguments,
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BarFieldValue {
    id: String,
    #[serde(default)]
    placeholder: String,
    #[serde(default)]
    history: Vec<String>,
}

fn bar_id(reader: &mut dyn ArgumentReader, index: usize) -> Result<String, DecodeError> {
    let id = reader.text(index, "bar id must be text")?;
    if id.is_empty() {
        return Err(DecodeError::Range("bar id must not be empty"));
    }
    Ok(id)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OpenBarArguments {
    pub authority: EditorAuthority,
    pub id: String,
    pub spec: knot_model::editor::bars::BarSpec,
}
impl HostArguments for OpenBarArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        use knot_model::editor::bars::{BarButton, BarField, BarSpec, BarToggle};
        let authority = EditorAuthority::decode(reader, 2)?;
        let id = bar_id(reader, 3)?;
        let value: BarSpecValue =
            serde_json::from_value(setting_value(reader.command_arguments(4)?))
                .map_err(|_| DecodeError::Type("invalid bar"))?;
        let spec = BarSpec {
            fields: value
                .fields
                .into_iter()
                .map(|field| BarField {
                    id: field.id,
                    placeholder: field.placeholder,
                    history: field.history,
                })
                .collect(),
            toggles: value
                .toggles
                .into_iter()
                .map(|toggle| BarToggle {
                    id: toggle.id,
                    icon: toggle.icon,
                    tooltip: toggle.tooltip,
                    active: toggle.active,
                    command: toggle.command.into(),
                })
                .collect(),
            buttons: value
                .buttons
                .into_iter()
                .map(|button| BarButton {
                    id: button.id,
                    icon: button.icon,
                    tooltip: button.tooltip,
                    command: button.command.into(),
                })
                .collect(),
            message: value.message,
        };
        if let Some(problem) = spec.problem() {
            return Err(DecodeError::Range(problem));
        }
        Ok(Self {
            authority,
            id,
            spec,
        })
    }
}

/// Open the caller's bar on the captured editor, or update it in place. The
/// bar lasts until closed, the view closes, or the caller's lifecycle ends.
#[host_operation("editorOpenBar")]
pub(crate) async fn editor_open_bar(
    context: HostContext,
    arguments: OpenBarArguments,
) -> Result<(), AccessError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let owner = ExtensionKey::new(request.extension, request.lifecycle);
        host.open_editor_bar(owner, editor, &arguments.id, arguments.spec, cx);
        Ok(())
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BarArguments {
    pub authority: EditorAuthority,
    pub id: String,
}
impl HostArguments for BarArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            authority: EditorAuthority::decode(reader, 2)?,
            id: bar_id(reader, 3)?,
        })
    }
}

/// Close the caller's bar on the captured editor; closing a bar that is not
/// open does nothing.
#[host_operation("editorCloseBar")]
pub(crate) async fn editor_close_bar(
    context: HostContext,
    arguments: BarArguments,
) -> Result<(), AccessError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let owner = ExtensionKey::new(request.extension, request.lifecycle);
        editor.update(cx, |editor, cx| editor.close_bar(owner, &arguments.id, cx));
        Ok(())
    })?
}

fn popover_id(reader: &mut dyn ArgumentReader, index: usize) -> Result<String, DecodeError> {
    let id = reader.text(index, "popover id must be text")?;
    if id.is_empty() {
        return Err(DecodeError::Range("popover id must not be empty"));
    }
    Ok(id)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OpenPopoverArguments {
    pub authority: EditorAuthority,
    pub id: String,
    pub anchor: std::ops::Range<u32>,
    pub options: super::composites::PopoverOptions,
    pub layout: super::composites::LayoutWire,
}
impl HostArguments for OpenPopoverArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let line = |value: u64| {
            u32::try_from(value).map_err(|_| DecodeError::Range("popover anchor line is too large"))
        };
        Ok(Self {
            authority: EditorAuthority::decode(reader, 2)?,
            id: popover_id(reader, 3)?,
            anchor: line(reader.integer(4)?)?..line(reader.integer(5)?)?,
            options: serde_json::from_str(&reader.text(6, "popover options must be text")?)
                .map_err(|_| DecodeError::Type("popover options are invalid"))?,
            layout: decode_composite_layout(&reader.text(7, "popover layout must be text")?, true)?,
        })
    }
}

/// Open the caller's popover on the captured editor, anchored to a range of
/// its lines, or update it in place. The popover lasts until closed, the
/// anchor is deleted, the view closes, or the caller's lifecycle ends.
#[host_operation("editorOpenPopover")]
pub(crate) async fn editor_open_popover(
    context: HostContext,
    arguments: OpenPopoverArguments,
) -> Result<(), OpenError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let (editor, window) =
            host.contextual_editor_in_window(&request, arguments.authority, cx)?;
        let owner = ExtensionKey::new(request.extension, request.lifecycle);
        let content = host.popover_content(
            owner,
            arguments.anchor,
            arguments.options,
            arguments.layout,
            cx,
        )?;
        host.open_editor_popover(owner, editor, &window, &arguments.id, content, cx);
        Ok(())
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PopoverArguments {
    pub authority: EditorAuthority,
    pub id: String,
}
impl HostArguments for PopoverArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            authority: EditorAuthority::decode(reader, 2)?,
            id: popover_id(reader, 3)?,
        })
    }
}

/// Close the caller's popover on the captured editor; closing a popover that
/// is not open does nothing.
#[host_operation("editorClosePopover")]
pub(crate) async fn editor_close_popover(
    context: HostContext,
    arguments: PopoverArguments,
) -> Result<(), AccessError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let owner = ExtensionKey::new(request.extension, request.lifecycle);
        editor.update(cx, |editor, cx| {
            editor.close_popover(owner, &arguments.id, cx)
        });
        Ok(())
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FocusBarFieldArguments {
    pub authority: EditorAuthority,
    pub id: String,
    pub field: String,
    pub text: Option<String>,
}
impl HostArguments for FocusBarFieldArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            authority: EditorAuthority::decode(reader, 2)?,
            id: bar_id(reader, 3)?,
            field: reader.text(4, "bar field id must be text")?,
            text: reader.optional_text(5, "bar field text must be text")?,
        })
    }
}

/// Give a field of the caller's bar the captured editor's focus, first
/// replacing its text when one is given.
#[host_operation("editorFocusBarField")]
pub(crate) async fn editor_focus_bar_field(
    context: HostContext,
    arguments: FocusBarFieldArguments,
) -> Result<(), BarError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let owner = ExtensionKey::new(request.extension, request.lifecycle);
        let field =
            knot_model::editor::bars::BarFieldRef::new(owner, arguments.id, arguments.field);
        let text = arguments.text.map(|text| text.replace(['\r', '\n'], ""));
        editor
            .update(cx, |editor, cx| editor.focus_bar_field(field, text, cx))
            .then_some(())
            .ok_or(BarError::NotFound)
    })?
}

/// Each field's text of one bar, by field ID.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub(crate) struct BarFieldTexts(pub std::collections::BTreeMap<String, String>);
impl HostSuccess for BarFieldTexts {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(self)
    }
}

/// The text of each field of the caller's bar on the captured editor.
#[host_operation("editorBarFields")]
pub(crate) async fn editor_bar_fields(
    context: HostContext,
    arguments: BarArguments,
) -> Result<BarFieldTexts, BarError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let owner = ExtensionKey::new(request.extension, request.lifecycle);
        editor
            .read(cx)
            .bar_field_texts(owner, &arguments.id, cx)
            .map(|texts| BarFieldTexts(texts.into_iter().collect()))
            .ok_or(BarError::NotFound)
    })?
}

/// Start field edit notifications for a handle's bars.
#[host_operation("watchEditorBars")]
pub(crate) async fn watch_editor_bars(
    context: HostContext,
    arguments: EditorArguments,
) -> Result<(), AccessError> {
    let request = context.request;
    context.foreground(|host, cx| host.watch_editor_bars(&request, arguments.authority, cx))?
}

/// The bars edited since the last acknowledgment, with their field texts.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub(crate) struct EditedBar {
    pub id: String,
    pub fields: std::collections::BTreeMap<String, String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub(crate) struct EditedBars(pub Vec<EditedBar>);
impl HostSuccess for EditedBars {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(self)
    }
}

/// A bar notification's listener turn began: returns what was edited since
/// the last one; see `BarWatch`.
#[host_operation("acknowledgeEditorBars")]
pub(crate) async fn acknowledge_editor_bars(
    context: HostContext,
    arguments: EditorArguments,
) -> Result<EditedBars, AccessError> {
    let owner = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context.foreground(|host, cx| {
        EditedBars(host.acknowledge_editor_bars(owner, arguments.authority, cx))
    })
}

/// Start close notifications for a handle's popovers.
#[host_operation("watchEditorPopovers")]
pub(crate) async fn watch_editor_popovers(
    context: HostContext,
    arguments: EditorArguments,
) -> Result<(), AccessError> {
    let request = context.request;
    context.foreground(|host, cx| host.watch_editor_popovers(&request, arguments.authority, cx))?
}

/// A popover Knot closed, and why.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub(crate) struct ClosedPopover {
    pub id: String,
    pub reason: &'static str,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub(crate) struct ClosedPopovers(pub Vec<ClosedPopover>);
impl HostSuccess for ClosedPopovers {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(self)
    }
}

/// A popover notification's listener turn began: returns the popovers closed
/// since the last one; see `PopoverWatch`.
#[host_operation("acknowledgeEditorPopovers")]
pub(crate) async fn acknowledge_editor_popovers(
    context: HostContext,
    arguments: EditorArguments,
) -> Result<ClosedPopovers, AccessError> {
    let owner = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context.foreground(|host, _| {
        ClosedPopovers(host.acknowledge_editor_popovers(owner, arguments.authority))
    })
}

/// The first row wholly inside an editor's viewport and the rows that are.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub(crate) struct Viewport {
    pub first: usize,
    pub rows: usize,
}
impl HostSuccess for Viewport {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(self)
    }
}

/// The rows wholly visible in the captured editor, from its last layout.
#[host_operation("editorViewport")]
pub(crate) async fn editor_viewport(
    context: HostContext,
    arguments: EditorArguments,
) -> Result<Viewport, AccessError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let (first, rows) = editor.read(cx).viewport();
        Ok(Viewport { first, rows })
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScrollByArguments {
    pub authority: EditorAuthority,
    pub rows: i64,
}
impl HostArguments for ScrollByArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let authority = EditorAuthority::decode(reader, 2)?;
        let knot_protocol::CommandArgumentValue::Number(rows) = reader.command_arguments(3)? else {
            return Err(DecodeError::Type("scroll rows must be a number"));
        };
        Ok(Self {
            authority,
            rows: rows as i64,
        })
    }
}

/// Scroll the captured editor by rows without moving its caret.
#[host_operation("editorScrollBy")]
pub(crate) async fn editor_scroll_by(
    context: HostContext,
    arguments: ScrollByArguments,
) -> Result<(), AccessError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        editor.update(cx, |editor, cx| editor.scroll_by_rows(arguments.rows, cx));
        Ok(())
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowAtArguments {
    pub authority: EditorAuthority,
    pub row: u64,
}
impl HostArguments for RowAtArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            authority: EditorAuthority::decode(reader, 2)?,
            row: reader.integer(3)?,
        })
    }
}

/// Where a document row starts: its buffer line and byte column.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub(crate) struct RowStart {
    pub line: usize,
    pub column: usize,
}
impl HostSuccess for RowStart {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(self)
    }
}

/// Where a document row of the captured editor starts, the last row's start
/// for a row past the end.
#[host_operation("editorRowAt")]
pub(crate) async fn editor_row_at(
    context: HostContext,
    arguments: RowAtArguments,
) -> Result<RowStart, AccessError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let row = usize::try_from(arguments.row).unwrap_or(usize::MAX);
        let (line, column) = editor.read(cx).row_start(row, cx);
        Ok(RowStart { line, column })
    })?
}

/// The document row a byte column of a buffer line shows on in the captured
/// editor.
#[host_operation("editorRowOf")]
pub(crate) async fn editor_row_of(
    context: HostContext,
    arguments: LineMeasureArguments,
) -> Result<u64, EditorError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let line = usize::try_from(arguments.line).map_err(|_| EditorError::InvalidRange)?;
        let column = usize::try_from(arguments.value).map_err(|_| EditorError::InvalidRange)?;
        let row = editor
            .read(cx)
            .row_of_position(line, column, cx)
            .ok_or(EditorError::InvalidRange)?;
        Ok(row as u64)
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FoldArguments {
    pub authority: EditorAuthority,
    pub start: u64,
    pub end: u64,
}
impl HostArguments for FoldArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            authority: EditorAuthority::decode(reader, 2)?,
            start: reader.integer(3)?,
            end: reader.integer(4)?,
        })
    }
}

/// A collapsed fold's first and last buffer lines.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub(crate) struct FoldRange {
    pub start: usize,
    pub end: usize,
}

/// Collapse a range of lines in the captured editor, keeping its first line
/// visible.
#[host_operation("editorFold")]
pub(crate) async fn editor_fold(
    context: HostContext,
    arguments: FoldArguments,
) -> Result<(), EditorError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let folded = editor.update(cx, |editor, cx| {
            editor.fold_through(arguments.start, arguments.end, cx)
        });
        folded.then_some(()).ok_or(EditorError::InvalidRange)
    })?
}

/// Expand the collapsed fold over exactly a range of lines of the captured
/// editor, if there is one.
#[host_operation("editorUnfold")]
pub(crate) async fn editor_unfold(
    context: HostContext,
    arguments: FoldArguments,
) -> Result<(), EditorError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        editor.update(cx, |editor, cx| {
            editor.unfold_through(arguments.start, arguments.end, cx)
        });
        Ok(())
    })?
}

/// The collapsed folds of the captured editor, ordered by first line and then
/// outermost first.
#[host_operation("editorFolds")]
pub(crate) async fn editor_folds(
    context: HostContext,
    arguments: EditorArguments,
) -> Result<Vec<FoldRange>, AccessError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        Ok(editor
            .read(cx)
            .folds()
            .into_iter()
            .map(|lines| FoldRange {
                start: lines.start,
                end: lines.end - 1,
            })
            .collect())
    })?
}

/// The regions of the captured editor's text that can fold, as the
/// language's `folds` query or the text's indentation offers them, ordered by
/// first line and then outermost first.
#[host_operation("editorFoldRegions")]
pub(crate) async fn editor_fold_regions(
    context: HostContext,
    arguments: EditorArguments,
) -> Result<Vec<FoldRange>, AccessError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        Ok(editor
            .read(cx)
            .fold_regions(cx)
            .iter()
            .map(|lines| FoldRange {
                start: lines.start,
                end: lines.end - 1,
            })
            .collect())
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LineMeasureArguments {
    pub authority: EditorAuthority,
    pub line: u64,
    /// A byte column for `editorCellsBefore` and `editorRowOf`, a display
    /// cell for `editorColumnAtCells`.
    pub value: u64,
}
impl HostArguments for LineMeasureArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            authority: EditorAuthority::decode(reader, 2)?,
            line: reader.integer(3)?,
            value: reader.integer(4)?,
        })
    }
}

/// The display cells from the start of a captured editor's buffer line to a
/// byte column in it, with the editor's tab size.
#[host_operation("editorCellsBefore")]
pub(crate) async fn editor_cells_before(
    context: HostContext,
    arguments: LineMeasureArguments,
) -> Result<u64, EditorError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let line = usize::try_from(arguments.line).map_err(|_| EditorError::InvalidRange)?;
        let column = usize::try_from(arguments.value).map_err(|_| EditorError::InvalidRange)?;
        let cells = editor
            .read(cx)
            .line_cells_before(line, column, cx)
            .ok_or(EditorError::InvalidRange)?;
        Ok(cells as u64)
    })?
}

/// The byte column of the character covering a display cell in a captured
/// editor's buffer line, with the editor's tab size.
#[host_operation("editorColumnAtCells")]
pub(crate) async fn editor_column_at_cells(
    context: HostContext,
    arguments: LineMeasureArguments,
) -> Result<u64, EditorError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let line = usize::try_from(arguments.line).map_err(|_| EditorError::InvalidRange)?;
        let cells = usize::try_from(arguments.value).map_err(|_| EditorError::InvalidRange)?;
        let column = editor
            .read(cx)
            .line_column_at_cells(line, cells, cx)
            .ok_or(EditorError::InvalidRange)?;
        Ok(column as u64)
    })?
}

#[host_operation("editorViewState")]
pub(crate) async fn editor_view_state(
    context: HostContext,
    arguments: EditorArguments,
) -> Result<crate::app::editor::EditorViewState, AccessError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        Ok(editor.read(cx).view_state(cx))
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RestoreViewStateArguments {
    pub authority: EditorAuthority,
    pub state: crate::app::editor::EditorViewState,
}
impl HostArguments for RestoreViewStateArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let offset = |reader: &mut dyn ArgumentReader, index| {
            usize::try_from(reader.integer(index)?)
                .map_err(|_| DecodeError::Range("view state offsets must fit in memory"))
        };
        let authority = EditorAuthority::decode(reader, 2)?;
        let anchor = offset(reader, 3)?;
        let caret = offset(reader, 4)?;
        let top = offset(reader, 5)?;
        let knot_protocol::CommandArgumentValue::Number(scroll_x) = reader.command_arguments(6)?
        else {
            return Err(DecodeError::Type("view state scrollX must be a number"));
        };
        Ok(Self {
            authority,
            state: crate::app::editor::EditorViewState {
                anchor,
                caret,
                top,
                scroll_x: scroll_x as f32,
                folds: Vec::new(),
            },
        })
    }
}

/// Restore a captured editor view state. Offsets clamp to the current text.
#[host_operation("editorRestoreViewState")]
pub(crate) async fn editor_restore_view_state(
    context: HostContext,
    arguments: RestoreViewStateArguments,
) -> Result<(), AccessError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        editor.update(cx, |editor, cx| {
            editor.break_history_group(cx);
            // Extensions restore selection and scroll; folds stay as they are.
            let mut state = arguments.state;
            state.folds = editor.view_state(cx).folds;
            editor.restore_view_state(state, cx);
        });
        Ok(())
    })?
}

pub(crate) use knot_protocol::KeybindingArguments;

#[derive(Debug, HostError)]
pub(crate) enum KeymapError {
    #[error(transparent)]
    Access(AccessError),
    #[error(code = "UnsupportedOperation", message = "{message}")]
    Invalid { message: String },
}
impl KeymapError {
    pub(crate) fn invalid(message: String) -> Self {
        Self::Invalid { message }
    }
}
impl From<AccessError> for KeymapError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

#[host_operation("mutateKeybindings")]
pub(crate) async fn mutate_keybindings(
    context: HostContext,
    arguments: KeybindingArguments,
) -> Result<(), KeymapError> {
    let owner = crate::app::commands::keymaps::BindingOwner::Extension(
        context.request.extension,
        context.request.lifecycle,
    );
    context.foreground(|_, cx| {
        if !cx.has_global::<crate::app::commands::keymaps::ApplicationKeymaps>() {
            return Err(KeymapError::invalid(
                "application keymap is unavailable".into(),
            ));
        }
        let result = cx
            .global_mut::<crate::app::commands::keymaps::ApplicationKeymaps>()
            .0
            .apply_batch(owner, &arguments.mutations);
        if matches!(result, Ok(true)) {
            crate::app::commands::keymaps::rebuild(cx);
        }
        result.map(|_| ()).map_err(KeymapError::invalid)
    })?
}

#[derive(Debug, HostError)]
pub(crate) enum RecordError {
    #[error(transparent)]
    Access(AccessError),
    #[error(code = "StorageUnavailable", message = "Storage {database}: {cause}")]
    Unavailable { database: String, cause: String },
}

impl From<AccessError> for RecordError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

impl From<StorageError> for RecordError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::Cancelled => Self::Access(AccessError::Cancelled),
            StorageError::Unavailable { database, cause } => Self::Unavailable {
                database: database.display().to_string(),
                cause,
            },
        }
    }
}

impl HostContext {
    fn records(&self) -> Result<OwnerStorage, RecordError> {
        self.foreground(|host, _| {
            if host.storage_stopping {
                return Err(AccessError::Cancelled.into());
            }
            let key = ExtensionKey::new(self.request.extension, self.request.lifecycle);
            let owner = host
                .storage_owners
                .get(&key)
                .ok_or(AccessError::ContextUnavailable)?;
            let storage = host
                .storage
                .as_ref()
                .map_err(|cause| RecordError::Unavailable {
                    database: "local application-data directory".into(),
                    cause: cause.clone(),
                })?;
            let records = storage.for_owner(owner.clone());
            let caller = Arc::new(std::sync::atomic::AtomicBool::new(true));
            host.storage_callers
                .push((self.request, Arc::downgrade(&caller)));
            Ok(records.with_caller(caller))
        })?
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RecordKey {
    key: String,
}
impl HostArguments for RecordKey {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            key: reader.text(2, "storage key must be text")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RecordWrite {
    key: String,
    value: serde_json::Value,
}
impl HostArguments for RecordWrite {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            key: reader.text(2, "storage key must be text")?,
            value: serde_json::to_value(reader.command_arguments(3)?).expect("validated JSON"),
        })
    }
}

pub(crate) struct RecordResult(Option<serde_json::Value>);
impl HostSuccess for RecordResult {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        match self.0 {
            Some(value) => encode_json(serde_json::json!({ "found": true, "value": value })),
            None => encode_json(serde_json::json!({ "found": false })),
        }
    }
}

#[host_operation("storageRead")]
async fn storage_read(
    context: HostContext,
    arguments: RecordKey,
) -> Result<RecordResult, RecordError> {
    Ok(RecordResult(context.records()?.read(arguments.key).await?))
}

#[host_operation("storageWrite")]
async fn storage_write(context: HostContext, arguments: RecordWrite) -> Result<(), RecordError> {
    Ok(context
        .records()?
        .write(arguments.key, arguments.value)
        .await?)
}

#[host_operation("storageDelete")]
async fn storage_delete(context: HostContext, arguments: RecordKey) -> Result<(), RecordError> {
    Ok(context.records()?.delete(arguments.key).await?)
}

#[host_operation("storageList")]
async fn storage_list(context: HostContext, _: ()) -> Result<Vec<String>, RecordError> {
    Ok(context.records()?.list().await?)
}

#[derive(Debug, HostError)]
pub(crate) enum CreateBufferError {
    #[error(transparent)]
    Access(AccessError),
    #[error(code = "StorageUnavailable", message = "Storage {database}: {cause}")]
    Unavailable { database: String, cause: String },
    #[error(code = "PersistKeyInUse", message = "PersistKeyInUse")]
    PersistKeyInUse,
}

impl From<AccessError> for CreateBufferError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

impl From<RecordError> for CreateBufferError {
    fn from(error: RecordError) -> Self {
        match error {
            RecordError::Access(error) => Self::Access(error),
            RecordError::Unavailable { database, cause } => Self::Unavailable { database, cause },
        }
    }
}

impl From<StorageError> for CreateBufferError {
    fn from(error: StorageError) -> Self {
        RecordError::from(error).into()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CreateBufferArguments {
    text: String,
    language: Option<String>,
    read_only: bool,
    persist: Option<String>,
}
impl HostArguments for CreateBufferArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            text: reader.text(2, "buffer text must be text")?,
            language: reader.optional_text(3, "language must be text or null")?,
            read_only: reader
                .optional_boolean(4, "readOnly must be a boolean")?
                .unwrap_or(false),
            persist: reader.optional_text(5, "persist must be text or null")?,
        })
    }
}

/// Create a buffer outside document identity, owned by the calling
/// lifecycle. A persist key starts it from the text stored under that key.
#[host_operation("createBuffer")]
async fn create_buffer(
    context: HostContext,
    arguments: CreateBufferArguments,
) -> Result<u64, CreateBufferError> {
    let stored = match &arguments.persist {
        Some(key) => context
            .records()?
            .read(crate::app::extensions::host::extension_buffers::persist_record(key))
            .await?
            .and_then(|value| match value {
                serde_json::Value::String(text) => Some(text),
                _ => None,
            }),
        None => None,
    };
    let request = context.request;
    let spec = crate::app::extensions::host::extension_buffers::BufferSpec {
        text: arguments.text,
        language: arguments.language,
        read_only: arguments.read_only,
        persist: arguments.persist,
    };
    context
        .foreground(|host, cx| host.create_extension_buffer(&request, spec, stored, cx))?
        .map(|handle| handle.value())
        .map_err(|_| CreateBufferError::PersistKeyInUse)
}

/// End a buffer a package created, keeping its persisted text.
#[host_operation("disposeBuffer")]
async fn dispose_buffer(
    context: HostContext,
    arguments: BufferArguments,
) -> Result<(), BufferError> {
    let request = context.request;
    context.foreground(|host, cx| host.dispose_extension_buffer(&request, arguments.buffer, cx))?
}

/// Remove the text a buffer persisted.
#[host_operation("clearPersistedBuffer")]
async fn clear_persisted_buffer(
    context: HostContext,
    arguments: BufferArguments,
) -> Result<(), BufferError> {
    let request = context.request;
    context.foreground(|host, cx| host.clear_persisted_buffer(&request, arguments.buffer, cx))?
}

#[derive(Debug, HostError)]
pub(crate) enum RecoveryError {
    #[error(transparent)]
    Open(OpenError),
    #[error(code = "CaptureNotFound", message = "CaptureNotFound")]
    CaptureNotFound,
    #[error(code = "RecoveryUnavailable", message = "Recovery: {cause}")]
    Unavailable { cause: String },
}

impl From<AccessError> for RecoveryError {
    fn from(error: AccessError) -> Self {
        Self::Open(error.into())
    }
}

impl From<OpenError> for RecoveryError {
    fn from(error: OpenError) -> Self {
        Self::Open(error)
    }
}

impl From<String> for RecoveryError {
    fn from(cause: String) -> Self {
        Self::Unavailable { cause }
    }
}

impl From<crate::app::recovery::RestoreError> for RecoveryError {
    fn from(error: crate::app::recovery::RestoreError) -> Self {
        use crate::app::recovery::RestoreError;
        match error {
            RestoreError::Unavailable => Self::CaptureNotFound,
            RestoreError::Resource(error) => Self::Open(product::operations::resource_error(error)),
            RestoreError::Store(cause) => Self::Unavailable { cause },
        }
    }
}

impl HostContext {
    /// The calling lifecycle's canonical owner, stamped at admission. Recovery
    /// captures are never addressed by an author-supplied owner.
    fn recovery_owner(&self) -> Result<String, RecoveryError> {
        self.foreground(|host, _| {
            let key = ExtensionKey::new(self.request.extension, self.request.lifecycle);
            host.storage_owners
                .get(&key)
                .map(|owner| owner.identity())
                .ok_or(AccessError::ContextUnavailable)
        })?
        .map_err(RecoveryError::from)
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RetainedCapture {
    id: String,
    document: String,
    title: String,
    kind: &'static str,
    resource_uri: Option<String>,
    captured_at: i64,
}

impl From<crate::app::recovery::StoredSnapshot> for RetainedCapture {
    fn from(snapshot: crate::app::recovery::StoredSnapshot) -> Self {
        use crate::app::recovery::SnapshotState;
        Self {
            id: snapshot.capture_key,
            document: snapshot.document_id.to_string(),
            title: snapshot.title,
            kind: match snapshot.state {
                SnapshotState::Untitled => "untitled",
                SnapshotState::Destination => "destination",
                SnapshotState::Persisted => "persisted",
            },
            resource_uri: snapshot.resource_uri,
            captured_at: snapshot.captured_at_ms,
        }
    }
}

impl HostSuccess for RetainedCapture {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(self)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CaptureLookup {
    id: String,
}

impl HostArguments for CaptureLookup {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            id: reader.text(2, "capture identity must be text")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CaptureRestore {
    id: String,
    target: Option<knot_protocol::DocumentDestination>,
}

impl HostArguments for CaptureRestore {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            id: reader.text(2, "capture identity must be text")?,
            target: reader.destination(3)?,
        })
    }
}

pub(crate) struct RecoveryRestore {
    outcome: serde_json::Value,
    presentation: knot_protocol::DocumentPresentation,
}

impl HostSuccess for RecoveryRestore {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        let mut value = self.outcome;
        value["presentation"] = serde_json::to_value(self.presentation).expect("serializable");
        encode_json(value)
    }
}

#[host_operation("recoveryList")]
async fn recovery_list(context: HostContext, _: ()) -> Result<Vec<RetainedCapture>, RecoveryError> {
    let owner = context.recovery_owner()?;
    let listing = context.foreground(|_, cx| crate::app::recovery::retained(owner, cx))?;
    Ok(listing.await?.into_iter().map(Into::into).collect())
}

#[host_operation("recoveryRetain")]
async fn recovery_retain(
    context: HostContext,
    arguments: DocumentLookup,
) -> Result<RetainedCapture, RecoveryError> {
    let owner = context.recovery_owner()?;
    let document = uuid::Uuid::parse_str(&arguments.id).map_err(|_| OpenError::DocumentNotFound)?;
    let retain = context
        .foreground(|_, cx| crate::app::recovery::retain(owner, document, cx))?
        .ok_or(OpenError::DocumentNotFound)?;
    Ok(retain.await?.into())
}

#[host_operation("recoveryRelease")]
async fn recovery_release(
    context: HostContext,
    arguments: CaptureLookup,
) -> Result<bool, RecoveryError> {
    let owner = context.recovery_owner()?;
    let release =
        context.foreground(|_, cx| crate::app::recovery::release(owner, arguments.id, cx))?;
    Ok(release.await?)
}

#[host_operation("recoveryRestore")]
async fn recovery_restore(
    context: HostContext,
    arguments: CaptureRestore,
) -> Result<RecoveryRestore, RecoveryError> {
    use crate::app::recovery::{RestoreOutcome, RestoredResource};
    let owner = context.recovery_owner()?;
    let request = context.request;
    let (destination, restore) = context.foreground(|host, cx| {
        let destination = host.document_destination(&request, arguments.target, cx)?;
        let restore = product::operations::restore_retained(owner, arguments.id, cx);
        Ok::<_, AccessError>((destination, restore))
    })??;
    let outcome = restore.await?;
    context.foreground(move |host, cx| {
        let (document, outcome) = match outcome {
            RestoreOutcome::Restored { document, resource } => (
                document,
                serde_json::json!({
                    "outcome": "restored",
                    "resource": match resource {
                        RestoredResource::Untitled => "untitled",
                        RestoredResource::Unchanged => "unchanged",
                        RestoredResource::Missing => "missing",
                        RestoredResource::Changed => "changed",
                    },
                }),
            ),
            RestoreOutcome::Preserved { owner, document } => {
                let documents = cx.global::<crate::app::documents::ApplicationDocuments>();
                let owner = documents
                    .0
                    .read(cx)
                    .get(owner)
                    .map(|owner| owner.identity());
                (
                    document,
                    serde_json::json!({
                        "outcome": "preserved",
                        "owner": owner.map(|owner| owner.to_string()),
                    }),
                )
            }
            RestoreOutcome::AlreadyOpen { .. } => {
                unreachable!("only automatic snapshots restore as already open")
            }
        };
        let identity = cx
            .global::<crate::app::documents::ApplicationDocuments>()
            .0
            .read(cx)
            .get(document)
            .map(|document| document.identity().to_string())
            .ok_or(OpenError::DocumentNotFound)?;
        let (document, target) = product::operations::present_document(identity, destination, cx)?;
        Ok(RecoveryRestore {
            outcome,
            presentation: host.document_presentation(document, target, cx)?,
        })
    })?
}

/// Failures of `workbench.replaceWindows`. A rejection leaves every window,
/// document, and capture as it was.
#[derive(Debug, HostError)]
pub(crate) enum ReplaceError {
    #[error(transparent)]
    Resource(OpenError),
    #[error(code = "OperationInProgress", message = "OperationInProgress")]
    OperationInProgress,
    #[error(code = "SessionConflict", message = "SessionConflict: {reason}")]
    Conflict { reason: String },
}

impl From<AccessError> for ReplaceError {
    fn from(error: AccessError) -> Self {
        Self::Resource(error.into())
    }
}

impl From<OpenError> for ReplaceError {
    fn from(error: OpenError) -> Self {
        Self::Resource(error)
    }
}

impl From<RecoveryError> for ReplaceError {
    fn from(error: RecoveryError) -> Self {
        match error {
            RecoveryError::Open(error) => Self::Resource(error),
            RecoveryError::CaptureNotFound | RecoveryError::Unavailable { .. } => {
                Self::Resource(OpenError::Access(AccessError::ContextUnavailable))
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReplaceWindowsArguments {
    request: product::sessions::ReplaceWindowsRequest,
}

impl HostArguments for ReplaceWindowsArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let request = serde_json::to_value(reader.command_arguments(2)?)
            .ok()
            .and_then(|value| serde_json::from_value(value).ok())
            .ok_or(DecodeError::Type("invalid window replacement"))?;
        Ok(Self { request })
    }
}

impl HostSuccess for product::sessions::ReplaceWindowsResult {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(self)
    }
}

#[host_operation("replaceWindows")]
async fn replace_windows(
    context: HostContext,
    arguments: ReplaceWindowsArguments,
) -> Result<product::sessions::ReplaceWindowsResult, ReplaceError> {
    let owner = context.recovery_owner()?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let request = context.request;
    let caller = product::operations::OperationCaller {
        host: context.host.clone(),
        key: ExtensionKey::new(request.extension, request.lifecycle),
        invocation: request.invocation,
    };
    context.foreground(|_, cx| {
        product::sessions::replace_windows(
            arguments.request,
            owner,
            Some(caller),
            move |result| {
                let _ = sender.send(result);
            },
            cx,
        )
    })??;
    receiver.await.map_err(|_| AccessError::Cancelled)?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OpenWindowArguments {
    pub roots: Vec<String>,
}

impl HostArguments for OpenWindowArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            roots: reader.text_list(2)?,
        })
    }
}

#[host_operation("openWindow")]
async fn open_window(
    context: HostContext,
    arguments: OpenWindowArguments,
) -> Result<knot_protocol::WindowInfo, WorkspaceError> {
    // The window's window observes the host, so it opens after this host update.
    let (sender, receiver) = tokio::sync::oneshot::channel();
    context.foreground(|_, cx| {
        cx.defer(move |cx| {
            let _ = sender.send(product::sessions::open_window(cx));
        })
    })?;
    let target = receiver.await.map_err(|_| AccessError::Cancelled)?;
    if !arguments.roots.is_empty() {
        let replaced = replace_workspace_roots(
            context.clone(),
            WorkspaceArguments {
                roots: arguments.roots,
                target: Some(target.clone()),
            },
        )
        .await;
        if let Err(error) = replaced {
            let _ = context.foreground(|_, cx| product::sessions::discard_window(&target, cx));
            return Err(error);
        }
    }
    let window = context.foreground(|host, cx| {
        host.application_snapshot(cx)
            .windows
            .into_iter()
            .find(|window| window.target == target)
    })?;
    Ok(window.ok_or(AccessError::InvalidTarget)?)
}

impl HostSuccess for crate::app::window_layout::WindowLayout {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(self)
    }
}

#[host_operation("captureLayout")]
async fn capture_layout(
    context: HostContext,
    arguments: WindowLookup,
) -> Result<crate::app::window_layout::WindowLayout, AccessError> {
    context.foreground(|_, cx| product::sessions::capture_layout(&arguments.target, cx))?
}

#[host_operation("focusWindow")]
async fn focus_window(context: HostContext, arguments: WindowLookup) -> Result<(), AccessError> {
    context.foreground(|_, cx| product::sessions::focus_window(&arguments.target, cx))?
}

use crate::app::languages::{LanguageDefinition, LanguageQueries, editing::EditingDefinition};
use crate::app::syntax::{SyntaxLanguageError, SyntaxQuery};
use knot_protocol::LanguageInfo;

/// Failures of a language registration.
#[derive(Debug, HostError)]
pub(crate) enum LanguageError {
    #[error(transparent)]
    Access(AccessError),
    #[error(
        code = "UnknownGrammar",
        message = "no compiled-in grammar is named {grammar}"
    )]
    UnknownGrammar { grammar: String },
    #[error(
        code = "InvalidQuery",
        message = "invalid {query} query at {row}:{column}: {message}"
    )]
    InvalidQuery {
        query: String,
        message: String,
        row: usize,
        column: usize,
    },
    #[error(
        code = "LanguageRegistrationNotFound",
        message = "LanguageRegistrationNotFound"
    )]
    RegistrationNotFound,
}

impl From<AccessError> for LanguageError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

impl LanguageError {
    fn from_syntax(error: SyntaxLanguageError, grammar: String) -> Self {
        match error {
            SyntaxLanguageError::UnknownGrammar => Self::UnknownGrammar { grammar },
            SyntaxLanguageError::InvalidQuery {
                query,
                message,
                row,
                column,
            } => Self::InvalidQuery {
                query: match query {
                    SyntaxQuery::Highlights => "highlights",
                    SyntaxQuery::Injections => "injections",
                    SyntaxQuery::Indents => "indents",
                    SyntaxQuery::Folds => "folds",
                }
                .into(),
                message,
                row,
                column,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct LanguageArguments(LanguageDefinition);

impl HostArguments for LanguageArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self(LanguageDefinition {
            info: LanguageInfo {
                id: reader.text(2, "language id must be text")?,
                name: reader.text(3, "language name must be text")?,
                grammar: reader.text(4, "language grammar must be text")?,
                file_extensions: reader.text_list(5)?,
                file_names: reader.text_list(6)?,
                interpreters: reader.text_list(7)?,
            },
            queries: LanguageQueries {
                highlights: reader.text(8, "highlights query must be text")?,
                injections: reader.text(9, "injections query must be text")?,
                indents: reader.text(10, "indents query must be text")?,
                folds: reader.text(11, "folds query must be text")?,
            },
            editing: serde_json::from_value::<EditingDefinition>(setting_value(
                reader.command_arguments(12)?,
            ))
            .map_err(|_| DecodeError::Type("invalid language editing data"))?,
        }))
    }
}

#[host_operation("registerLanguage")]
async fn register_language(
    context: HostContext,
    arguments: LanguageArguments,
) -> Result<u64, LanguageError> {
    let arguments = arguments.0;
    let request = context.request;
    // Large queries take tens of milliseconds to compile, and a package
    // registers many languages, so they compile in parallel off the foreground.
    let grammar = arguments.info.grammar.clone();
    let language = context
        .application
        .background_executor()
        .spawn(async move { arguments.compile() })
        .await
        .map_err(|error| LanguageError::from_syntax(error, grammar))?;
    let owner = crate::app::languages::LanguageOwner {
        extension: request.extension,
        lifecycle: request.lifecycle,
    };
    context
        .foreground(|_, cx| crate::app::languages::register(owner, language, cx))
        .map_err(LanguageError::from)
}

#[derive(Clone, Debug, PartialEq)]
struct LanguageAssociationArguments(crate::app::languages::LanguageAssociation);

impl HostArguments for LanguageAssociationArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self(crate::app::languages::LanguageAssociation {
            language: reader.text(2, "language id must be text")?,
            file_extensions: reader.text_list(3)?,
            file_names: reader.text_list(4)?,
            interpreters: reader.text_list(5)?,
        }))
    }
}

/// Claim file patterns for a language ID; `unregisterLanguage` removes the
/// claim like a registration.
#[host_operation("associateLanguage")]
async fn associate_language(
    context: HostContext,
    arguments: LanguageAssociationArguments,
) -> Result<u64, AccessError> {
    let arguments = arguments.0;
    let owner = crate::app::languages::LanguageOwner {
        extension: context.request.extension,
        lifecycle: context.request.lifecycle,
    };
    context.foreground(|_, cx| crate::app::languages::associate(owner, arguments, cx))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LanguageRegistrationArguments {
    registration: u64,
}

impl HostArguments for LanguageRegistrationArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            registration: reader.integer(2)?,
        })
    }
}

#[host_operation("unregisterLanguage")]
async fn unregister_language(
    context: HostContext,
    arguments: LanguageRegistrationArguments,
) -> Result<(), LanguageError> {
    let request = context.request;
    context.foreground(|_, cx| {
        if crate::app::languages::unregister(
            request.extension,
            request.lifecycle,
            arguments.registration,
            cx,
        ) {
            Ok(())
        } else {
            Err(LanguageError::RegistrationNotFound)
        }
    })?
}

#[host_operation("listLanguages")]
async fn list_languages(context: HostContext, _: ()) -> Result<Vec<LanguageInfo>, AccessError> {
    context.foreground(|_, cx| crate::app::languages::list(cx))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LanguageId {
    id: String,
}

impl HostArguments for LanguageId {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            id: reader.text(2, "language id must be text")?,
        })
    }
}

#[host_operation("getLanguage")]
async fn get_language(
    context: HostContext,
    arguments: LanguageId,
) -> Result<Option<LanguageDefinition>, AccessError> {
    context.foreground(|_, cx| crate::app::languages::get(&arguments.id, cx))
}

#[host_operation("listGrammars")]
async fn list_grammars(context: HostContext, _: ()) -> Result<Vec<String>, AccessError> {
    context.foreground(|_, _| {
        crate::app::syntax::grammars::grammar_ids()
            .map(str::to_owned)
            .collect()
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocumentLanguageArguments {
    id: String,
    language: Option<String>,
}

impl HostArguments for DocumentLanguageArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            id: reader.text(2, "document identity must be text")?,
            language: reader.optional_text(3, "language must be text or null")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocumentIdArguments {
    id: String,
}

impl HostArguments for DocumentIdArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            id: reader.text(2, "document identity must be text")?,
        })
    }
}

#[host_operation("reloadDocument")]
async fn reload_document(
    context: HostContext,
    arguments: DocumentIdArguments,
) -> Result<(), OpenError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    context.foreground(move |_, cx| {
        product::operations::reload_document(
            &arguments.id,
            move |result, _| {
                let _ = sender.send(result);
            },
            cx,
        )
    })??;
    receiver.await.map_err(|_| AccessError::Cancelled)?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SaveDocumentArguments {
    id: String,
    uri: Option<String>,
}

impl HostArguments for SaveDocumentArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            id: reader.text(2, "document identity must be text")?,
            uri: reader.optional_text(3, "resource URI must be text or null")?,
        })
    }
}

#[host_operation("saveDocument")]
async fn save_document(
    context: HostContext,
    arguments: SaveDocumentArguments,
) -> Result<(), OpenError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let target = arguments.uri.clone();
    context.foreground(move |_, cx| {
        product::operations::save_document(
            &arguments.id,
            arguments.uri,
            move |result, _| {
                let _ = sender.send(result);
            },
            cx,
        )
    })??;
    receiver.await.map_err(|_| AccessError::Cancelled)??;
    // A file the save created is listed right away, as after `files.writeText`.
    if let Some(target) = target {
        let providers = context.foreground(|_, cx| KnotWindow::filesystems(cx))?;
        if let Ok(uri) = knot_model::files::normalize(&providers, &target).await {
            refresh_file_index(&context, std::slice::from_ref(&uri)).await;
        }
    }
    Ok(())
}

#[host_operation("setDocumentLanguage")]
async fn set_document_language(
    context: HostContext,
    arguments: DocumentLanguageArguments,
) -> Result<(), OpenError> {
    context.foreground(|_, cx| {
        let documents = cx
            .try_global::<crate::app::documents::ApplicationDocuments>()
            .ok_or(OpenError::DocumentNotFound)?
            .0
            .clone();
        let (model, uri) = uuid::Uuid::parse_str(&arguments.id)
            .ok()
            .and_then(|identity| {
                let documents = documents.read(cx);
                let document = documents.document_by_identity(identity)?;
                Some((document.model().clone(), document.resource_uri().cloned()))
            })
            .ok_or(OpenError::DocumentNotFound)?;
        crate::app::languages::set_document_language(&model, uri.as_ref(), arguments.language, cx);
        crate::app::application_observation::changed(cx);
        Ok(())
    })?
}

use crate::app::settings::{
    ApplicationSettings, SettingDefinition, SettingOwner, SettingType, SettingsError, SettingsScope,
};
use knot_protocol::SettingsSubscriptionId;

/// Failures of a settings operation.
#[derive(Debug, HostError)]
pub(crate) enum SettingError {
    #[error(transparent)]
    Access(AccessError),
    #[error(code = "DuplicateSetting", message = "{key} is already defined")]
    Duplicate { key: String },
    #[error(
        code = "SettingTypeMismatch",
        message = "{key} requires a value of type {expected}"
    )]
    TypeMismatch { key: String, expected: String },
}

impl From<AccessError> for SettingError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

impl SettingError {
    fn from_settings(error: SettingsError, key: &str) -> Self {
        match error {
            SettingsError::DuplicateSetting => Self::Duplicate { key: key.into() },
            SettingsError::TypeMismatch { expected } => Self::TypeMismatch {
                key: key.into(),
                expected: expected.name().into(),
            },
        }
    }
}

/// A JSON value from JavaScript; see [`knot_model::settings::restore_integers`].
fn setting_value(value: knot_protocol::CommandArgumentValue) -> serde_json::Value {
    knot_model::settings::restore_integers(serde_json::to_value(value).expect("validated JSON"))
}

fn settings_scope(
    reader: &mut dyn ArgumentReader,
    index: usize,
) -> Result<SettingsScope, DecodeError> {
    Ok(SettingsScope {
        language: reader.optional_text(index, "scope language must be text")?,
        view: reader.optional_text(index + 1, "scope view must be text")?,
        document: reader.optional_text(index + 2, "scope document must be text")?,
    })
}

#[derive(Clone, Debug, PartialEq)]
struct SettingDefinitionArguments(SettingDefinition);

impl HostArguments for SettingDefinitionArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self(SettingDefinition {
            key: reader.text(2, "setting key must be text")?,
            value_type: SettingType::parse(&reader.text(3, "setting type must be text")?).ok_or(
                DecodeError::Type(
                    "setting type must be boolean, integer, number, string, or strings",
                ),
            )?,
            default: setting_value(reader.command_arguments(4)?),
            description: reader.text(5, "setting description must be text")?,
        }))
    }
}

#[host_operation("defineSetting")]
async fn define_setting(
    context: HostContext,
    arguments: SettingDefinitionArguments,
) -> Result<(), SettingError> {
    let arguments = arguments.0;
    let request = context.request;
    context.foreground(|host, cx| {
        let key = arguments.key.clone();
        let owner = SettingOwner::Extension {
            extension: request.extension,
            lifecycle: request.lifecycle,
        };
        let value_type = arguments.value_type;
        let dropped = cx
            .default_global::<ApplicationSettings>()
            .0
            .define(owner, arguments)
            .map_err(|error| SettingError::from_settings(error, &key))?;
        for scope in dropped {
            host.report_setting_warning(
                format!(
                    "Setting {key}: dropped the value for {} because it is not of type {}",
                    scope.describe(),
                    value_type.name()
                ),
                cx,
            );
        }
        Ok(())
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SettingWrites {
    values: Vec<(String, serde_json::Value)>,
    scope: SettingsScope,
}

impl HostArguments for SettingWrites {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let serde_json::Value::Object(values) = setting_value(reader.command_arguments(2)?) else {
            return Err(DecodeError::Type("setting values must be an object"));
        };
        Ok(Self {
            values: values.into_iter().collect(),
            scope: settings_scope(reader, 3)?,
        })
    }
}

/// Write every value or none: a mismatching value rejects the whole call.
#[host_operation("setSettings")]
async fn set_settings(context: HostContext, arguments: SettingWrites) -> Result<(), SettingError> {
    context.foreground(|host, cx| {
        let settings = &mut cx.default_global::<ApplicationSettings>().0;
        let keys = settings
            .set_all(arguments.values, &arguments.scope)
            .map_err(|(key, error)| SettingError::from_settings(error, &key))?;
        for key in keys {
            host.setting_changed(&key);
        }
        crate::app::application_observation::changed(cx);
        // Native views read settings while laying out.
        cx.refresh_windows();
        Ok(())
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SettingLookup {
    key: String,
    scope: SettingsScope,
}

impl HostArguments for SettingLookup {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            key: reader.text(2, "setting key must be text")?,
            scope: settings_scope(reader, 3)?,
        })
    }
}

pub(crate) struct SettingValue(Option<serde_json::Value>);

impl HostSuccess for SettingValue {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(self.0.unwrap_or(serde_json::Value::Null))
    }
}

#[host_operation("getSetting")]
async fn get_setting(
    context: HostContext,
    arguments: SettingLookup,
) -> Result<SettingValue, AccessError> {
    context.foreground(|_, cx| {
        SettingValue(
            cx.try_global::<ApplicationSettings>()
                .and_then(|settings| settings.0.get(&arguments.key, &arguments.scope))
                .cloned(),
        )
    })
}

#[host_operation("resetSetting")]
async fn reset_setting(context: HostContext, arguments: SettingLookup) -> Result<(), AccessError> {
    context.foreground(|host, cx| {
        if cx.has_global::<ApplicationSettings>()
            && cx
                .global_mut::<ApplicationSettings>()
                .0
                .reset(&arguments.key, &arguments.scope)
        {
            host.setting_changed(&arguments.key);
            crate::app::application_observation::changed(cx);
            cx.refresh_windows();
        }
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SettingKey {
    key: String,
}

impl HostArguments for SettingKey {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            key: reader.text(2, "setting key must be text")?,
        })
    }
}

#[host_operation("observeSetting")]
async fn observe_setting(
    context: HostContext,
    arguments: SettingKey,
) -> Result<SettingsSubscriptionId, AccessError> {
    let key = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context.foreground(|host, _| host.observe_setting(arguments.key, key))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SettingSubscription {
    subscription: SettingsSubscriptionId,
}

impl HostArguments for SettingSubscription {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            subscription: SettingsSubscriptionId::new(reader.integer(2)?),
        })
    }
}

#[host_operation("unobserveSetting")]
async fn unobserve_setting(
    context: HostContext,
    arguments: SettingSubscription,
) -> Result<(), AccessError> {
    let key = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context.foreground(|host, _| {
        // A subscription the host already released is as good as disposed.
        host.unobserve_setting(arguments.subscription, key);
        Ok(())
    })?
}

use crate::app::commands::keymaps::{
    ApplicationKeymapPresets, KeymapPreset, KeymapPresetInfo, PresetOwner,
};

/// Failures of a keymap preset operation.
#[derive(Debug, HostError)]
pub(crate) enum KeymapPresetError {
    #[error(transparent)]
    Access(AccessError),
    #[error(
        code = "KeymapPresetRegistrationNotFound",
        message = "KeymapPresetRegistrationNotFound"
    )]
    RegistrationNotFound,
}

impl From<AccessError> for KeymapPresetError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

fn preset_owner(context: &HostContext) -> PresetOwner {
    PresetOwner {
        extension: context.request.extension,
        lifecycle: context.request.lifecycle,
    }
}

/// The keymap preset a package registers.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RegisteredPreset(KeymapPreset);

impl HostArguments for RegisteredPreset {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let preset: KeymapPreset =
            serde_json::from_value(setting_value(reader.command_arguments(2)?))
                .map_err(|_| DecodeError::Type("invalid keymap preset"))?;
        match preset.invalid_value() {
            Some(message) => Err(DecodeError::Type(message)),
            None => Ok(Self(preset)),
        }
    }
}

#[host_operation("registerKeymapPreset")]
async fn register_keymap_preset(
    context: HostContext,
    arguments: RegisteredPreset,
) -> Result<u64, AccessError> {
    let owner = preset_owner(&context);
    context.foreground(|_, cx| {
        cx.default_global::<ApplicationKeymapPresets>()
            .0
            .register(owner, arguments.0)
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KeymapPresetRegistrationArguments {
    registration: u64,
}

impl HostArguments for KeymapPresetRegistrationArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            registration: reader.integer(2)?,
        })
    }
}

#[host_operation("unregisterKeymapPreset")]
async fn unregister_keymap_preset(
    context: HostContext,
    arguments: KeymapPresetRegistrationArguments,
) -> Result<(), KeymapPresetError> {
    let owner = preset_owner(&context);
    context.foreground(|_, cx| {
        if cx
            .default_global::<ApplicationKeymapPresets>()
            .0
            .unregister(owner, arguments.registration)
        {
            Ok(())
        } else {
            Err(KeymapPresetError::RegistrationNotFound)
        }
    })?
}

#[host_operation("listKeymapPresets")]
async fn list_keymap_presets(
    context: HostContext,
    _: (),
) -> Result<Vec<KeymapPresetInfo>, AccessError> {
    context.foreground(|_, cx| cx.default_global::<ApplicationKeymapPresets>().0.list())
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KeymapPresetName {
    name: String,
}

impl HostArguments for KeymapPresetName {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            name: reader.text(2, "keymap preset name must be text")?,
        })
    }
}

#[host_operation("getKeymapPreset")]
async fn get_keymap_preset(
    context: HostContext,
    arguments: KeymapPresetName,
) -> Result<Option<KeymapPreset>, AccessError> {
    context.foreground(|_, cx| {
        cx.default_global::<ApplicationKeymapPresets>()
            .0
            .get(&arguments.name)
    })
}

use crate::app::theme::ColorDefinition;
use crate::app::themes::{ApplicationThemes, ThemeDefinition, ThemeInfo, ThemeOwner};

/// Failures of a theme operation.
#[derive(Debug, HostError)]
pub(crate) enum ThemeError {
    #[error(transparent)]
    Access(AccessError),
    #[error(
        code = "UnknownCapture",
        message = "{capture} is not a highlight capture name or prefix"
    )]
    UnknownCapture { capture: String },
    #[error(code = "DuplicateColor", message = "{key} is already defined")]
    DuplicateColor { key: String },
    #[error(
        code = "ThemeRegistrationNotFound",
        message = "ThemeRegistrationNotFound"
    )]
    RegistrationNotFound,
}

impl From<AccessError> for ThemeError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

fn theme_owner(context: &HostContext) -> ThemeOwner {
    ThemeOwner {
        extension: context.request.extension,
        lifecycle: context.request.lifecycle,
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ThemeDefinitionArguments(ThemeDefinition);

impl HostArguments for ThemeDefinitionArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let definition: ThemeDefinition =
            serde_json::from_value(setting_value(reader.command_arguments(2)?))
                .map_err(|_| DecodeError::Type("invalid theme definition"))?;
        match definition.invalid_value() {
            Some(message) => Err(DecodeError::Type(message)),
            None => Ok(Self(definition)),
        }
    }
}

#[host_operation("registerTheme")]
async fn register_theme(
    context: HostContext,
    arguments: ThemeDefinitionArguments,
) -> Result<u64, ThemeError> {
    let arguments = arguments.0;
    if let Some(capture) = arguments.unknown_capture() {
        return Err(ThemeError::UnknownCapture {
            capture: capture.into(),
        });
    }
    let owner = theme_owner(&context);
    Ok(context.foreground(|_, cx| {
        cx.default_global::<ApplicationThemes>()
            .0
            .register(owner, arguments)
    })?)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThemeRegistrationArguments {
    registration: u64,
}

impl HostArguments for ThemeRegistrationArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            registration: reader.integer(2)?,
        })
    }
}

#[host_operation("unregisterTheme")]
async fn unregister_theme(
    context: HostContext,
    arguments: ThemeRegistrationArguments,
) -> Result<(), ThemeError> {
    let owner = theme_owner(&context);
    context.foreground(|_, cx| {
        if cx
            .default_global::<ApplicationThemes>()
            .0
            .unregister(owner, arguments.registration)
        {
            Ok(())
        } else {
            Err(ThemeError::RegistrationNotFound)
        }
    })?
}

use crate::app::icons::{ApplicationIcons, glyph_exists, refresh_icons};
use knot_model::icons::{IconAssociation, IconDefinition, IconOwner, IconSpec};

/// Failures of an icon operation.
#[derive(Debug, HostError)]
pub(crate) enum IconError {
    #[error(transparent)]
    Access(AccessError),
    #[error(code = "DuplicateIcon", message = "{name} already has a default icon")]
    DuplicateIcon { name: String },
    #[error(
        code = "IconRegistrationNotFound",
        message = "IconRegistrationNotFound"
    )]
    RegistrationNotFound,
}

impl From<AccessError> for IconError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

fn icon_owner(context: &HostContext) -> IconOwner {
    IconOwner {
        extension: context.request.extension,
        lifecycle: context.request.lifecycle,
    }
}

fn icon_name(name: String) -> Result<String, DecodeError> {
    let valid = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
    if valid {
        Ok(name)
    } else {
        Err(DecodeError::Type(
            "icon name must use letters, digits, and . - _",
        ))
    }
}

fn icon_definition(
    value: knot_protocol::CommandArgumentValue,
) -> Result<IconDefinition, DecodeError> {
    let spec: IconSpec = serde_json::from_value(setting_value(value))
        .map_err(|_| DecodeError::Type("invalid icon definition"))?;
    if spec.lucide_name().is_some_and(|name| !glyph_exists(name)) {
        return Err(DecodeError::Type("unknown Lucide icon"));
    }
    spec.definition().map_err(DecodeError::Type)
}

/// A name and the definition it gets.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NamedIcon {
    name: String,
    definition: IconDefinition,
}

impl HostArguments for NamedIcon {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            name: icon_name(reader.text(2, "icon name must be text")?)?,
            definition: icon_definition(reader.command_arguments(3)?)?,
        })
    }
}

/// A pack's name and the icons it defines.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct IconPackArguments {
    name: String,
    icons: std::collections::BTreeMap<String, IconDefinition>,
}

impl HostArguments for IconPackArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let name = reader.text(2, "icon pack name must be text")?;
        if name.is_empty() {
            return Err(DecodeError::Type("icon pack name must be nonempty"));
        }
        let knot_protocol::CommandArgumentValue::Object(entries) = reader.command_arguments(3)?
        else {
            return Err(DecodeError::Type("icon pack icons must be an object"));
        };
        let icons = entries
            .into_iter()
            .map(|(icon, value)| Ok((icon_name(icon)?, icon_definition(value)?)))
            .collect::<Result<_, DecodeError>>()?;
        Ok(Self { name, icons })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct IconRegistrationArguments {
    registration: u64,
}

impl HostArguments for IconRegistrationArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            registration: reader.integer(2)?,
        })
    }
}

#[host_operation("defineIcon")]
async fn define_icon(context: HostContext, arguments: NamedIcon) -> Result<(), IconError> {
    let owner = icon_owner(&context);
    context.foreground(|_, cx| {
        let NamedIcon { name, definition } = arguments;
        cx.default_global::<ApplicationIcons>()
            .0
            .define(owner, &name, definition)
            .map_err(|_| IconError::DuplicateIcon { name })?;
        refresh_icons(cx);
        Ok(())
    })?
}

#[host_operation("registerIconOverride")]
async fn register_icon_override(
    context: HostContext,
    arguments: NamedIcon,
) -> Result<u64, IconError> {
    let owner = icon_owner(&context);
    Ok(context.foreground(|_, cx| {
        let id = cx.default_global::<ApplicationIcons>().0.register_override(
            owner,
            &arguments.name,
            arguments.definition,
        );
        refresh_icons(cx);
        id
    })?)
}

#[host_operation("unregisterIconOverride")]
async fn unregister_icon_override(
    context: HostContext,
    arguments: IconRegistrationArguments,
) -> Result<(), IconError> {
    let owner = icon_owner(&context);
    context.foreground(|_, cx| {
        if cx
            .default_global::<ApplicationIcons>()
            .0
            .unregister_override(owner, arguments.registration)
        {
            refresh_icons(cx);
            Ok(())
        } else {
            Err(IconError::RegistrationNotFound)
        }
    })?
}

#[host_operation("registerIconPack")]
async fn register_icon_pack(
    context: HostContext,
    arguments: IconPackArguments,
) -> Result<u64, IconError> {
    let owner = icon_owner(&context);
    Ok(context.foreground(|_, cx| {
        let id = cx.default_global::<ApplicationIcons>().0.register_pack(
            owner,
            &arguments.name,
            arguments.icons,
        );
        refresh_icons(cx);
        id
    })?)
}

#[host_operation("unregisterIconPack")]
async fn unregister_icon_pack(
    context: HostContext,
    arguments: IconRegistrationArguments,
) -> Result<(), IconError> {
    let owner = icon_owner(&context);
    context.foreground(|_, cx| {
        if cx
            .default_global::<ApplicationIcons>()
            .0
            .unregister_pack(owner, arguments.registration)
        {
            refresh_icons(cx);
            Ok(())
        } else {
            Err(IconError::RegistrationNotFound)
        }
    })?
}

/// Pattern-to-name maps for files, extensions, and folders.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct IconAssociationArguments(IconAssociation);

impl HostArguments for IconAssociationArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let mut claims = |index| {
            let knot_protocol::CommandArgumentValue::Object(entries) =
                reader.command_arguments(index)?
            else {
                return Err(DecodeError::Type("icon associations must be objects"));
            };
            entries
                .into_iter()
                .map(|(pattern, value)| {
                    let knot_protocol::CommandArgumentValue::String(icon) = value else {
                        return Err(DecodeError::Type("associated icon names must be text"));
                    };
                    if pattern.is_empty() {
                        return Err(DecodeError::Type("association patterns must be nonempty"));
                    }
                    Ok((pattern, icon_name(icon)?))
                })
                .collect::<Result<_, DecodeError>>()
        };
        Ok(Self(IconAssociation {
            files: claims(2)?,
            extensions: claims(3)?,
            folders: claims(4)?,
        }))
    }
}

#[host_operation("registerIconAssociation")]
async fn register_icon_association(
    context: HostContext,
    arguments: IconAssociationArguments,
) -> Result<u64, IconError> {
    let owner = icon_owner(&context);
    Ok(context.foreground(|_, cx| {
        let id = cx
            .default_global::<ApplicationIcons>()
            .0
            .associate(owner, arguments.0);
        refresh_icons(cx);
        id
    })?)
}

#[host_operation("unregisterIconAssociation")]
async fn unregister_icon_association(
    context: HostContext,
    arguments: IconRegistrationArguments,
) -> Result<(), IconError> {
    let owner = icon_owner(&context);
    context.foreground(|_, cx| {
        if cx
            .default_global::<ApplicationIcons>()
            .0
            .unassociate(owner, arguments.registration)
        {
            refresh_icons(cx);
            Ok(())
        } else {
            Err(IconError::RegistrationNotFound)
        }
    })?
}

#[host_operation("listIconPacks")]
async fn list_icon_packs(context: HostContext, _: ()) -> Result<Vec<String>, AccessError> {
    context.foreground(|_, cx| {
        cx.try_global::<ApplicationIcons>()
            .map(|icons| icons.0.pack_names())
            .unwrap_or_default()
    })
}

use crate::app::status_bar::{ApplicationStatusBar, StatusItem, StatusOwner};

/// Failures of a status item operation.
#[derive(Debug, HostError)]
pub(crate) enum StatusItemError {
    #[error(transparent)]
    Access(AccessError),
    #[error(
        code = "StatusItemRegistrationNotFound",
        message = "StatusItemRegistrationNotFound"
    )]
    RegistrationNotFound,
}

impl From<AccessError> for StatusItemError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

fn status_owner(context: &HostContext) -> StatusOwner {
    StatusOwner {
        extension: context.request.extension,
        lifecycle: context.request.lifecycle,
    }
}

fn status_item(value: knot_protocol::CommandArgumentValue) -> Result<StatusItem, DecodeError> {
    serde_json::from_value(setting_value(value))
        .map_err(|_| DecodeError::Type("invalid status item"))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StatusItemRegistration {
    id: String,
    item: StatusItem,
}

impl HostArguments for StatusItemRegistration {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            id: reader.text(2, "status item id must be text")?,
            item: status_item(reader.command_arguments(3)?)?,
        })
    }
}

#[host_operation("registerStatusItem")]
async fn register_status_item(
    context: HostContext,
    arguments: StatusItemRegistration,
) -> Result<u64, StatusItemError> {
    let owner = status_owner(&context);
    Ok(context.foreground(|_, cx| {
        cx.default_global::<ApplicationStatusBar>()
            .0
            .register(owner, arguments.id, arguments.item)
    })?)
}

/// A new value for a registration, or for one window's replacement of it,
/// where null clears the replacement.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StatusItemUpdate {
    registration: u64,
    item: Option<StatusItem>,
    window: Option<String>,
}

impl HostArguments for StatusItemUpdate {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let item = match reader.command_arguments(3)? {
            knot_protocol::CommandArgumentValue::Null => None,
            value => Some(status_item(value)?),
        };
        Ok(Self {
            registration: reader.integer(2)?,
            item,
            window: reader.optional_text(4, "window target must be text or null")?,
        })
    }
}

#[host_operation("updateStatusItem")]
async fn update_status_item(
    context: HostContext,
    arguments: StatusItemUpdate,
) -> Result<(), StatusItemError> {
    let owner = status_owner(&context);
    context.foreground(|_, cx| {
        // An equal value changes nothing, so it must not notify the windows
        // that observe the bar and redraw them.
        let unchanged = cx.try_global::<ApplicationStatusBar>().is_some_and(|bar| {
            bar.0.holds(
                owner,
                arguments.registration,
                arguments.window.as_deref(),
                arguments.item.as_ref(),
            )
        });
        if unchanged {
            return Ok(());
        }
        if cx.default_global::<ApplicationStatusBar>().0.update(
            owner,
            arguments.registration,
            arguments.window,
            arguments.item,
        ) {
            Ok(())
        } else {
            Err(StatusItemError::RegistrationNotFound)
        }
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StatusItemRegistrationId(u64);

impl HostArguments for StatusItemRegistrationId {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self(reader.integer(2)?))
    }
}

#[host_operation("unregisterStatusItem")]
async fn unregister_status_item(
    context: HostContext,
    arguments: StatusItemRegistrationId,
) -> Result<(), StatusItemError> {
    let owner = status_owner(&context);
    context.foreground(|_, cx| {
        if cx
            .default_global::<ApplicationStatusBar>()
            .0
            .unregister(owner, arguments.0)
        {
            Ok(())
        } else {
            Err(StatusItemError::RegistrationNotFound)
        }
    })?
}

use crate::app::resource_decorations::{
    ApplicationResourceDecorations, Decoration, DecorationOwner,
};

/// Failures of a resource decoration operation.
#[derive(Debug, HostError)]
pub(crate) enum DecorationError {
    #[error(transparent)]
    Open(OpenError),
    #[error(
        code = "DecorationSourceNotFound",
        message = "DecorationSourceNotFound"
    )]
    SourceNotFound,
}

impl From<AccessError> for DecorationError {
    fn from(error: AccessError) -> Self {
        Self::Open(error.into())
    }
}

impl From<OpenError> for DecorationError {
    fn from(error: OpenError) -> Self {
        Self::Open(error)
    }
}

fn decoration_owner(context: &HostContext) -> DecorationOwner {
    DecorationOwner {
        extension: context.request.extension,
        lifecycle: context.request.lifecycle,
    }
}

#[host_operation("createDecorationSource")]
async fn create_decoration_source(context: HostContext, _: ()) -> Result<u64, DecorationError> {
    let owner = decoration_owner(&context);
    Ok(context.foreground(|_, cx| {
        cx.default_global::<ApplicationResourceDecorations>()
            .0
            .create_source(owner)
    })?)
}

/// The changes one `set` makes to a source: URIs mapped to a decoration, or
/// to null to remove it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DecorationChanges {
    source: u64,
    changes: Vec<(String, Option<Decoration>)>,
}

impl HostArguments for DecorationChanges {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let source = reader.integer(2)?;
        let serde_json::Value::Object(entries) = setting_value(reader.command_arguments(3)?) else {
            return Err(DecodeError::Type("decorations must be an object"));
        };
        let changes = entries
            .into_iter()
            .map(|(uri, value)| {
                let decoration = match value {
                    serde_json::Value::Null => None,
                    value => {
                        let decoration: Decoration = serde_json::from_value(value)
                            .map_err(|_| DecodeError::Type("invalid decoration"))?;
                        if !decoration.is_valid() {
                            return Err(DecodeError::Type("invalid decoration badge"));
                        }
                        Some(decoration)
                    }
                };
                Ok((uri, decoration))
            })
            .collect::<Result<_, DecodeError>>()?;
        Ok(Self { source, changes })
    }
}

#[host_operation("setDecorations")]
async fn set_decorations(
    context: HostContext,
    arguments: DecorationChanges,
) -> Result<(), DecorationError> {
    let owner = decoration_owner(&context);
    let providers = context.foreground(|_, cx| KnotWindow::filesystems(cx))?;
    let mut changes = Vec::with_capacity(arguments.changes.len());
    for (uri, decoration) in arguments.changes {
        let uri = knot_model::files::normalize(&providers, &uri)
            .await
            .map_err(|error| product::operations::resource_error(error.into()))?;
        changes.push((uri, decoration));
    }
    context.foreground(|_, cx| {
        match cx.default_global::<ApplicationResourceDecorations>().0.set(
            owner,
            arguments.source,
            changes,
        ) {
            Some(true) => {
                crate::app::resource_decorations::refresh(cx);
                Ok(())
            }
            Some(false) => Ok(()),
            None => Err(DecorationError::SourceNotFound),
        }
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DecorationSourceId(u64);

impl HostArguments for DecorationSourceId {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self(reader.integer(2)?))
    }
}

#[host_operation("clearDecorations")]
async fn clear_decorations(
    context: HostContext,
    arguments: DecorationSourceId,
) -> Result<(), DecorationError> {
    let owner = decoration_owner(&context);
    context.foreground(|_, cx| {
        match cx
            .default_global::<ApplicationResourceDecorations>()
            .0
            .clear(owner, arguments.0)
        {
            Some(true) => {
                crate::app::resource_decorations::refresh(cx);
                Ok(())
            }
            Some(false) => Ok(()),
            None => Err(DecorationError::SourceNotFound),
        }
    })?
}

#[host_operation("removeDecorationSource")]
async fn remove_decoration_source(
    context: HostContext,
    arguments: DecorationSourceId,
) -> Result<(), DecorationError> {
    let owner = decoration_owner(&context);
    context.foreground(|_, cx| {
        if cx
            .default_global::<ApplicationResourceDecorations>()
            .0
            .remove_source(owner, arguments.0)
        {
            crate::app::resource_decorations::refresh(cx);
            Ok(())
        } else {
            Err(DecorationError::SourceNotFound)
        }
    })?
}

use crate::app::menus::{ApplicationMenus, MenuItem, MenuLocation, MenuOwner};

/// Failures of a menu item operation.
#[derive(Debug, HostError)]
pub(crate) enum MenuItemError {
    #[error(transparent)]
    Access(AccessError),
    #[error(
        code = "MenuItemRegistrationNotFound",
        message = "MenuItemRegistrationNotFound"
    )]
    RegistrationNotFound,
}

impl From<AccessError> for MenuItemError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

fn menu_owner(context: &HostContext) -> MenuOwner {
    MenuOwner {
        extension: context.request.extension,
        lifecycle: context.request.lifecycle,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MenuItemRegistration {
    location: MenuLocation,
    item: MenuItem,
}

impl HostArguments for MenuItemRegistration {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let location = MenuLocation::parse(&reader.text(2, "menu location must be text")?)
            .ok_or(DecodeError::Type("unknown menu location"))?;
        let item: MenuItem = serde_json::from_value(setting_value(reader.command_arguments(3)?))
            .map_err(|_| DecodeError::Type("invalid menu item"))?;
        item.validate(location)
            .map_err(|_| DecodeError::Type("invalid menu item"))?;
        Ok(Self { location, item })
    }
}

#[host_operation("addMenuItem")]
async fn add_menu_item(
    context: HostContext,
    arguments: MenuItemRegistration,
) -> Result<u64, MenuItemError> {
    let owner = menu_owner(&context);
    Ok(context.foreground(|_, cx| {
        cx.default_global::<ApplicationMenus>()
            .0
            .add(owner, arguments.location, arguments.item)
            .expect("the arguments were validated when decoded")
    })?)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MenuItemRegistrationId(u64);

impl HostArguments for MenuItemRegistrationId {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self(reader.integer(2)?))
    }
}

#[host_operation("removeMenuItem")]
async fn remove_menu_item(
    context: HostContext,
    arguments: MenuItemRegistrationId,
) -> Result<(), MenuItemError> {
    let owner = menu_owner(&context);
    context.foreground(|_, cx| {
        if cx
            .default_global::<ApplicationMenus>()
            .0
            .remove(owner, arguments.0)
        {
            Ok(())
        } else {
            Err(MenuItemError::RegistrationNotFound)
        }
    })?
}

#[host_operation("listThemes")]
async fn list_themes(context: HostContext, _: ()) -> Result<Vec<ThemeInfo>, AccessError> {
    context.foreground(|_, cx| {
        cx.try_global::<ApplicationThemes>()
            .map(|themes| themes.0.list())
            .unwrap_or_default()
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThemeName {
    name: String,
}

impl HostArguments for ThemeName {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            name: reader.text(2, "theme name must be text")?,
        })
    }
}

#[host_operation("getTheme")]
async fn get_theme(
    context: HostContext,
    arguments: ThemeName,
) -> Result<Option<ThemeDefinition>, AccessError> {
    context.foreground(|_, cx| {
        cx.try_global::<ApplicationThemes>()
            .and_then(|themes| themes.0.get(&arguments.name).cloned())
    })
}

/// The color a package defines.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DefinedColor(ColorDefinition);

impl HostArguments for DefinedColor {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let key = reader.text(2, "color key must be text")?;
        let description = reader.text(3, "color description must be text")?;
        let mut default = |index| {
            reader
                .text(index, "color default must be text")
                .and_then(|text| {
                    crate::app::themes::color_default(&text).ok_or(DecodeError::Type(
                        "color default must be #rrggbb, #rrggbbaa, or a color key",
                    ))
                })
        };
        let dark = default(4)?;
        let light = default(5)?;
        if key.is_empty() {
            return Err(DecodeError::Type("color key must be nonempty text"));
        }
        Ok(Self(ColorDefinition {
            key,
            description,
            dark,
            light,
        }))
    }
}

#[host_operation("defineColor")]
async fn define_color(context: HostContext, arguments: DefinedColor) -> Result<(), ThemeError> {
    let owner = theme_owner(&context);
    context.foreground(|_, cx| {
        let key = arguments.0.key.clone();
        cx.default_global::<ApplicationThemes>()
            .0
            .define_color(owner, arguments.0)
            .map_err(|_| ThemeError::DuplicateColor { key })
    })?
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use knot_protocol::RequestId;

    #[test]
    fn setting_values_restore_integers_from_javascript_doubles() {
        use knot_protocol::CommandArgumentValue as Js;
        let value = Js::Object(
            [
                ("integer".into(), Js::Number(4.0)),
                ("fraction".into(), Js::Number(4.5)),
                ("unsafe".into(), Js::Number(1e17)),
                ("list".into(), Js::Array(vec![Js::Number(-2.0)])),
            ]
            .into(),
        );
        let value = setting_value(value);
        assert!(value["integer"].is_i64());
        assert!(value["list"][0].is_i64());
        assert!(value["fraction"].is_f64());
        assert!(value["unsafe"].is_f64());
    }

    #[gpui::test]
    async fn immediate_handler_commits_and_rejects_a_disposed_context(cx: &mut TestAppContext) {
        let key = ExtensionKey::new(ExtensionId::new(702), ExtensionLifecycleId::new(1));
        let (host, context) = cx.update(|cx| {
            let commands = cx.new(crate::app::commands::dispatch::ProductCommandDispatcher::new);
            cx.set_global(ApplicationProductCommands(commands));
            let host = install(cx);
            let context = host.update(cx, |host, cx| {
                host.load(key, ExtensionConfig::default()).unwrap();
                HostContext::new(
                    knot_protocol::RequestContext {
                        extension: key.extension,
                        lifecycle: key.lifecycle,
                        id: RequestId::new(1),
                        invocation: None,
                    },
                    cx,
                )
            });
            (host, context)
        });
        write_clipboard(
            context.clone(),
            ClipboardArguments {
                text: "application".into(),
            },
        )
        .await
        .unwrap();
        cx.update(|cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().unwrap(),
                "application"
            );
            host.update(cx, |host, cx| host.unload(key, cx).unwrap());
        });
        let result = write_clipboard(
            context,
            ClipboardArguments {
                text: "late".into(),
            },
        )
        .await;
        assert!(matches!(result, Err(AccessError::Cancelled)));
        cx.update(|cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().unwrap(),
                "application"
            )
        });
    }
    fn storage_context(
        root: &std::path::Path,
        cx: &mut TestAppContext,
    ) -> (Entity<ProductExtensionHost>, HostContext, ExtensionKey) {
        let key = ExtensionKey::new(ExtensionId::new(703), ExtensionLifecycleId::new(1));
        cx.update(|cx| {
            let commands = cx.new(crate::app::commands::dispatch::ProductCommandDispatcher::new);
            cx.set_global(ApplicationProductCommands(commands));
            let host = install(cx);
            let context = host.update(cx, |host, cx| {
                host.load(key, ExtensionConfig::default()).unwrap();
                host.set_test_storage_root(root.to_owned(), cx);
                host.storage_owners.insert(
                    key,
                    crate::app::extensions::storage::StorageOwner::personal_config(),
                );
                HostContext::new(
                    knot_protocol::RequestContext {
                        extension: key.extension,
                        lifecycle: key.lifecycle,
                        id: RequestId::new(1),
                        invocation: None,
                    },
                    cx,
                )
            });
            (host, context, key)
        })
    }

    #[gpui::test]
    async fn retained_captures_survive_unload_and_the_unloaded_lifecycle_loses_access(
        cx: &mut TestAppContext,
    ) {
        use crate::app::{
            documents::{ApplicationDocuments, DocumentCollection},
            model::{Buffer, BufferModel},
            recovery::{ApplicationRecovery, RecoveryService, RecoveryStore, SnapshotKind},
        };
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("recovery.sqlite3");
        let (host, context, key) = storage_context(root.path(), cx);
        let identity = cx.update(|cx| {
            let documents = cx.new(|_| DocumentCollection::new());
            cx.set_global(ApplicationDocuments(documents.clone()));
            cx.set_global(ApplicationRecovery(RecoveryService::new(
                RecoveryStore::open(path.clone(), cx.background_executor().clone()),
            )));
            let model = cx.new(|_| Buffer::new(BufferModel::from_text("kept")));
            documents.update(cx, |documents, cx| {
                let document = documents.create_untitled("Untitled", model, cx);
                documents.get(document).unwrap().identity().to_string()
            })
        });
        let lookup = DocumentLookup { id: identity };
        let capture = recovery_retain(context.clone(), lookup.clone())
            .await
            .unwrap();
        assert!(matches!(
            recovery_retain(
                context.clone(),
                DocumentLookup {
                    id: "missing".into()
                }
            )
            .await,
            Err(RecoveryError::Open(OpenError::DocumentNotFound))
        ));

        host.update(cx, |host, cx| host.unload(key, cx).unwrap());
        let cancelled = |result: Result<_, RecoveryError>| {
            matches!(
                result,
                Err(RecoveryError::Open(OpenError::Access(
                    AccessError::Cancelled
                )))
            )
        };
        assert!(cancelled(
            recovery_list(context.clone(), ()).await.map(|_| ())
        ));
        assert!(cancelled(
            recovery_retain(context.clone(), lookup).await.map(|_| ())
        ));
        assert!(cancelled(
            recovery_release(
                context,
                CaptureLookup {
                    id: capture.id.clone()
                }
            )
            .await
            .map(|_| ())
        ));

        // The capture is durable and owned by the canonical identity, not the lifecycle.
        let reopened = RecoveryStore::open(path, cx.background_executor.clone());
        let stored = reopened
            .snapshot(SnapshotKind::Retained, capture.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.owner.as_deref(), Some("personal-config"));
        assert_eq!(stored.text, "kept");
    }

    #[gpui::test]
    async fn unload_before_storage_starts_cancels_without_creating_a_database(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let (host, context, key) = storage_context(root.path(), cx);
        let pending = context
            .records()
            .unwrap()
            .write("key".into(), serde_json::json!(1));
        host.update(cx, |host, cx| host.unload(key, cx).unwrap());
        assert!(matches!(pending.await, Err(StorageError::Cancelled)));
        assert!(!root.path().join("personal-config.sqlite3").exists());
    }

    #[gpui::test]
    async fn command_cancellation_invalidates_queued_storage(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let (host, mut context, key) = storage_context(root.path(), cx);
        let invocation = host.update(cx, |host, _| {
            let registration = host
                .commands
                .register_command(
                    &context.request,
                    "storage-test".into(),
                    "Storage test".into(),
                )
                .unwrap();
            host.commands
                .enqueue_extension_root(
                    knot_protocol::Command {
                        name: "storage-test".into(),
                        arguments: knot_protocol::CommandArgumentValue::Null,
                    },
                    CommandTarget {
                        registration,
                        extension: key.extension,
                        lifecycle: key.lifecycle,
                    },
                    None,
                )
                .id
        });
        context.request.invocation = Some(invocation);
        let pending = context
            .records()
            .unwrap()
            .write("key".into(), serde_json::json!(1));
        host.update(cx, |host, cx| {
            host.commands.cancel(invocation);
            host.drain_command_events(cx);
        });
        assert!(matches!(pending.await, Err(StorageError::Cancelled)));
        assert!(!root.path().join("personal-config.sqlite3").exists());
    }

    #[gpui::test]
    fn unload_during_storage_transaction_preserves_commit(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let (host, context, key) = storage_context(root.path(), cx);
        host.update(cx, |host, _| {
            host.storage = Ok(crate::app::extensions::storage::ExtensionStorage::new(
                root.path().to_owned(),
                crate::app::extensions::storage::test_executor(),
            ));
        });
        let (started, ready) = std::sync::mpsc::channel();
        let (resume, gate) = std::sync::mpsc::channel();
        let pending = context.records().unwrap().write_with_gate(started, gate);
        ready
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        host.update(cx, |host, cx| host.unload(key, cx).unwrap());
        resume.send(()).unwrap();
        pollster::block_on(pending).unwrap();
        let drain = host.update(cx, |host, _| host.shutdown_storage().unwrap());
        pollster::block_on(drain);
        let reopened = crate::app::extensions::storage::ExtensionStorage::new(
            root.path().to_owned(),
            crate::app::extensions::storage::test_executor(),
        );
        assert_eq!(
            pollster::block_on(
                reopened
                    .for_owner(crate::app::extensions::storage::StorageOwner::personal_config())
                    .read("key".into())
            )
            .unwrap(),
            Some(serde_json::json!(42))
        );
        pollster::block_on(reopened.shutdown());
    }

    #[gpui::test]
    async fn shutdown_rejects_new_host_storage_requests(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let (host, context, _) = storage_context(root.path(), cx);
        let pending = context
            .records()
            .unwrap()
            .write("key".into(), serde_json::json!(1));
        let drain = host.update(cx, |host, _| host.shutdown_storage().unwrap());
        assert!(matches!(
            storage_write(
                context,
                RecordWrite {
                    key: "late".into(),
                    value: serde_json::json!(2)
                }
            )
            .await,
            Err(RecordError::Access(AccessError::Cancelled))
        ));
        pending.await.unwrap();
        drain.await;
        assert!(host.update(cx, |host, _| host.shutdown_storage()).is_none());
    }

    #[gpui::test]
    fn product_quit_drains_storage_without_blocking_foreground(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let (host, context, _) = storage_context(root.path(), cx);
        host.update(cx, |host, _| {
            host.storage = Ok(crate::app::extensions::storage::ExtensionStorage::new(
                root.path().to_owned(),
                crate::app::extensions::storage::test_executor(),
            ));
        });
        let (started, ready) = std::sync::mpsc::channel();
        let (resume, gate) = std::sync::mpsc::channel();
        let records = context.records().unwrap();
        let pending = records.write_with_gate(started, gate);
        ready
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        cx.update(quit_after_storage);
        cx.update(quit_after_storage);
        cx.run_until_parked();
        cx.update(|cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                "foreground remains responsive".into(),
            ))
        });
        assert!(cx.read(|cx| host.read(cx).storage_stopping));
        assert!(matches!(
            pollster::block_on(records.write("late".into(), serde_json::json!(1))),
            Err(StorageError::Cancelled)
        ));
        resume.send(()).unwrap();
        pollster::block_on(pending).unwrap();
        cx.run_until_parked();
    }
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NoticeActionValue {
    title: String,
    command: crate::app::status_bar::StatusCommand,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NoticeValue {
    message: String,
    title: Option<String>,
    severity: String,
    auto_dismiss: Option<bool>,
    actions: Vec<NoticeActionValue>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NoticeArguments {
    notice: NoticeValue,
    target: Option<String>,
}

impl HostArguments for NoticeArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let notice = serde_json::from_value(setting_value(reader.command_arguments(2)?))
            .map_err(|_| DecodeError::Type("invalid notice"))?;
        Ok(Self {
            notice,
            target: reader.optional_text(3, "window target must be text")?,
        })
    }
}

/// Record a notice, which pops up without waiting for the user, and return its
/// ID.
#[host_operation("showNotice")]
pub(crate) async fn show_notice(
    context: HostContext,
    arguments: NoticeArguments,
) -> Result<u64, AccessError> {
    let request = context.request;
    let NoticeArguments { notice, target } = arguments;
    let severity = crate::app::notifications::Severity::parse(&notice.severity)
        .ok_or(AccessError::InvalidTarget)?;
    context.foreground(move |host, cx| {
        if let Some(window) = &target
            && crate::app::product::find_window(window, cx).is_none()
        {
            return Err(AccessError::InvalidTarget);
        }
        let source = host
            .lifecycle_label(ExtensionKey::new(request.extension, request.lifecycle))
            .unwrap_or_else(|| "Knot".to_owned());
        let spec = crate::app::notifications::NoticeSpec {
            message: notice.message,
            title: notice.title,
            severity,
            auto_dismiss: notice.auto_dismiss,
            actions: notice
                .actions
                .into_iter()
                .map(|action| crate::app::notifications::NoticeAction {
                    title: action.title,
                    command: action.command,
                })
                .collect(),
            target,
            source,
        };
        Ok(crate::app::notifications::raise(spec, cx))
    })?
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NoticeIds(Vec<u64>);

impl HostArguments for NoticeIds {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        serde_json::from_value(setting_value(reader.command_arguments(2)?))
            .map(Self)
            .map_err(|_| DecodeError::Type("notice IDs must be integers"))
    }
}

#[host_operation("markNotificationsRead")]
pub(crate) async fn mark_notifications_read(
    context: HostContext,
    arguments: NoticeIds,
) -> Result<(), AccessError> {
    context.foreground(move |_, cx| {
        crate::app::notifications::update(cx, |store| store.mark_read(&arguments.0));
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NoticeId(u64);

impl HostArguments for NoticeId {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self(reader.integer(2)?))
    }
}

#[host_operation("dismissNotification")]
pub(crate) async fn dismiss_notification(
    context: HostContext,
    arguments: NoticeId,
) -> Result<(), AccessError> {
    context.foreground(move |_, cx| {
        crate::app::notifications::update(cx, |store| store.dismiss(arguments.0));
    })
}

#[host_operation("clearNotifications")]
pub(crate) async fn clear_notifications(context: HostContext, (): ()) -> Result<(), AccessError> {
    context.foreground(|_, cx| {
        crate::app::notifications::update(cx, |store| store.clear());
    })
}
