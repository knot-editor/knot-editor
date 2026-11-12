//! One editor's state over a shared buffer: selection, scrolling, wrapping,
//! folding, completion, and the native editing and movement commands. Caret
//! and selection use byte positions backed by buffer anchors; movement steps by
//! graphemes and, across rows, keeps a display column in monospace cells. An
//! editor keeps no copy of the text: each operation takes the buffer it reads
//! or edits, and converts positions through the buffer's line index.
//!
//! The editor follows the [`EditorSettings`] its owner resolves for it, and
//! queues what it cannot do itself, such as clipboard and settings writes, as
//! [`EditorEffect`]s for its owner to run.

pub mod bars;
mod brackets;
pub mod commands;
mod comments;
mod completion;
mod decorations;
mod editing;
mod folding;
mod gutter;
mod indentation;
mod input;
mod lines;
mod rows;
mod save_hygiene;
mod text;
mod wrap;

pub use decorations::ViewDecoration;
pub use input::unit_range;
pub use rows::{DecorationFill, Row, decoration_fills};
pub use text::*;
pub use wrap::{WrapWidth, cells_before, index_at_cells, index_at_or_before_cells};

use std::{
    cell::RefCell,
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use knot_core::anchored_range::AnchoredRangeId;
use knot_protocol::{
    BufferHandle, ByteRange, CompletionProviderRegistrationId, CompletionRequest,
    CompletionResponse, DecorationStyle, TextEdit,
};

use crate::{
    buffer::{BufferModel, ContributionSource, ResolvedEditorContribution, decoration_precedence},
    completion::{CompletionController, CompletionProviderRegistration, CompletionSurface},
    history::{EditGroupKind, EditGrouping, HistoryViewChange, HistoryViewState},
    indentation::{IndentUnit, leading_whitespace},
};

/// The settings an editor's commands follow, resolved by its owner for the
/// editor's view kind, language, and document.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorSettings {
    /// Columns per tab stop.
    pub tab_size: usize,
    /// Whether indentation inserts spaces rather than tabs.
    pub insert_spaces: bool,
    /// Whether typing an opener inserts its closer.
    pub auto_closing_brackets: bool,
    /// Whether typing an opener over a selection wraps it.
    pub auto_surround: bool,
    /// Whether long lines wrap.
    pub soft_wrap: bool,
    /// Whether a save trims trailing whitespace.
    pub trim_trailing_whitespace: bool,
    /// Whether a save ends a text with a line terminator.
    pub insert_final_newline: bool,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            tab_size: 4,
            insert_spaces: true,
            auto_closing_brackets: true,
            auto_surround: true,
            soft_wrap: false,
            trim_trailing_whitespace: true,
            insert_final_newline: true,
        }
    }
}

/// Work an editor operation leaves to the editor's owner.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorEffect {
    /// Put `text` on the clipboard.
    WriteClipboard(String),
    /// Give `document` its own `editor.softWrap` value.
    SetDocumentSoftWrap { document: uuid::Uuid, wrap: bool },
    /// Give `document` its own `editor.insertSpaces` value.
    SetDocumentInsertSpaces { document: uuid::Uuid, spaces: bool },
    /// Drop `document`'s own indentation values and detect them again from
    /// its text.
    DetectIndentation { document: uuid::Uuid },
}

static NEXT_HISTORY_CONTEXT: AtomicU64 = AtomicU64::new(1);

/// One contributed decoration segment, painted in `style` and the color of
/// `key`. `start` and `end` are byte columns within `line`; a `line`
/// decoration covers its whole line and leaves them at 0.
#[derive(Clone)]
pub struct RenderedDecoration {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub key: String,
    pub style: DecorationStyle,
}

/// The marker the gutter shows on `line`: the most severe of the line's
/// markers, with the command a click runs.
#[derive(Clone)]
pub struct RenderedGutterMarker {
    pub line: usize,
    pub key: String,
    pub command: Option<String>,
    pub source: ContributionSource,
    pub range: ByteRange,
}

#[derive(Clone)]
pub struct RenderedContributionAction {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub command: String,
    pub source: ContributionSource,
    pub range: ByteRange,
}

/// The units a drag selects: characters after a click, words after a
/// double-click, lines after a triple-click.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionUnit {
    Character,
    Word,
    Line,
}

/// The unit a drag started on: a caret position for characters, or the
/// clicked word or line, which stays selected whichever way the drag goes.
#[derive(Clone, Copy, Debug)]
pub struct DragAnchor {
    pub unit: SelectionUnit,
    pub start: (usize, usize),
    pub end: (usize, usize),
}

/// Restorable editor presentation state. Offsets are absolute UTF-8 byte
/// positions in the shared buffer; `top` is the start of the first visible row.
/// `folds` are the collapsed folds as byte ranges from the start of each
/// fold's first line to the end of its last.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorViewState {
    pub anchor: usize,
    pub caret: usize,
    pub top: usize,
    pub scroll_x: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folds: Vec<Range<usize>>,
}

