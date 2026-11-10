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
    #[error(code = "ProviderFailure", message = "ProviderFailure")]
    ProviderFailure,
    #[error(code = "BufferClosed", message = "BufferClosed")]
    BufferClosed,
    #[error(code = "DocumentNotFound", message = "DocumentNotFound")]
    DocumentNotFound,
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
}

impl HostArguments for OpenArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            uri: reader.text(2, "resource URI must be text")?,
            target: reader.destination(3)?,
            preview: reader
                .optional_boolean(4, "preview must be a boolean")?
                .unwrap_or(false),
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
                None | Some("dock") => product::operations::TerminalLocation::Dock,
                Some("center") => product::operations::TerminalLocation::Center,
                Some(_) => {
                    return Err(DecodeError::Range(
                        "terminal location must be dock or center",
                    ));
                }
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
    let target = context.foreground(move |_, cx| {
        product::operations::open_terminal(destination, directory, location, cx)
    })??;
    Ok(knot_protocol::PresentationInfo {
        target,
        kind: knot_protocol::SurfaceKind::Terminal,
        document: None,
        buffer: None,
        tool: None,
    })
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
        if host
            .application_observation
            .unsubscribe(arguments.subscription, key)
        {
            Ok(())
        } else {
            Err(AccessError::OwnershipDisposed)
        }
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

pub(crate) use crate::app::extensions::buffers::BufferError;
use knot_protocol::{
    BufferHandle, BufferSubscriptionId, ByteRange, EditorContribution, TextEdit, TextSnapshot,
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
    CompletionProviderRegistrationId, ConfirmationResult, FolderPickerResult, InputCompletion,
    InputResult, InputUpdate, PickerItem, PickerResult, PickerRow, PickerUpdate, ProviderIdentity,
    TreeAction, TreeProviderRegistrationId, ViewContentKind,
};

use crate::app::transient::{PromptText, TransientOutcome, TransientOwner};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TreeArguments {
    pub id: String,
    pub title: String,
    pub content: ViewContentKind,
    pub actions: Vec<TreeAction>,
    pub location: crate::app::workbench::DockLocation,
    pub order: Option<u64>,
    pub visible: bool,
    pub language: Option<String>,
    /// Whether a text frame shows the editor gutter.
    pub gutter: bool,
}
impl HostArguments for TreeArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let location = match reader.text(4, "view placement must be text")?.as_str() {
            "left" => crate::app::workbench::DockLocation::Left,
            "right" => crate::app::workbench::DockLocation::Right,
            "bottom" => crate::app::workbench::DockLocation::Bottom,
            _ => return Err(DecodeError::Type("invalid view placement")),
        };
        let content = match reader.text(7, "view content kind must be text")?.as_str() {
            "tree" => ViewContentKind::Tree,
            "text" => ViewContentKind::Text,
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
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CompletionArguments {
    pub label: String,
}
impl HostArguments for CompletionArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            label: reader.text(2, "UnsupportedOperation")?,
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
pub(crate) struct CompletionRegistrationArguments {
    pub registration: CompletionProviderRegistrationId,
}
impl HostArguments for CompletionRegistrationArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            registration: CompletionProviderRegistrationId::new(reader.integer(2)?),
        })
    }
    fn provider_mutation(&self) -> Option<ProviderIdentity> {
        Some(ProviderIdentity::Completion(self.registration))
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
}
impl HostArguments for InputArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            title: reader.text(2, "input title must be text")?,
            value: reader.optional_text(3, "input value must be text or null")?,
            prompt: reader.optional_text(4, "input prompt must be text or null")?,
            target: reader.optional_text(5, "window target must be text")?,
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MessageArguments {
    pub title: String,
    pub message: Option<String>,
    pub target: Option<String>,
}
impl HostArguments for MessageArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            title: reader.text(2, "message title must be text")?,
            message: reader.optional_text(3, "message body must be text or null")?,
            target: reader.optional_text(4, "window target must be text")?,
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
            move |shell, cx| shell.begin_picker(owner, title, items, None, cx),
            cx,
        )
    })??;
    match receiver.await {
        Ok(TransientOutcome::Picker(id)) => Ok(PickerResult::Selected { id }),
        Ok(TransientOutcome::Dismissed) => Ok(PickerResult::Dismissed),
        _ => Err(TransientError::Cancelled),
    }
}

