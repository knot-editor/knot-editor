//! Typed messages exchanged between extension runtimes and the editor host.
//!
//! These types intentionally carry only Knot concepts. They form the transport
//! boundary that extension isolates use to ask the foreground editor to access
//! native state; neither side exposes V8, gpui, or Rust editor-model objects.

extern crate self as knot_protocol;

pub mod declarations;

use std::{borrow::Borrow, collections::BTreeMap, fmt, sync::Arc};

/// An extension-owned identity, opaque outside the host boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExtensionId(u64);

impl ExtensionId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

/// A request identity, unique within an extension's lifetime.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RequestId(u64);

impl RequestId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

/// An identity for one loaded extension lifetime.
///
/// Reusing an [`ExtensionId`] for a later load does not reuse this value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExtensionLifecycleId(u64);

impl ExtensionLifecycleId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

/// Identifies one load of an extension.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExtensionKey {
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
}

impl ExtensionKey {
    pub const fn new(extension: ExtensionId, lifecycle: ExtensionLifecycleId) -> Self {
        Self {
            extension,
            lifecycle,
        }
    }
}

/// An opaque identity for one extension command registration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CommandRegistrationId(u64);

impl CommandRegistrationId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

/// A Knot-owned identity for one live view instance.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ViewId(u64);

impl ViewId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

/// An opaque identity for one command invocation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CommandInvocationId(u64);

impl CommandInvocationId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

/// An opaque identity for one buffer-change subscription.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize)]
pub struct BufferSubscriptionId(u64);

impl BufferSubscriptionId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

/// An opaque identity for one tree data-provider registration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize)]
pub struct TreeProviderRegistrationId(u64);

impl TreeProviderRegistrationId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

/// An opaque identity for one completion-provider registration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CompletionProviderRegistrationId(u64);

impl CompletionProviderRegistrationId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

/// An opaque reference to an editor buffer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize)]
pub struct BufferHandle(u64);

impl BufferHandle {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

/// The stable, globally unique name of a command definition.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
#[serde(transparent)]
pub struct CommandName(String);

impl AsRef<str> for CommandName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for CommandName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommandName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<String> for CommandName {
    fn from(name: String) -> Self {
        Self(name)
    }
}

impl From<&str> for CommandName {
    fn from(name: &str) -> Self {
        Self(name.into())
    }
}

/// One JSON-compatible value carried as an explicit command argument.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum CommandArgumentValue {
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Array(Vec<Self>),
    Object(BTreeMap<String, Self>),
}

fn no_arguments() -> CommandArgumentValue {
    CommandArgumentValue::Null
}

/// A semantic command independent of its invocation source and native adapter.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Command {
    pub name: CommandName,
    pub arguments: CommandArgumentValue,
}

/// One ordered keymap change captured from a JavaScript turn.
#[derive(Clone, Debug, PartialEq)]
pub enum KeybindingMutation {
    Set {
        key: String,
        view: Option<String>,
        context: Option<String>,
        command: Option<Command>,
    },
    Remove {
        key: String,
        view: Option<String>,
        context: Option<String>,
    },
    /// Route committed text in matching views to a command, or mask a lower
    /// text binding with `None`.
    SetText {
        view: Option<String>,
        context: Option<String>,
        command: Option<CommandName>,
    },
    RemoveText {
        view: Option<String>,
        context: Option<String>,
    },
    Invalid(String),
}

/// Ordered native keymap mutations admitted after one JavaScript turn.
#[derive(Clone, Debug, PartialEq)]
pub struct KeybindingArguments {
    pub mutations: Vec<KeybindingMutation>,
}

impl crate::declarations::HostArguments for KeybindingArguments {
    fn decode(
        _: &mut dyn crate::declarations::ArgumentReader,
    ) -> Result<Self, crate::declarations::DecodeError> {
        Err(crate::declarations::DecodeError::Type(
            "keybinding batches require native turn admission",
        ))
    }
}

/// The stable result of attempting to invoke a command.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CommandOutcome {
    Completed,
    Unavailable,
    InvalidTarget,
    InvalidArgument { message: String },
    Cancelled,
    HandlerFailure { message: String },
}

/// The private routing result for a command requested by JavaScript.
///
/// Same-runtime commands are returned to the requesting isolate for a nested
/// handler call. All other routes settle through the dispatcher before the
/// host responds.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CommandInvokeDispatch {
    Outcome {
        outcome: CommandOutcome,
    },
    Inline {
        invocation: CommandInvocationId,
        registration: CommandRegistrationId,
    },
}