impl knot_protocol::declarations::HostSuccess for EditorViewState {
    fn encode(
        self,
    ) -> Result<knot_protocol::declarations::EncodedValue, knot_protocol::declarations::DecodeError>
    {
        knot_protocol::declarations::encode_json(self)
    }
}

/// How a selection change scrolls to show the caret.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reveal {
    /// Scroll the minimum, as ordinary caret movement does.
    Nearest,
    /// Center the caret's row when it is outside the viewport.
    Center,
    /// Center the caret's row even when it is inside the viewport.
    AlwaysCenter,
    /// Put the caret's line at the top of the viewport, as far as the text
    /// allows.
    Top,
    /// Put the caret's line at the bottom of the viewport, as far as the text
    /// allows.
    Bottom,
}

/// How the caret is drawn.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CaretShape {
    /// A thin vertical bar before the character.
    #[default]
    Bar,
    /// A box over the character under the caret.
    Block,
    /// A line under the character under the caret.
    Underline,
}

/// One editor's state over a shared buffer: selection, scrolling and the
/// viewport in rows, key contexts, the caret shape, completion, and the
/// native editing and movement commands. It needs no window: the view that
/// renders it handles pointer and platform text input and reports the
/// viewport's size and wrap width as it lays out.
pub struct EditorState {
    /// The `view` fact settings resolve for: `editor` for documents, or the
    /// contributed view ID of the frame that embeds this editor.
    view_kind: Arc<str>,
    /// The settings commands follow, as the owner last resolved them.
    settings: EditorSettings,
    /// Work queued for the owner, drained by [`Self::take_effects`].
    effects: Vec<EditorEffect>,
    /// The model's line count as of the last refresh.
    line_count: usize,
    /// The model revision this editor last refreshed from.
    seen_revision: u64,
    /// Where each line breaks into rows, as of `wrap_revision`.
    wrap: wrap::WrapMap,
    wrap_revision: u64,
    /// The collapsed folds, whose lines `wrap` hides.
    folds: Vec<folding::Fold>,
    /// The top of a restored view state, kept until the view next sets the
    /// wrap width, so a restore before lines first wrap still shows the
    /// same row.
    restored_top: Option<usize>,
    /// Decoration sets lifecycles have set on this view, in the order set.
    view_decorations: Vec<decorations::ViewDecorationSet>,
    /// Projection of shared, anchored buffer contributions and this view's
    /// decorations.
    decorations: Vec<RenderedDecoration>,
    gutter_markers: Vec<RenderedGutterMarker>,
    contribution_actions: Vec<RenderedContributionAction>,
    /// The brackets matched at the caret.
    bracket_cache: RefCell<Option<brackets::BracketCache>>,
    /// Vertical scroll offset in rows, fractional while a wheel scroll
    /// stops between rows (0 = top of buffer).
    scroll_top: f32,
    /// Horizontal scroll in pixels, which the view keeps from shaped text;
    /// recorded with the view state.
    scroll_x: f32,
    /// The viewport's height in rows, fractional for a partly shown last
    /// row; set by the view each layout, 0 until then.
    viewport_rows: f32,
    /// Caret position: (line index, byte offset within that line).
    cursor_line: usize,
    cursor_col: usize,
    /// The display cell vertical movement aims for, kept across short lines.
    preferred_column: Option<usize>,
    /// Whether the next layout scrolls the caret into view.
    reveal_caret: bool,
    preserve_scroll_on_layout: bool,
    /// Selection anchor (the "other" end of the selection, opposite the
    /// caret). Only meaningful while `has_selection` is true.
    anchor_line: usize,
    anchor_col: usize,
    has_selection: bool,
    position_range: Option<AnchoredRangeId>,
    selection_reversed: bool,
    /// IME preedit (marked) range as document-wide UTF-16 offsets. `None` =
    /// no active composition.
    marked_range_utf16: Option<Range<usize>>,
    history_context: u64,
    last_history_restore_generation: u64,
    completion: Option<CompletionController>,
    completion_surface: Option<CompletionSurface>,
    next_completion_generation: u64,
    /// A line Enter left for the indent query to indent, once the tree
    /// covers the edit.
    pending_indent: Option<indentation::PendingIndent>,
    /// Pairs typing closed, tracked while the caret stays inside them.
    auto_closed: Vec<brackets::AutoClosed>,
    /// Named key contexts keybindings can require, each owned by the
    /// extension lifecycle that set it. Presentation state, never recorded.
    key_contexts: Vec<(knot_protocol::ExtensionKey, Arc<str>)>,
    /// Caret shapes lifecycles have set; the latest one wins and each belongs
    /// to its lifecycle. Presentation state, never recorded.
    caret_shapes: Vec<(knot_protocol::ExtensionKey, CaretShape)>,
    /// Bars lifecycles have opened above the text, and the field among them
    /// that has the editor's focus. Presentation state, never recorded.
    bars: bars::EditorBars,
}