/// A picker or input whose caller follows its changes: the highlighted item,
/// or the typed value. The caller pulls each change within its own turn,
/// because a separate notification root could not run while the caller awaits
/// the interaction.
pub(crate) struct TransientSession {
    owner: ExtensionKey,
    completion: tokio::sync::oneshot::Receiver<TransientOutcome>,
    changes: tokio::sync::watch::Receiver<Option<String>>,
    /// The input that shows the caller's messages.
    input: Option<WeakEntity<crate::app::transient::InputView>>,
    /// The value the caller last pulled; feedback applies only while the
    /// field still holds it.
    delivered: Option<String>,
    /// The picker whose rows the caller supplies, with the queries it asks
    /// for.
    queried: Option<QueriedPicker>,
}

struct QueriedPicker {
    picker: WeakEntity<crate::app::picker::Picker>,
    queries: tokio::sync::watch::Receiver<Option<String>>,
    /// The query the caller last pulled; rows apply only while the field
    /// still holds it.
    delivered: Option<String>,
}

impl TransientSession {
    pub(crate) fn owner(&self) -> ExtensionKey {
        self.owner
    }
}

/// The next thing a caller pulls from a session.
enum Pulled {
    Changed { session: u64, value: Option<String> },
    Queried { session: u64, query: String },
    Settled(TransientOutcome),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BeginPickerArguments {
    picker: PickerArguments,
    /// Whether the caller supplies the rows for each query.
    queried: bool,
}
impl HostArguments for BeginPickerArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            picker: PickerArguments::decode(reader)?,
            queried: reader.integer(5)? != 0,
        })
    }
}

/// Open a picker and settle with its first highlight or query, or its
/// outcome.
#[host_operation("beginPicker")]
pub(crate) async fn begin_picker(
    context: HostContext,
    arguments: BeginPickerArguments,
) -> Result<PickerUpdate, TransientError> {
    let request = context.request;
    let owner = transient_owner(&request);
    let BeginPickerArguments {
        picker:
            PickerArguments {
                title,
                items,
                target,
            },
        queried,
    } = arguments;
    let (sender, changes) = tokio::sync::watch::channel(None);
    let (query_sender, queries) = tokio::sync::watch::channel(None);
    let (completion, queried) = context.foreground(|host, cx| {
        host.with_transient_window(
            &request,
            target.as_deref(),
            move |shell, cx| {
                if queried {
                    let (completion, picker) =
                        shell.begin_queried_picker(owner, title, sender, query_sender, cx)?;
                    Ok((
                        completion,
                        Some(QueriedPicker {
                            picker,
                            queries,
                            delivered: None,
                        }),
                    ))
                } else {
                    let completion = shell.begin_picker(owner, title, items, Some(sender), cx)?;
                    Ok((completion, None))
                }
            },
            cx,
        )
    })??;
    let session = TransientSession {
        owner: ExtensionKey::new(request.extension, request.lifecycle),
        completion,
        changes,
        input: None,
        delivered: None,
        queried,
    };
    picker_update(next_change(&context, session).await?)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PickerSessionArguments {
    pub session: u64,
    /// The rows answering the last pulled query, if that is what was pulled.
    pub rows: Option<Vec<PickerRow>>,
}
impl HostArguments for PickerSessionArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let rows = serde_json::to_value(reader.command_arguments(3)?)
            .ok()
            .and_then(|value| serde_json::from_value::<Option<Vec<PickerRow>>>(value).ok())
            .ok_or(DecodeError::Type("invalid picker rows"))?;
        Ok(Self {
            session: reader.integer(2)?,
            rows,
        })
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
        ..crate::app::picker::PickerEntry::new(item.label)
    }
}

/// Show the rows the caller derived from the last pulled query, unless the
/// field has changed since, then settle with the picker's next query or
/// highlight, or with its outcome once it settles. Only the latest unread
/// change of each is kept.
#[host_operation("nextPickerEvent")]
pub(crate) async fn next_picker_event(
    context: HostContext,
    arguments: PickerSessionArguments,
) -> Result<PickerUpdate, TransientError> {
    let owner = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    let PickerSessionArguments { session, rows } = arguments;
    let session = context
        .foreground(|host, cx| {
            let session = host.take_transient_session(owner, session)?;
            if let (Some(rows), Some(queried)) = (rows, session.queried.as_ref())
                && let (Some(picker), Some(query)) =
                    (queried.picker.upgrade(), queried.delivered.as_deref())
            {
                let entries = rows.into_iter().map(picker_entry).collect();
                picker.update(cx, |picker, cx| picker.supply(query, entries, cx));
            }
            Some(session)
        })?
        .ok_or(TransientError::Cancelled)?;
    picker_update(next_change(&context, session).await?)
}