/// A half-open range in UTF-8 byte offsets.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ByteRange {
    pub start_byte_offset: usize,
    pub end_byte_offset: usize,
}

/// One replacement expressed in pre-commit buffer coordinates.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub range: ByteRange,
    pub text: String,
}

/// One semantic editor contribution supplied by an extension. The
/// decoration and gutter marker each name a theme color key.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorContribution {
    pub range: ByteRange,
    pub decoration: Option<Decoration>,
    pub gutter: Option<String>,
    pub command: Option<String>,
}

/// How a contribution paints its range, in the color of a theme key.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Decoration {
    pub color: String,
    pub style: DecorationStyle,
}

impl Decoration {
    pub fn new(color: &str, style: DecorationStyle) -> Self {
        Self {
            color: color.to_owned(),
            style,
        }
    }
}

/// What a decoration draws: a background behind its text, a band across its
/// whole lines, a straight underline, or a wavy one.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DecorationStyle {
    Background,
    Line,
    Underline,
    Wavy,
}

/// Whether a tree item can be expanded.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TreeCollapsibleState {
    #[default]
    None,
    Collapsed,
    Expanded,
}

/// The icon a surface paints. A name is an icon name; `file` and `folder` name
/// a file or folder, which the icon associations resolve at paint time.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum IconRef {
    Name(String),
    File { file: String },
    Folder { folder: String },
}

impl From<&str> for IconRef {
    fn from(name: &str) -> Self {
        Self::Name(name.to_owned())
    }
}

/// One semantic command-backed action attached to a row, group, or frame.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeAction {
    pub label: String,
    pub command: String,
    pub arguments: CommandArgumentValue,
    /// The action then renders as an icon button.
    #[serde(default)]
    pub icon: Option<IconRef>,
    /// Offer the action only in the context menu.
    #[serde(default)]
    pub menu_only: bool,
}

/// Whether a tree row is ordinary selectable content or a group header.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TreeItemKind {
    #[default]
    Item,
    Group,
}

/// Renderer-neutral semantics for one extension-provided tree item.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeItem {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    /// A muted second line under the label; a view where any row has one
    /// shows every row at two-line height.
    #[serde(default)]
    pub detail: Option<String>,
    /// An unknown name paints the fallback icon.
    #[serde(default)]
    pub icon: Option<IconRef>,
    #[serde(default)]
    pub collapsible_state: TreeCollapsibleState,
    #[serde(default)]
    pub command: Option<String>,
    /// Passed to the row's command with the gesture that activated it.
    #[serde(default = "no_arguments")]
    pub arguments: CommandArgumentValue,
    #[serde(default)]
    pub kind: TreeItemKind,
    /// Paint the row dimmed, as for an ignored file.
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub status_text: Option<String>,
    /// The theme color key of the row's badge.
    #[serde(default)]
    pub badge: Option<String>,
    #[serde(default)]
    pub actions: Vec<TreeAction>,
}

/// Native content a contributed view definition provides in each window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewContentKind {
    Tree,
    Text,
}

/// One selectable row offered to an extension picker.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickerItem {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub icon: Option<IconRef>,
}

/// One row a query-driven picker is given for a query: an item with the
/// UTF-16 code unit ranges of its label and description to highlight.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickerRow {
    #[serde(flatten)]
    pub item: PickerItem,
    #[serde(default, deserialize_with = "unit_ranges")]
    pub label_matches: Option<Vec<[usize; 2]>>,
    #[serde(default, deserialize_with = "unit_ranges")]
    pub description_matches: Option<Vec<[usize; 2]>>,
}

/// Settled outcome of a picker interaction.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PickerResult {
    Selected { id: String },
    Dismissed,
}

/// One step of a picker whose caller previews the highlighted item or answers
/// its query: the highlight or query with the session to ask again, or the
/// settled outcome.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PickerUpdate {
    Highlighted { session: u64, id: Option<String> },
    Queried { session: u64, query: String },
    Selected { id: String },
    Dismissed,
}

/// One completion offered under a text input: accepting it replaces the
/// whole field with `value`.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputCompletion {
    pub label: String,
    pub value: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub icon: Option<IconRef>,
    /// UTF-16 code unit ranges of the label to highlight as matched.
    #[serde(default, deserialize_with = "unit_ranges")]
    pub matches: Option<Vec<[usize; 2]>>,
}