impl EditorState {
    /// Build an editor over `model` whose settings resolve for view `kind`.
    pub fn new(
        kind: impl Into<Arc<str>>,
        settings: EditorSettings,
        buffer: &mut BufferModel,
    ) -> Self {
        let line_count = buffer.line_count();
        let seen_revision = buffer.revision();
        let (decorations, gutter_markers, contribution_actions) =
            project_contributions(buffer, buffer.resolved_contributions());
        let position_range = buffer.add_view_position(0..0);
        Self {
            view_kind: kind.into(),
            settings,
            effects: Vec::new(),
            line_count,
            seen_revision,
            wrap: wrap::WrapMap::new(line_count),
            wrap_revision: seen_revision,
            folds: Vec::new(),
            restored_top: None,
            view_decorations: Vec::new(),
            decorations,
            gutter_markers,
            contribution_actions,
            bracket_cache: RefCell::default(),
            scroll_top: 0.,
            scroll_x: 0.,
            viewport_rows: 0.,
            cursor_line: 0,
            cursor_col: 0,
            preferred_column: None,
            reveal_caret: true,
            preserve_scroll_on_layout: false,
            anchor_line: 0,
            anchor_col: 0,
            has_selection: false,
            position_range,
            selection_reversed: false,
            marked_range_utf16: None,
            history_context: NEXT_HISTORY_CONTEXT.fetch_add(1, Ordering::Relaxed),
            last_history_restore_generation: 0,
            completion: None,
            completion_surface: None,
            next_completion_generation: 1,
            pending_indent: None,
            auto_closed: Vec::new(),
            key_contexts: Vec::new(),
            caret_shapes: Vec::new(),
            bars: bars::EditorBars::default(),
        }
    }

    /// Follow a change to `buffer` that this editor did not make, or that it
    /// made through another path: drop a completion the change outdated,
    /// refresh derived state and the selection's anchored position, and
    /// apply an undo or redo's restored selection or a pending indent.
    pub fn buffer_changed(&mut self, buffer: &mut BufferModel) {
        if self.completion.as_ref().is_some_and(|completion| {
            completion.revision() != buffer.revision() || !buffer.is_open()
        }) {
            self.completion = None;
            self.completion_surface = None;
        }
        if self.seen_revision != buffer.revision() {
            self.seen_revision = buffer.revision();
            self.marked_range_utf16 = None;
            self.preferred_column = None;
        }
        self.refresh(buffer, buffer.resolve_view_position(self.position_range));
        if let Some(restore) = buffer.history_view_restore() {
            self.apply_history_view_restore(buffer, restore.generation, &restore.state);
        }
        self.reveal_caret_folds(buffer);
        self.sync_view_position(buffer);
        self.apply_pending_indent(buffer);
    }

    /// Release the anchors this editor holds in `buffer`, as it goes away.
    pub fn release(&mut self, buffer: &mut BufferModel) {
        buffer.remove_view_position(self.position_range);
        let pairs = std::mem::take(&mut self.auto_closed);
        self.release_auto_closed(pairs, buffer);
        self.release_folds(buffer);
        self.release_decorations(buffer);
    }

    /// The `view` fact settings resolve for: `editor` for documents, or the
    /// contributed view ID of the frame that embeds this editor.
    pub fn view_kind(&self) -> &str {
        &self.view_kind
    }

    /// Follow `settings` from now on.
    pub fn set_settings(&mut self, settings: EditorSettings) {
        self.settings = settings;
    }

    /// The work queued for the owner since the last call, in order.
    pub fn take_effects(&mut self) -> Vec<EditorEffect> {
        std::mem::take(&mut self.effects)
    }

    /// The caret as a buffer line and a byte column in it.
    pub fn caret(&self) -> (usize, usize) {
        (self.cursor_line, self.cursor_col)
    }

    /// The selection's other end, the caret's own position without a
    /// selection.
    pub fn anchor(&self) -> (usize, usize) {
        if self.has_selection {
            (self.anchor_line, self.anchor_col)
        } else {
            self.caret()
        }
    }

    /// The viewport's height in rows, as the view last laid it out.
    pub fn viewport_rows(&self) -> f32 {
        self.viewport_rows
    }

    /// Select from `anchor` to `caret` without what a selection command also
    /// does, such as revealing the caret, for a test.
    #[cfg(any(test, feature = "test-support"))]
    pub fn place_for_test(
        &mut self,
        anchor: (usize, usize),
        caret: (usize, usize),
        buffer: &mut BufferModel,
    ) {
        (self.anchor_line, self.anchor_col) = anchor;
        (self.cursor_line, self.cursor_col) = caret;
        self.has_selection = anchor != caret;
        self.preferred_column = None;
        self.sync_view_position(buffer);
    }

    /// Scroll to `rows` within the text, for a test.
    #[cfg(any(test, feature = "test-support"))]
    pub fn scroll_to_for_test(&mut self, rows: f32) {
        self.scroll_top = rows;
        self.clamp_scroll();
    }