fn picker_update(pulled: Pulled) -> Result<PickerUpdate, TransientError> {
    match pulled {
        Pulled::Changed { session, value } => Ok(PickerUpdate::Highlighted { session, id: value }),
        Pulled::Queried { session, query } => Ok(PickerUpdate::Queried { session, query }),
        Pulled::Settled(TransientOutcome::Picker(id)) => Ok(PickerUpdate::Selected { id }),
        Pulled::Settled(TransientOutcome::Dismissed) => Ok(PickerUpdate::Dismissed),
        Pulled::Settled(_) => Err(TransientError::Cancelled),
    }
}

/// Settle with the session's next change, or with its outcome once it settles.
async fn next_change(
    context: &HostContext,
    mut session: TransientSession,
) -> Result<Pulled, TransientError> {
    enum Next {
        Settled(Result<TransientOutcome, tokio::sync::oneshot::error::RecvError>),
        Queried,
        Changed,
        Closed,
    }
    // A settled interaction wins over a change that was never read.
    let next = match session.completion.try_recv() {
        Ok(outcome) => Next::Settled(Ok(outcome)),
        Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
            return Err(TransientError::Cancelled);
        }
        Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
            let completion = &mut session.completion;
            let queries = session.queried.as_mut().map(|queried| &mut queried.queries);
            let mut queried = std::pin::pin!(async {
                match queries {
                    Some(queries) => queries.changed().await,
                    None => std::future::pending().await,
                }
            });
            let mut changed = std::pin::pin!(session.changes.changed());
            std::future::poll_fn(|task| {
                if let std::task::Poll::Ready(outcome) =
                    std::pin::Pin::new(&mut *completion).poll(task)
                {
                    return std::task::Poll::Ready(Next::Settled(outcome));
                }
                // A query is answered before the highlight it will replace.
                if let std::task::Poll::Ready(queried) = queried.as_mut().poll(task) {
                    return std::task::Poll::Ready(match queried {
                        Ok(()) => Next::Queried,
                        Err(_) => Next::Closed,
                    });
                }
                changed.as_mut().poll(task).map(|changed| match changed {
                    Ok(()) => Next::Changed,
                    Err(_) => Next::Closed,
                })
            })
            .await
        }
    };
    let next = match next {
        Next::Closed => Next::Settled((&mut session.completion).await),
        next => next,
    };
    match next {
        Next::Settled(Ok(outcome)) => Ok(Pulled::Settled(outcome)),
        Next::Settled(Err(_)) => Err(TransientError::Cancelled),
        Next::Queried => {
            let queried = session.queried.as_mut().expect("queried picker");
            let query = queried
                .queries
                .borrow_and_update()
                .clone()
                .unwrap_or_default();
            queried.delivered = Some(query.clone());
            let session = context.foreground(|host, _| host.keep_transient_session(session))?;
            Ok(Pulled::Queried { session, query })
        }
        Next::Changed | Next::Closed => {
            let value = session.changes.borrow_and_update().clone();
            session.delivered.clone_from(&value);
            let session = context.foreground(|host, _| host.keep_transient_session(session))?;
            Ok(Pulled::Changed { session, value })
        }
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
    } = arguments;
    let (receiver, _) = context.foreground(|host, cx| {
        host.with_transient_window(
            &request,
            target.as_deref(),
            move |shell, cx| {
                shell.begin_input(owner, title, value.unwrap_or_default(), prompt, None, cx)
            },
            cx,
        )
    })??;
    match receiver.await {
        Ok(TransientOutcome::Input(value)) => Ok(InputResult::Accepted { value }),
        Ok(TransientOutcome::Dismissed) => Ok(InputResult::Dismissed),
        _ => Err(TransientError::Cancelled),
    }
}