/// Ranges of whole numbers, which cross the host boundary as floating point.
fn unit_ranges<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<[usize; 2]>>, D::Error> {
    use serde::Deserialize as _;
    let ranges = Option::<Vec<[f64; 2]>>::deserialize(deserializer)?;
    ranges
        .map(|ranges| {
            ranges
                .into_iter()
                .map(|range| {
                    let unit = |value: f64| {
                        (value >= 0. && value.fract() == 0. && value < 9_007_199_254_740_992.)
                            .then_some(value as usize)
                            .ok_or_else(|| serde::de::Error::custom("expected a text offset"))
                    };
                    Ok([unit(range[0])?, unit(range[1])?])
                })
                .collect()
        })
        .transpose()
}

/// One candidate that matched a query: its index in the candidates passed and
/// the UTF-16 code unit ranges of the candidate that matched.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchedCandidate {
    pub index: usize,
    pub ranges: Vec<[usize; 2]>,
}

/// The next change of a followed text input, or its outcome.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InputUpdate {
    Changed { session: u64, value: String },
    Accepted { value: String },
    Dismissed,
}

/// Settled outcome of a text input interaction.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InputResult {
    Accepted { value: String },
    Dismissed,
}

/// Settled outcome of a confirmation interaction.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ConfirmationResult {
    Confirmed,
    Dismissed,
}

/// Settled outcome of a native folder picker.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FolderPickerResult {
    Selected { uri: String },
    Dismissed,
}

/// The kind of a file API resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    File,
    Directory,
}

/// One entry of a `files.list` result.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct FileEntry {
    pub uri: String,
    pub name: String,
    pub kind: FileKind,
    /// Whether the ignore policy ignores the entry.
    pub ignored: bool,
}

/// One file a `files.search` matched.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct FileSearchHit {
    pub uri: String,
    /// The searched directory the file was found in.
    pub directory: String,
    /// The path below `directory`, with `/` separators.
    pub path: String,
    pub name: String,
    /// The UTF-16 ranges of `path` that matched.
    pub ranges: Vec<[usize; 2]>,
}

/// The result of a `files.search`.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct FileSearchResult {
    pub results: Vec<FileSearchHit>,
    /// Whether more files matched than the limit returned.
    pub more: bool,
}

/// One asynchronous child request issued by a native view instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeChildrenRequest {
    pub registration: TreeProviderRegistrationId,
    pub window: String,
    pub view: ViewId,
    pub parent_id: Option<String>,
    pub generation: u64,
}

/// A recoverable tree-provider callback failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeProviderError {
    pub message: String,
}

/// One extension callback result returned to a native view instance.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeChildrenResponse {
    pub registration: TreeProviderRegistrationId,
    pub window: String,
    pub view: ViewId,
    pub parent_id: Option<String>,
    pub generation: u64,
    pub result: Result<Vec<TreeItem>, TreeProviderError>,
}

/// One asynchronous completion request issued by an editor view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionRequest {
    pub registration: CompletionProviderRegistrationId,
    pub buffer: BufferHandle,
    pub revision: u64,
    pub cursor_byte_offset: usize,
    pub prefix: String,
    pub generation: u64,
}

/// One semantic completion candidate returned by an extension provider.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionResultItem {
    pub label: String,
    pub insert_text: String,
}

/// A recoverable completion-provider callback failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionProviderError {
    pub message: String,
}

/// One extension callback result returned to an editor completion session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionResponse {
    pub registration: CompletionProviderRegistrationId,
    pub revision: u64,
    pub generation: u64,
    pub result: Result<Vec<CompletionResultItem>, CompletionProviderError>,
}