    /// Lay out a viewport `rows` rows tall without a view, for a test.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_viewport_rows_for_test(&mut self, rows: f32) {
        self.viewport_rows = rows;
    }

    /// The vertical scroll in rows, fractional between rows.
    pub fn scroll_top(&self) -> f32 {
        self.scroll_top
    }

    /// The horizontal scroll in pixels.
    pub fn scroll_x(&self) -> f32 {
        self.scroll_x
    }

    /// Where each line breaks into rows.
    pub fn wrap(&self) -> &wrap::WrapMap {
        &self.wrap
    }

    /// The buffer's line count as of the last refresh.
    pub fn line_count(&self) -> usize {
        self.line_count
    }

    /// The buffer revision this editor last refreshed from.
    pub fn seen_revision(&self) -> u64 {
        self.seen_revision
    }

    /// The contributed decorations, the buffer's and this view's, by line.
    pub fn decorations(&self) -> &[RenderedDecoration] {
        &self.decorations
    }

    /// The most severe gutter marker of each line that has one, by line.
    pub fn gutter_markers(&self) -> &[RenderedGutterMarker] {
        &self.gutter_markers
    }

    /// The contributed commands a click on their text runs.
    pub fn contribution_actions(&self) -> &[RenderedContributionAction] {
        &self.contribution_actions
    }

    /// The completion list, while a completion is open.
    pub fn completion_surface(&self) -> Option<&CompletionSurface> {
        self.completion_surface.as_ref()
    }

    /// The generation of the open completion.
    pub fn completion_generation(&self) -> Option<u64> {
        self.completion
            .as_ref()
            .map(CompletionController::generation)
    }

    /// The IME preedit range as document-wide UTF-16 offsets.
    pub fn marked_range_utf16(&self) -> Option<Range<usize>> {
        self.marked_range_utf16.clone()
    }

    /// The open completion's item, pending, and failure counts and its
    /// generation.
    #[cfg(any(test, feature = "test-support"))]
    pub fn completion_state(&self) -> Option<(usize, usize, usize, u64)> {
        let controller = self.completion.as_ref()?;
        let snapshot = controller.snapshot();
        Some((
            snapshot.items.len(),
            snapshot.pending_provider_count,
            snapshot.failures.len(),
            controller.generation(),
        ))
    }

    pub fn selected_byte_range(&self, model: &BufferModel) -> Option<Range<usize>> {
        self.selection_range().map(|(start, end)| {
            to_flat_byte(model, start.0, start.1)..to_flat_byte(model, end.0, end.1)
        })
    }

    /// Retain deliberate scrolling when this editor receives a relocated
    /// viewport. Pending caret reveals from editing still take precedence.
    pub fn preserve_scroll_for_relocation(&mut self) {
        self.preserve_scroll_on_layout = true;
    }

    /// The restorable presentation state: selection ends and the top visible
    /// row as byte offsets, and horizontal scroll in pixels.
    pub fn view_state(&self, model: &BufferModel) -> EditorViewState {
        let state = self.history_view_state(model);
        EditorViewState {
            anchor: state.anchor,
            caret: state.caret,
            top: self.top_offset(model),
            scroll_x: self.scroll_x,
            folds: self.fold_bytes(model),
        }
    }

    /// Apply a restored presentation state. Offsets clamp to the current text
    /// and snap back to a character boundary, so changed content never yields
    /// an invalid position; the restored scroll is kept instead of revealing
    /// the caret.
    pub fn restore_view_state(&mut self, state: EditorViewState, buffer: &mut BufferModel) {
        self.restore_folds(&state.folds, buffer);
        let model = &*buffer;
        let snap = |offset: usize| {
            let mut offset = offset.min(model.len());
            while !model.is_char_boundary(offset) {
                offset -= 1;
            }
            offset
        };
        (self.anchor_line, self.anchor_col) = flat_byte_to_position(model, snap(state.anchor));
        (self.cursor_line, self.cursor_col) = flat_byte_to_position(model, snap(state.caret));
        self.has_selection = snap(state.anchor) != snap(state.caret);
        let (top_line, top_col) = flat_byte_to_position(model, snap(state.top));
        self.scroll_top = self.wrap.row_of(top_line, top_col) as f32;
        self.restored_top = Some(snap(state.top));
        self.scroll_x = state.scroll_x.max(0.);
        self.preferred_column = None;
        self.reveal_caret = false;
        self.preserve_scroll_on_layout = true;
        self.sync_view_position(buffer);
    }

    /// The selection's anchor and caret as byte offsets; equal without a
    /// selection.
    pub fn selection(&self, buffer: &BufferModel) -> (usize, usize) {
        let state = self.history_view_state(buffer);
        (state.anchor, state.caret)
    }

    /// The key contexts some live lifecycle has set on this editor.
    pub fn key_contexts(&self) -> Vec<&str> {
        let mut contexts = self
            .key_contexts
            .iter()
            .map(|(_, name)| name.as_ref())
            .collect::<Vec<_>>();
        contexts.sort_unstable();
        contexts.dedup();
        contexts
    }

    /// Set or clear `owner`'s key context `name`. Lifecycles own their
    /// contexts independently, so one clearing a name leaves another's.
    pub fn set_key_context(
        &mut self,
        owner: knot_protocol::ExtensionKey,
        name: &str,
        active: bool,
    ) {
        self.key_contexts
            .retain(|(key, context)| *key != owner || context.as_ref() != name);
        if active {
            self.key_contexts.push((owner, Arc::<str>::from(name)));
        }
    }

    /// The shape the caret is drawn with: the latest lifecycle's, else a bar.
    pub fn caret_shape(&self) -> CaretShape {
        self.caret_shapes
            .last()
            .map_or(CaretShape::Bar, |(_, shape)| *shape)
    }

    /// Set `owner`'s caret shape, or clear it with `None` to reveal the
    /// previous lifecycle's.
    pub fn set_caret_shape(
        &mut self,
        owner: knot_protocol::ExtensionKey,
        shape: Option<CaretShape>,
    ) {
        self.caret_shapes.retain(|(key, _)| *key != owner);
        if let Some(shape) = shape {
            self.caret_shapes.push((owner, shape));
        }
    }

    /// Whether an IME composition is in progress.
    pub fn is_composing(&self) -> bool {
        self.marked_range_utf16.is_some()
    }

    /// Clear every key context, caret shape, bar, and decoration `owner` set.
    pub fn remove_owner_state(
        &mut self,
        owner: knot_protocol::ExtensionKey,
        buffer: &mut BufferModel,
    ) {
        self.key_contexts.retain(|(key, _)| *key != owner);
        self.caret_shapes.retain(|(key, _)| *key != owner);
        self.bars.remove_owner(owner);
        self.remove_decorations(owner, buffer);
        self.project_contributions(buffer);
    }

    /// The bars above the text and the field with the editor's focus.
    pub fn bars(&self) -> &bars::EditorBars {
        &self.bars
    }

    pub fn bars_mut(&mut self) -> &mut bars::EditorBars {
        &mut self.bars
    }

    /// Move the selection to offsets an extension named, or return `false`
    /// and change nothing unless both lie on character boundaries within the
    /// text.
    pub fn set_selection_checked(
        &mut self,
        anchor: u64,
        caret: u64,
        reveal: Reveal,
        buffer: &mut BufferModel,
    ) -> bool {
        let (Some(anchor), Some(caret)) = (buffer.text_offset(anchor), buffer.text_offset(caret))
        else {
            return false;
        };
        self.set_selection(anchor, caret, reveal, buffer);
        true
    }

    /// Move the selection to byte offsets on character boundaries within the
    /// text. Like other caret moves, this ends the current typing undo group.
    pub fn set_selection(
        &mut self,
        anchor: usize,
        caret: usize,
        reveal: Reveal,
        buffer: &mut BufferModel,
    ) {
        self.break_history_group(buffer);
        self.marked_range_utf16 = None;
        self.preferred_column = None;
        let model = &*buffer;
        (self.anchor_line, self.anchor_col) = flat_byte_to_position(model, anchor);
        (self.cursor_line, self.cursor_col) = flat_byte_to_position(model, caret);
        self.has_selection = anchor != caret;
        self.reveal_caret_folds(buffer);
        let viewport = self.viewport_rows;
        if viewport > 0. && matches!(reveal, Reveal::Center | Reveal::AlwaysCenter) {
            let row = self.caret_row() as f32;
            if reveal == Reveal::AlwaysCenter
                || row < self.scroll_top
                || row + 1. > self.scroll_top + viewport
            {
                self.scroll_top = row + 0.5 - viewport / 2.;
            }
        }
        let rows = self.wrap.rows(self.cursor_line);
        match reveal {
            Reveal::Top if viewport > 0. => {
                self.scroll_top = rows.start as f32;
            }
            Reveal::Bottom if viewport > 0. => {
                self.scroll_top = rows.end as f32 - viewport;
            }
            _ => {}
        }
        self.finish_position_change(buffer);
    }

    /// Select from `anchor` to `caret`, a caret alone when they are equal,
    /// as a click or drag does.
    pub fn select_from(
        &mut self,
        anchor: (usize, usize),
        caret: (usize, usize),
        buffer: &mut BufferModel,
    ) {
        // Selecting a collapsed fold's first line selects the whole fold.
        let caret = self.out_of_folds(&*buffer, caret, caret >= anchor);
        (self.anchor_line, self.anchor_col) = anchor;
        (self.cursor_line, self.cursor_col) = caret;
        self.has_selection = anchor != caret;
        self.marked_range_utf16 = None;
        self.preferred_column = None;
        self.finish_position_change(buffer);
    }

    /// The first row wholly inside the viewport and how many rows are, from
    /// the last layout; always at least one row.
    pub fn viewport(&self) -> (usize, usize) {
        let last_row = self.wrap.row_count().saturating_sub(1);
        let first = (self.scroll_top.ceil() as usize).min(last_row);
        let end =
            ((self.scroll_top + self.viewport_rows).floor() as usize).min(self.wrap.row_count());
        (first, end.saturating_sub(first).max(1))
    }

    /// The buffer line the document row `row` belongs to and the byte column
    /// where the row starts in it; a row past the last is the last.
    pub fn row_start(&self, row: usize, model: &BufferModel) -> (usize, usize) {
        let (line, sub) = self.wrap.position(row);
        let start = self.wrap.row_range(line, sub, line_end(model, line)).start;
        (line, start)
    }

    /// The document row showing byte `column` of `line`, with a column at a
    /// break on the next row. `None` past the last line or off a character
    /// boundary.
    pub fn row_of_position(
        &self,
        line: usize,
        column: usize,
        model: &BufferModel,
    ) -> Option<usize> {
        if line >= model.line_count() || !line_text(model, line).is_char_boundary(column) {
            return None;
        }
        Some(self.wrap.row_of(line, column))
    }

    /// Scroll by `rows`, negative upward, without moving the caret or
    /// revealing it; the scroll stays within the text.
    pub fn scroll_by_rows(&mut self, rows: i64) {
        self.scroll_top += rows as f32;
        self.reveal_caret = false;
        self.clamp_scroll();
    }

    /// Scroll by `rows`, which may be fractional, and `x` pixels sideways
    /// within `max_x`, without moving the caret, as the wheel does.
    pub fn scroll_by(&mut self, rows: f32, x: f32, max_x: f32) {
        self.scroll_top += rows;
        self.scroll_x = (self.scroll_x + x).clamp(0., max_x.max(0.));
        self.reveal_caret = false;
        self.clamp_scroll();
    }

    /// Record the viewport's height in rows, as the view laid it out, and
    /// scroll the caret into view when it moved or the viewport resized
    /// unless a relocation kept the scroll. Returns whether the caret is to
    /// be revealed horizontally. Changes nothing the view paints from, so it
    /// does not notify.
    pub fn lay_out(&mut self, viewport_rows: f32, resized: bool) -> bool {
        self.viewport_rows = viewport_rows;
        let reveal = self.reveal_caret || (resized && !self.preserve_scroll_on_layout);
        if reveal {
            self.ensure_cursor_visible();
        }
        if self.wrap.width().is_some() {
            // Wrapped rows fit the width, so nothing scrolls sideways.
            self.scroll_x = 0.;
        }
        self.reveal_caret = false;
        self.preserve_scroll_on_layout = false;
        self.clamp_scroll();
        reveal && self.wrap.width().is_none()
    }

    /// Scroll sideways so the caret, `x` pixels into the text, shows in a
    /// text area `width` pixels wide.
    pub fn reveal_x(&mut self, x: f32, width: f32) {
        if x < self.scroll_x {
            self.scroll_x = x.max(0.);
        }
        if x + 2. > self.scroll_x + width {
            self.scroll_x = (x + 2. - width).max(0.);
        }
    }

    /// Refresh state derived from the model and, when given, move the
    /// selection to the model's anchored view position.
    fn refresh(&mut self, model: &BufferModel, position: Option<Range<usize>>) {
        self.line_count = model.line_count();
        self.sync_wrap(model);
        self.sync_folds(model);
        self.project_contributions(model);
        if let Some(position) = position {
            self.restore_view_position(model, position);
        }
    }

    fn sync_view_position(&mut self, buffer: &mut BufferModel) {
        let state = self.history_view_state(&*buffer);
        let (anchor, caret) = (state.anchor, state.caret);
        self.selection_reversed = self.has_selection && caret < anchor;
        let range = anchor.min(caret)..anchor.max(caret);
        self.position_range = buffer.replace_view_position(self.position_range, range);
    }

    fn finish_position_change(&mut self, buffer: &mut BufferModel) {
        self.reveal_caret_folds(buffer);
        self.prune_auto_closed(buffer);
        self.dismiss_completion();
        self.reveal_caret = true;
        self.ensure_cursor_visible();
        self.clamp_scroll();
        self.sync_view_position(buffer);
    }

    pub(crate) fn break_history_group(&self, buffer: &mut BufferModel) {
        buffer.break_history_group();
    }

    fn restore_view_position(&mut self, model: &BufferModel, range: Range<usize>) {
        let start = flat_byte_to_position(model, range.start);
        let end = flat_byte_to_position(model, range.end);
        if self.has_selection && !range.is_empty() {
            if self.selection_reversed {
                (self.cursor_line, self.cursor_col) = start;
                (self.anchor_line, self.anchor_col) = end;
            } else {
                (self.anchor_line, self.anchor_col) = start;
                (self.cursor_line, self.cursor_col) = end;
            }
        } else {
            (self.cursor_line, self.cursor_col) = end;
            self.has_selection = false;
        }
        self.clamp_scroll();
    }

    pub fn history_view_state(&self, model: &BufferModel) -> HistoryViewState {
        let caret = to_flat_byte(model, self.cursor_line, self.cursor_col);
        let anchor = if self.has_selection {
            to_flat_byte(model, self.anchor_line, self.anchor_col)
        } else {
            caret
        };
        HistoryViewState {
            context: self.history_context,
            anchor,
            caret,
        }
    }

    fn apply_history_view_restore(
        &mut self,
        model: &BufferModel,
        generation: u64,
        state: &HistoryViewState,
    ) {
        if generation <= self.last_history_restore_generation {
            return;
        }
        self.last_history_restore_generation = generation;
        if state.context != self.history_context {
            return;
        }
        (self.anchor_line, self.anchor_col) = flat_byte_to_position(model, state.anchor);
        (self.cursor_line, self.cursor_col) = flat_byte_to_position(model, state.caret);
        self.has_selection = state.anchor != state.caret;
        self.selection_reversed = state.caret < state.anchor;
        self.reveal_caret = true;
        self.ensure_cursor_visible();
        self.clamp_scroll();
    }

    /// The rows the viewport shows at the current scroll, including a partly
    /// visible last row.
    pub fn visible_rows(&self) -> Range<usize> {
        let count = self.wrap.row_count();
        let first = (self.scroll_top.floor() as usize).min(count);
        let rows = self.viewport_rows.ceil() as usize + 1;
        first..(first + rows).min(count)
    }

    /// The lines with a row in [`Self::visible_rows`].
    pub fn visible_lines(&self) -> Range<usize> {
        let rows = self.visible_rows();
        if rows.is_empty() {
            return self.line_count..self.line_count;
        }
        self.wrap.position(rows.start).0..self.wrap.position(rows.end - 1).0 + 1
    }

    /// The row the caret shows on.
    pub fn caret_row(&self) -> usize {
        self.wrap.row_of(self.cursor_line, self.cursor_col)
    }

    /// The start of the top visible row, as a byte offset.
    fn top_offset(&self, model: &BufferModel) -> usize {
        let (line, sub) = self.wrap.position(self.scroll_top as usize);
        let start = self.wrap.row_range(line, sub, line_end(model, line)).start;
        to_flat_byte(model, line, start)
    }

    /// Bring the wrap map up to the model's revision, rewrapping the lines
    /// that changed since it last followed it.
    fn sync_wrap(&mut self, model: &BufferModel) {
        if self.wrap_revision == model.revision() {
            return;
        }
        let change = model.line_changes_since(self.wrap_revision);
        self.wrap
            .edit(model.line_count(), change, |line| line_text(model, line));
        self.wrap_revision = model.revision();
    }

    fn clamp_scroll(&mut self) {
        let max = (self.wrap.row_count() as f32 - self.viewport_rows).max(0.);
        self.scroll_top = self.scroll_top.clamp(0., max);
    }

    /// After any cursor move, scroll just enough to keep the caret inside
    /// the viewport, leaving the caller's later `clamp_scroll` to bound it.
    fn ensure_cursor_visible(&mut self) {
        let viewport = self.viewport_rows;
        if viewport <= 0. {
            return;
        }
        let row = self.caret_row() as f32;
        if row < self.scroll_top {
            self.scroll_top = row;
        } else if row + 1. > self.scroll_top + viewport {
            self.scroll_top = row + 1. - viewport;
        }
    }

    /// Ordered (start, end) of the active selection, where start <= end in
    /// (line, col) lexicographic order, or `None` with only a caret.
    pub fn selection_range(&self) -> Option<((usize, usize), (usize, usize))> {
        if !self.has_selection {
            return None;
        }
        let a = (self.anchor_line, self.anchor_col);
        let c = (self.cursor_line, self.cursor_col);
        if a <= c { Some((a, c)) } else { Some((c, a)) }
    }

    /// Commit a replacement to the authoritative model and refresh derived
    /// state before resolving the resulting caret.
    fn splice(
        &mut self,
        range: Range<usize>,
        text: &str,
        grouping: Option<EditGroupKind>,
        view: HistoryViewChange,
        buffer: &mut BufferModel,
    ) {
        self.dismiss_completion();
        // A read-only or closed buffer refuses the edit and stays as it was.
        let _ = if let Some(kind) = grouping {
            buffer.replace_grouped_from_view(
                range,
                text,
                EditGrouping {
                    context: self.history_context,
                    kind,
                },
                view,
            )
        } else {
            buffer.replace_from_view(range, text, view)
        };
        let model = &*buffer;
        self.seen_revision = model.revision();
        self.refresh(model, None);
    }

    /// Commit a command's `edits`, ascending and disjoint in pre-commit
    /// coordinates, as one undo step whose undo restores the current
    /// selection, then select from `anchor` to `caret` in the new text.
    fn commit_edits(
        &mut self,
        edits: Vec<(Range<usize>, String)>,
        anchor: usize,
        caret: usize,
        buffer: &mut BufferModel,
    ) {
        if edits.is_empty() || !buffer.is_editable() {
            return;
        }
        self.break_history_group(buffer);
        self.dismiss_completion();
        let view = HistoryViewChange {
            before: self.history_view_state(&*buffer),
            after: HistoryViewState {
                context: self.history_context,
                anchor,
                caret,
            },
        };
        if !buffer.apply_edits_from_view(edits, view).unwrap_or(false) {
            return;
        }
        let model = &*buffer;
        self.seen_revision = model.revision();
        self.refresh(model, None);
        (self.anchor_line, self.anchor_col) = flat_byte_to_position(model, anchor);
        (self.cursor_line, self.cursor_col) = flat_byte_to_position(model, caret);
        self.has_selection = anchor != caret;
        self.marked_range_utf16 = None;
        self.preferred_column = None;
        self.finish_position_change(buffer);
    }

    /// The byte range replaced by input that names no range: the marked text
    /// if there is one, otherwise the selection, otherwise the caret.
    fn implicit_replacement_range(&self, model: &BufferModel) -> Range<usize> {
        if let Some(marked) = &self.marked_range_utf16 {
            return model.offset_of_utf16(marked.start)..model.offset_of_utf16(marked.end);
        }
        let state = self.history_view_state(model);
        state.anchor.min(state.caret)..state.anchor.max(state.caret)
    }

    /// Replace a byte range, or the implicit replacement range, with `text`,
    /// leaving the caret after it. Newlines take the buffer's line ending.
    fn replace_text_with_grouping(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        grouping: Option<EditGroupKind>,
        buffer: &mut BufferModel,
    ) {
        if !buffer.is_editable() {
            return;
        }
        self.preferred_column = None;
        let model = &*buffer;
        let text = text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\n', model.line_ending());
        let range = range.unwrap_or_else(|| self.implicit_replacement_range(model));
        let before = self.history_view_state(model);
        let after_offset = range.start + text.len();
        let after = HistoryViewState {
            context: self.history_context,
            anchor: after_offset,
            caret: after_offset,
        };
        self.splice(
            range,
            &text,
            grouping,
            HistoryViewChange { before, after },
            buffer,
        );
        self.marked_range_utf16 = None;
        (self.cursor_line, self.cursor_col) = flat_byte_to_position(&*buffer, after_offset);
        self.has_selection = false;
        self.finish_position_change(buffer);
    }

    /// Convert an optional platform UTF-16 range to bytes.
    fn bytes_for_utf16(
        &self,
        range: Option<Range<usize>>,
        model: &BufferModel,
    ) -> Option<Range<usize>> {
        range.map(|range| model.offset_of_utf16(range.start)..model.offset_of_utf16(range.end))
    }
}