/// Open an input and settle with its initial value or its outcome.
#[host_operation("beginInput")]
pub(crate) async fn begin_input(
    context: HostContext,
    arguments: InputArguments,
) -> Result<InputUpdate, TransientError> {
    let request = context.request;
    let owner = transient_owner(&request);
    let InputArguments {
        title,
        value,
        prompt,
        target,
    } = arguments;
    let (sender, changes) = tokio::sync::watch::channel(None);
    let (completion, input) = context.foreground(|host, cx| {
        host.with_transient_window(
            &request,
            target.as_deref(),
            move |shell, cx| {
                shell.begin_input(
                    owner,
                    title,
                    value.unwrap_or_default(),
                    prompt,
                    Some(sender),
                    cx,
                )
            },
            cx,
        )
    })??;
    let session = TransientSession {
        owner: ExtensionKey::new(request.extension, request.lifecycle),
        completion,
        changes,
        input: Some(input),
        delivered: None,
        queried: None,
    };
    input_update(next_change(&context, session).await?)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InputSessionArguments {
    pub session: u64,
    pub message: Option<String>,
    pub items: Vec<InputCompletion>,
}
impl HostArguments for InputSessionArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        let items = serde_json::to_value(reader.command_arguments(4)?)
            .ok()
            .and_then(|value| serde_json::from_value::<Vec<InputCompletion>>(value).ok())
            .ok_or(DecodeError::Type("invalid input completions"))?;
        Ok(Self {
            session: reader.integer(2)?,
            message: reader.optional_text(3, "input message must be text or null")?,
            items,
        })
    }
}

/// Show the message and completions the caller derived from the last value,
/// unless the field has changed since, then settle with
/// the input's next value, or with its outcome once it settles. Only the
/// latest unread value is kept.
#[host_operation("nextInputEvent")]
pub(crate) async fn next_input_event(
    context: HostContext,
    arguments: InputSessionArguments,
) -> Result<InputUpdate, TransientError> {
    let owner = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    let InputSessionArguments {
        session,
        message,
        items,
    } = arguments;
    let session = context
        .foreground(|host, cx| {
            let session = host.take_transient_session(owner, session)?;
            if let Some(input) = session.input.as_ref().and_then(WeakEntity::upgrade) {
                input.update(cx, |input, cx| {
                    if session.delivered.as_deref() == Some(input.text(cx).as_str()) {
                        input.set_feedback(message, items, cx);
                    }
                });
            }
            Some(session)
        })?
        .ok_or(TransientError::Cancelled)?;
    input_update(next_change(&context, session).await?)
}