/// Discoverable live product targets, separate from command invocation authority.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowTarget {
    pub window: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneTarget {
    pub window: WindowTarget,
    pub workbench: String,
    pub pane: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationTarget {
    pub pane: PaneTarget,
    pub tab: String,
    pub presentation: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentKind {
    Untitled,
    Destination,
    Persisted,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInfo {
    pub id: String,
    pub title: String,
    pub kind: DocumentKind,
    pub resource_uri: Option<String>,
    pub dirty: bool,
    pub revision: u64,
    pub buffer: BufferHandle,
    /// The document's registered language ID, explicit or detected, or
    /// `plaintext`.
    pub language: String,
    /// The indentation the document's settings resolve to.
    pub indentation: IndentationInfo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndentationStyle {
    Spaces,
    Tabs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndentationInfo {
    pub style: IndentationStyle,
    pub size: usize,
}

/// A registered language as extensions see it.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageInfo {
    pub id: String,
    pub name: String,
    pub grammar: String,
    pub file_extensions: Vec<String>,
    pub file_names: Vec<String>,
    pub interpreters: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SurfaceKind {
    Document,
    Terminal,
    Tool,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationInfo {
    pub target: PresentationTarget,
    pub kind: SurfaceKind,
    pub document: Option<String>,
    pub buffer: Option<BufferHandle>,
    /// Stable contribution ID for a contributed tool surface.
    pub tool: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneInfo {
    pub target: PaneTarget,
    pub tabs: Vec<PresentationInfo>,
    pub selected: PresentationTarget,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRootInfo {
    pub id: String,
    pub uri: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    pub window: WindowTarget,
    pub roots: Vec<WorkspaceRootInfo>,
    pub generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub target: WindowTarget,
    pub panes: Vec<PaneInfo>,
    pub current_target: Option<PresentationTarget>,
    /// The editor tab the status bar describes: the focused editor, else the
    /// last focused editor while its tab is shown, and none while a terminal
    /// has focus.
    pub status_target: Option<PresentationTarget>,
    pub workspace: WorkspaceInfo,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationSnapshot {
    pub documents: Vec<DocumentInfo>,
    pub windows: Vec<WindowInfo>,
    pub active_window: Option<WindowTarget>,
    pub active_target: Option<PresentationTarget>,
    /// Whether startup has committed its restored and requested windows.
    pub launched: bool,
}

/// Destination captured when a resource operation is admitted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocumentDestination {
    Window(WindowTarget),
    Pane(PaneTarget),
    View(PresentationTarget),
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPresentation {
    pub document: DocumentInfo,
    pub presentation: PresentationInfo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PresentationCloseOutcome {
    Closed,
    Cancelled,
}

/// One lifecycle-owned application observer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize)]
pub struct WorkbenchSubscriptionId(u64);
impl WorkbenchSubscriptionId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
    pub fn value(self) -> u64 {
        self.0
    }
}

/// One lifecycle-owned observer of a setting key.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize)]
pub struct SettingsSubscriptionId(u64);
impl SettingsSubscriptionId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
    pub fn value(self) -> u64 {
        self.0
    }
}

/// One lifecycle-owned file watch.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize)]
pub struct WatchSubscriptionId(u64);
impl WatchSubscriptionId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
    pub fn value(self) -> u64 {
        self.0
    }
}

/// A payload-free notification about a long-lived editor handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditorEvent {
    /// The handle's tab presentation is gone.
    Closed,
    /// The editor's selection changed since the last acknowledged notification.
    SelectionChanged,
}
impl EditorEvent {
    pub fn name(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::SelectionChanged => "selection",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationEvent {
    pub sequence: u64,
    #[serde(flatten)]
    pub change: ApplicationChange,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ApplicationChange {
    DocumentOpened {
        document: DocumentInfo,
    },
    DocumentChanged {
        document: DocumentInfo,
    },
    DocumentClosed {
        document: String,
    },
    WindowOpened {
        window: WindowInfo,
    },
    WindowClosed {
        window: WindowTarget,
    },
    PresentationsChanged {
        window: WindowInfo,
    },
    WorkspaceChanged {
        workspace: WorkspaceInfo,
    },
    ActiveTargetChanged {
        active_window: Option<WindowTarget>,
        active_target: Option<PresentationTarget>,
    },
    /// The window's captured layout changed after its view-state debounce.
    ViewStateChanged {
        window: WindowTarget,
    },
    /// Startup committed its restored and requested windows; sent once.
    Launched,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchObservation {
    pub subscription: WorkbenchSubscriptionId,
    pub snapshot: ApplicationSnapshot,
    pub watermark: u64,
}

/// Owned identity of a provider whose queued mutations precede disposal acknowledgement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderIdentity {
    Tree(TreeProviderRegistrationId),
    Completion(CompletionProviderRegistrationId),
}

/// Authority metadata shared by foreground capability implementations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestContext {
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
    pub id: RequestId,
    pub invocation: Option<CommandInvocationId>,
}

impl HostRequest {
    pub fn context(&self) -> RequestContext {
        RequestContext {
            extension: self.extension,
            lifecycle: self.lifecycle,
            id: self.id,
            invocation: self.invocation,
        }
    }
}

/// A request sent by one extension runtime to the editor foreground owner.
#[derive(Clone, Debug, PartialEq)]
pub struct HostRequest {
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
    pub id: RequestId,
    pub invocation: Option<CommandInvocationId>,
    pub operation: crate::declarations::DeclaredRequest,
}

/// A reply sent back to the extension runtime for one [`HostRequest`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostResponse {
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
    pub id: RequestId,
    pub result: Result<crate::declarations::DeclaredResponse, crate::declarations::HostFailure>,
}

/// Opaque presentation authority shared by one admitted command tree.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct CommandCapture {
    pub authority: CommandInvocationId,
    pub window: String,
    pub view: Option<String>,
    /// Stable contribution ID for a captured contributed tool, when any.
    pub tool: Option<String>,
    /// Whether the captured presentation is a document editor.
    pub editor: bool,
}

/// A foreground-authorized command invocation routed to its extension owner.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandInvocation {
    pub id: CommandInvocationId,
    pub registration: CommandRegistrationId,
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
    pub arguments: CommandArgumentValue,
    pub capture: Option<CommandCapture>,
}

/// Immutable UTF-16 snapshot storage shared across isolates and externalized by V8.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotText(pub Arc<[u16]>);

impl SnapshotText {
    pub fn from_utf8(text: &str) -> Self {
        Self(text.encode_utf16().collect::<Vec<_>>().into())
    }

    pub fn to_utf8(&self) -> String {
        String::from_utf16(&self.0).expect("snapshot UTF-16 is well formed")
    }
}

/// A snapshot of one requested UTF-8 byte range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextSnapshot {
    pub text: SnapshotText,
    pub range: ByteRange,
    pub revision: u64,
}

/// The matches of a buffer search and the revision it searched.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindResult {
    pub revision: u64,
    pub matches: Vec<ByteRange>,
    pub more: bool,
}

/// One committed editor-visible buffer change in pre-commit coordinates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BufferChange {
    pub buffer: BufferHandle,
    pub before_revision: u64,
    pub revision: u64,
    pub edits: Vec<TextEdit>,
}

#[cfg(test)]
mod tests {
    use super::{
        ByteRange, Command, CommandArgumentValue, CommandInvocationId, CommandInvokeDispatch,
        CommandOutcome, CommandRegistrationId, TextEdit,
    };

    #[test]
    fn command_round_trips_json_compatible_arguments() {
        let command = Command {
            name: "editor.insert".into(),
            arguments: CommandArgumentValue::Object(
                [
                    ("enabled".into(), CommandArgumentValue::Boolean(true)),
                    (
                        "options".into(),
                        CommandArgumentValue::Array(vec![
                            CommandArgumentValue::Null,
                            CommandArgumentValue::Number(3.5),
                        ]),
                    ),
                    ("text".into(), CommandArgumentValue::String("λ".into())),
                ]
                .into(),
            ),
        };

        let json = serde_json::to_string(&command).unwrap();
        assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&json).unwrap(),
            serde_json::json!({
                "name": "editor.insert",
                "arguments": {
                    "enabled": true,
                    "options": [null, 3.5],
                    "text": "λ"
                }
            })
        );
    }

    #[test]
    fn command_routing_and_outcomes_keep_their_wire_contract() {
        let dispatch = CommandInvokeDispatch::Inline {
            invocation: CommandInvocationId::new(13),
            registration: CommandRegistrationId::new(21),
        };
        assert_eq!(
            serde_json::to_value(dispatch).unwrap(),
            serde_json::json!({
                "kind": "inline",
                "invocation": 13,
                "registration": 21,
            })
        );
        assert_eq!(
            serde_json::to_value(CommandOutcome::InvalidArgument {
                message: "expected string".into(),
            })
            .unwrap(),
            serde_json::json!({
                "kind": "invalidArgument",
                "message": "expected string",
            })
        );
    }

    #[test]
    fn edits_and_host_errors_keep_their_wire_contract() {
        let edit = TextEdit {
            range: ByteRange {
                start_byte_offset: 2,
                end_byte_offset: 5,
            },
            text: "λ".into(),
        };
        assert_eq!(
            serde_json::to_value(edit).unwrap(),
            serde_json::json!({
                "range": { "startByteOffset": 2, "endByteOffset": 5 },
                "text": "λ",
            })
        );
    }
}