/// Where `offset` lands after `edits`, ascending and disjoint in pre-commit
/// coordinates. An offset inside a replaced range moves to its start, and an
/// insertion at the offset moves it past the inserted text unless
/// `before_insertions`.
fn map_offset(edits: &[(Range<usize>, String)], offset: usize, before_insertions: bool) -> usize {
    let mut mapped = offset;
    for (range, text) in edits {
        if range.start > offset
            || (range.start == offset && (before_insertions || !range.is_empty()))
        {
            break;
        }
        if offset < range.end {
            return mapped - (offset - range.start);
        }
        mapped = mapped + text.len() - range.len();
    }
    mapped
}

fn project_contributions(
    model: &BufferModel,
    contributions: Vec<ResolvedEditorContribution>,
) -> (
    Vec<RenderedDecoration>,
    Vec<RenderedGutterMarker>,
    Vec<RenderedContributionAction>,
) {
    let mut decorations = Vec::new();
    let mut gutter_markers = Vec::new();
    let mut actions = Vec::new();
    for contribution in contributions {
        let start = contribution.range.start_byte_offset;
        let end = contribution.range.end_byte_offset;
        let first_line = model.line_of_offset(start);
        let last_line = model.line_of_offset(end);

        if let Some(key) = &contribution.gutter {
            gutter_markers.push(RenderedGutterMarker {
                line: first_line,
                key: key.clone(),
                command: contribution.command.clone(),
                source: contribution.source,
                range: contribution.range,
            });
        }

        if contribution.decoration.is_none() && contribution.command.is_none() {
            continue;
        }

        for line in first_line..=last_line {
            let Range {
                start: line_start,
                end: line_end,
            } = model.line_range(line);
            let segment_start = start.max(line_start);
            let segment_end = end.min(line_end);

            if let Some(decoration) = &contribution.decoration
                && decoration.style == DecorationStyle::Line
            {
                // A range ending at column 0 of a later line leaves that
                // line out, as indenting a selection does.
                if line == first_line || end > line_start {
                    decorations.push(RenderedDecoration {
                        line,
                        start: 0,
                        end: 0,
                        key: decoration.color.clone(),
                        style: decoration.style,
                    });
                }
            } else if let Some(decoration) = &contribution.decoration
                && segment_start < segment_end
            {
                decorations.push(RenderedDecoration {
                    line,
                    start: segment_start - line_start,
                    end: segment_end - line_start,
                    key: decoration.color.clone(),
                    style: decoration.style,
                });
            }

            if let Some(command) = &contribution.command
                && segment_start <= segment_end
                && start <= line_end
                && end >= line_start
            {
                actions.push(RenderedContributionAction {
                    line,
                    start: segment_start - line_start,
                    end: segment_end - line_start,
                    command: command.clone(),
                    source: contribution.source,
                    range: contribution.range,
                });
            }
        }
    }
    // Each line keeps its most severe marker, and among equals the latest.
    gutter_markers.reverse();
    gutter_markers.sort_by_key(|marker| {
        (
            marker.line,
            std::cmp::Reverse(decoration_precedence(&marker.key)),
        )
    });
    gutter_markers.dedup_by_key(|marker| marker.line);
    (decorations, gutter_markers, actions)
}

#[cfg(test)]
mod tests;