fn input_update(pulled: Pulled) -> Result<InputUpdate, TransientError> {
    match pulled {
        Pulled::Changed { session, value } => Ok(InputUpdate::Changed {
            session,
            value: value.unwrap_or_default(),
        }),
        Pulled::Queried { .. } => Err(TransientError::Cancelled),
        Pulled::Settled(TransientOutcome::Input(value)) => Ok(InputUpdate::Accepted { value }),
        Pulled::Settled(TransientOutcome::Dismissed) => Ok(InputUpdate::Dismissed),
        Pulled::Settled(_) => Err(TransientError::Cancelled),
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
            move |shell, cx| {
                shell.begin_confirmation(
                    owner,
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

#[host_operation("showMessage")]
pub(crate) async fn show_message(
    context: HostContext,
    arguments: MessageArguments,
) -> Result<(), TransientError> {
    let request = context.request;
    let owner = transient_owner(&request);
    let MessageArguments {
        title,
        message,
        target,
    } = arguments;
    let receiver = context.foreground(|host, cx| {
        host.with_transient_window(
            &request,
            target.as_deref(),
            move |shell, cx| shell.begin_message(owner, PromptText { title, message }, cx),
            cx,
        )
    })??;
    match receiver.await {
        Ok(_) => Ok(()),
        Err(_) => Err(TransientError::Cancelled),
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
            move |shell, cx| shell.begin_folder_picker(Some(owner), cx),
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
    let providers = context.foreground(|_, cx| ProductShell::filesystems(cx))?;
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
            ProductShell::filesystems(cx),
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
            ProductShell::filesystems(cx),
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
            ProductShell::filesystems(cx),
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
}

impl HostArguments for WatchFilesArguments {
    fn decode(reader: &mut dyn ArgumentReader) -> Result<Self, DecodeError> {
        Ok(Self {
            uri: reader.text(2, "resource URI must be text")?,
            recursive: reader
                .optional_boolean(3, "watch recursive must be a boolean")?
                .unwrap_or(false),
        })
    }
}

#[host_operation("watchFiles")]
async fn watch_files(
    context: HostContext,
    arguments: WatchFilesArguments,
) -> Result<knot_protocol::WatchSubscriptionId, OpenError> {
    let providers = context.foreground(|_, cx| ProductShell::filesystems(cx))?;
    let uri = knot_model::files::normalize(&providers, &arguments.uri)
        .await
        .map_err(|error| product::operations::resource_error(error.into()))?;
    let key = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context
        .foreground(|host, cx| host.watch_files(uri, arguments.recursive, key, &providers, cx))?
        .map_err(|error| product::operations::resource_error(error.into()))
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
        if host.unwatch_files(arguments.subscription, key) {
            Ok(())
        } else {
            Err(AccessError::OwnershipDisposed)
        }
    })?
}

#[host_operation("statFile")]
async fn stat_file(
    context: HostContext,
    arguments: FileUriArguments,
) -> Result<Option<knot_protocol::FileKind>, OpenError> {
    let providers = context.foreground(|_, cx| ProductShell::filesystems(cx))?;
    knot_model::files::stat(providers, &arguments.uri)
        .await
        .map(|kind| kind.map(file_kind))
        .map_err(|error| product::operations::resource_error(error.into()))
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

#[host_operation("unregisterView")]
pub(crate) async fn unregister_tree_provider(
    context: HostContext,
    arguments: TreeRegistrationArguments,
) -> Result<(), SemanticError> {
    let request = context.request;
    context.foreground(|host, cx| host.unregister_view(&request, arguments, cx))?
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

#[host_operation("registerCompletionProvider")]
pub(crate) async fn register_completion_provider(
    context: HostContext,
    arguments: CompletionArguments,
) -> Result<CompletionProviderRegistrationId, SemanticError> {
    let request = context.request;
    context.foreground(|host, cx| {
        host.semantics
            .register_completion_provider(&request, arguments, cx)
    })?
}

#[host_operation("unregisterCompletionProvider")]
pub(crate) async fn unregister_completion_provider(
    context: HostContext,
    arguments: CompletionRegistrationArguments,
) -> Result<(), SemanticError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let registration = arguments.registration;
        host.semantics
            .unregister_completion_provider(&request, arguments, cx)?;
        host.cancel_completion_provider(registration, cx);
        Ok(())
    })?
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
    let request = context.request;
    context.foreground(|host, _| {
        host.commands
            .complete_inline(&request, arguments.invocation, arguments.outcome)
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

/// Bind a long-lived editor handle to the document tab a view target names,
/// returning its authority.
#[host_operation("openEditor")]
pub(crate) async fn open_editor(
    context: HostContext,
    arguments: ViewTargetArguments,
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

/// A selection notification's listener turn began; see `SelectionWatch`.
#[host_operation("acknowledgeEditorSelection")]
pub(crate) async fn acknowledge_editor_selection(
    context: HostContext,
    arguments: EditorArguments,
) -> Result<(), AccessError> {
    let owner = ExtensionKey::new(context.request.extension, context.request.lifecycle);
    context.foreground(|host, _| host.acknowledge_editor_selection(owner, arguments.authority))
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

#[derive(Debug, HostError)]
pub(crate) enum EditorError {
    #[error(transparent)]
    Access(AccessError),
    #[error(code = "InvalidRange", message = "InvalidRange")]
    InvalidRange,
    #[error(code = "OperationInProgress", message = "OperationInProgress")]
    OperationInProgress,
}
impl From<AccessError> for EditorError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

/// A selection's ends as UTF-8 byte offsets; equal for a bare caret.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub(crate) struct EditorSelection {
    pub anchor: usize,
    pub caret: usize,
}
impl HostSuccess for EditorSelection {
    fn encode(self) -> Result<EncodedValue, DecodeError> {
        encode_json(self)
    }
}

#[host_operation("editorSelection")]
pub(crate) async fn editor_selection(
    context: HostContext,
    arguments: EditorArguments,
) -> Result<EditorSelection, AccessError> {
    let request = context.request;
    context.foreground(|host, cx| {
        let editor = host.contextual_editor(&request, arguments.authority, cx)?;
        let (anchor, caret) = editor.read(cx).selection(cx);
        Ok(EditorSelection { anchor, caret })
    })?
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
    // The window's shell observes the host, so it opens after this host update.
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
        if host.unobserve_setting(arguments.subscription, key) {
            Ok(())
        } else {
            Err(AccessError::OwnershipDisposed)
        }
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
