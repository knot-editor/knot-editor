//! Native editors over an authoritative shared buffer model.
//!
//! An [`Editor`] holds one presentation's state and runs the native editing
//! and movement commands without a window; an [`EditorView`] renders it and
//! handles pointer and platform text input. Caret and selection use byte
//! positions backed by model anchors. Movement steps by graphemes and, across
//! rows, keeps a display column in monospace cells; pointer hit testing uses
//! shaped coordinates. Each editor independently owns scrolling, selection,
//! completion, and IME preedit. An editor keeps no copy of the text: it reads
//! the lines it needs from the model and converts positions through the
//! model's line index.

mod brackets;
mod comments;
mod completion;
mod editing;
mod gutter;
mod indentation;
mod input;
mod layout;
mod lines;
pub(super) mod native_commands;
mod paint;
mod text;
mod view;
mod wrap;

pub use view::EditorView;

use gutter::GutterLayout;
use layout::*;
use text::*;

use gpui::{prelude::*, *};
use std::{
    cell::RefCell,
    ops::Range,
    sync::atomic::{AtomicU64, Ordering},
};
use unicode_segmentation::UnicodeSegmentation;

use crate::core::anchored_range::AnchoredRangeId;
use crate::host::protocol::{
    BufferHandle, ByteRange, CompletionProviderRegistrationId, CompletionRequest,
    CompletionResponse, DecorationStyle, TextEdit,
};

use super::commands::dispatch as commands;
use super::typography::text_font as make_font;

use super::{
    CompletionAccept, CompletionDismiss, CompletionNext, CompletionPrevious, EDITOR_KEY_CONTEXT,
    completion::{CompletionController, CompletionProviderRegistration, CompletionSurface},
    history::{EditGroupKind, EditGrouping, HistoryViewChange, HistoryViewState},
    indentation::{IndentUnit, leading_whitespace},
    languages,
    model::{BufferModel, ContributionSource, ResolvedEditorContribution, decoration_precedence},
    settings::{self, ApplicationSettings, SettingsScope},
    theme::{Highlight, Theme, color, theme},
};

static NEXT_HISTORY_CONTEXT: AtomicU64 = AtomicU64::new(1);

const CARET_BLINK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

/// The caret shows whenever it moves, the text changes, or the view gains
/// focus, then toggles every [`CARET_BLINK_INTERVAL`] while the view stays
/// focused.
#[derive(Default)]
struct CaretBlink {
    /// The caret position, selection, and revision blinking restarted from;
    /// `None` while unfocused.
    key: Option<(usize, usize, bool, u64)>,
    visible: bool,
    _task: Option<Task<()>>,
}

/// One contributed decoration segment, painted in `style` and the color of
/// `key`. `start` and `end` are byte columns within `line`; a `line`
/// decoration covers its whole line and leaves them at 0.
#[derive(Clone)]
struct RenderedDecoration {
    line: usize,
    start: usize,
    end: usize,
    key: String,
    style: DecorationStyle,
}

/// Syntax highlights queried for a range of whole lines.
struct SyntaxCache {
    generation: u64,
    range: Range<usize>,
    spans: Vec<(Range<usize>, Highlight)>,
}

impl SyntaxCache {
    fn covers(&self, generation: u64, range: &Range<usize>) -> bool {
        self.generation == generation
            && self.range.start <= range.start
            && range.end <= self.range.end
    }
}

/// The marker the gutter shows on `line`: the most severe of the line's
/// markers, with the command a click runs.
#[derive(Clone)]
struct RenderedGutterMarker {
    line: usize,
    key: String,
    command: Option<String>,
    source: ContributionSource,
    range: ByteRange,
}

#[derive(Clone)]
struct RenderedContributionAction {
    line: usize,
    start: usize,
    end: usize,
    command: String,
    source: ContributionSource,
    range: ByteRange,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct EditorContributionAction {
    pub command: String,
    pub source: ContributionSource,
    pub range: ByteRange,
    pub model: Entity<BufferModel>,
    pub window: AnyWindowHandle,
    pub focus: WeakFocusHandle,
}

/// The units a drag selects: characters after a click, words after a
/// double-click, lines after a triple-click.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectionUnit {
    Character,
    Word,
    Line,
}

/// The unit a drag started on: a caret position for characters, or the
/// clicked word or line, which stays selected whichever way the drag goes.
#[derive(Clone, Copy, Debug)]
struct DragAnchor {
    unit: SelectionUnit,
    start: (usize, usize),
    end: (usize, usize),
}

#[derive(Clone, Copy)]
pub(crate) struct EditorRenderingOptions {
    /// Whether the view shows a gutter of markers and line numbers.
    pub show_gutter: bool,
}

impl Default for EditorRenderingOptions {
    fn default() -> Self {
        Self { show_gutter: true }
    }
}

/// Restorable editor presentation state. Offsets are absolute UTF-8 byte
/// positions in the shared buffer; `top` is the start of the first visible row.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EditorViewState {
    pub(crate) anchor: usize,
    pub(crate) caret: usize,
    pub(crate) top: usize,
    pub(crate) scroll_x: f32,
}

/// How a selection change scrolls to show the caret.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Reveal {
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
pub(crate) enum CaretShape {
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
/// native editing and movement commands. It needs no window: an
/// [`EditorView`] renders it, handles pointer and platform text input, and
/// reports the viewport's size and wrap width as it lays out.
pub struct Editor {
    /// The `view` fact settings resolve for: `editor` for documents, or the
    /// contributed view ID of the frame that embeds this editor.
    view_kind: SharedString,
    /// Authoritative document state. Everything below is editor state or
    /// derived from the model's current revision.
    model: Entity<BufferModel>,
    /// The model's line count as of the last refresh.
    line_count: usize,
    /// The model revision this editor last refreshed from.
    seen_revision: u64,
    /// Where each line breaks into rows, as of `wrap_revision`.
    wrap: wrap::WrapMap,
    wrap_revision: u64,
    /// The top of a restored view state, kept until the view next sets the
    /// wrap width, so a restore before lines first wrap still shows the
    /// same row.
    restored_top: Option<usize>,
    /// Projection of shared, anchored buffer contributions.
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
    completion_tasks: Vec<(CompletionProviderRegistrationId, Task<()>)>,
    next_completion_generation: u64,
    /// A line Enter left for the indent query to indent, once the tree
    /// covers the edit.
    pending_indent: Option<indentation::PendingIndent>,
    /// Pairs typing closed, tracked while the caret stays inside them.
    auto_closed: Vec<brackets::AutoClosed>,
    /// Named key contexts keybindings can require, each owned by the
    /// extension lifecycle that set it. Presentation state, never recorded.
    key_contexts: Vec<(crate::host::lifecycle::ExtensionKey, SharedString)>,
    /// Caret shapes lifecycles have set; the latest one wins and each belongs
    /// to its lifecycle. Presentation state, never recorded.
    caret_shapes: Vec<(crate::host::lifecycle::ExtensionKey, CaretShape)>,
    _model_subscription: Subscription,
    _release_subscription: Subscription,
}

impl Editor {
    /// Build an editor over `model` whose settings resolve for view `kind`.
    pub(crate) fn new(
        model: Entity<BufferModel>,
        kind: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (line_count, seen_revision, (decorations, gutter_markers, contribution_actions)) = {
            let model = model.read(cx);
            (
                model.line_count(),
                model.revision(),
                project_contributions(model, model.resolved_contributions()),
            )
        };

        let position_range = model.update(cx, |model, _| model.add_view_position(0..0));
        let model_subscription = cx.observe(&model, |this, model, cx| {
            let model = model.read(cx);
            if this.completion.as_ref().is_some_and(|completion| {
                completion.revision() != model.revision() || !model.is_open()
            }) {
                this.completion = None;
                this.completion_surface = None;
                this.completion_tasks.clear();
            }
            if this.seen_revision != model.revision() {
                this.seen_revision = model.revision();
                this.marked_range_utf16 = None;
                this.preferred_column = None;
            }
            this.refresh(model, model.resolve_view_position(this.position_range));
            if let Some(restore) = model.history_view_restore() {
                this.apply_history_view_restore(model, restore.generation, &restore.state);
            }
            this.sync_view_position(cx);
            this.apply_pending_indent(cx);
            cx.notify();
        });
        let release_subscription = cx.on_release(|this, cx| {
            this.model.update(cx, |model, _| {
                model.remove_view_position(this.position_range);
            });
            let pairs = std::mem::take(&mut this.auto_closed);
            this.release_auto_closed(pairs, cx);
        });

        Self {
            view_kind: kind.into(),
            model,
            line_count,
            seen_revision,
            wrap: wrap::WrapMap::new(line_count),
            wrap_revision: seen_revision,
            restored_top: None,
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
            completion_tasks: Vec::new(),
            next_completion_generation: 1,
            pending_indent: None,
            auto_closed: Vec::new(),
            key_contexts: Vec::new(),
            caret_shapes: Vec::new(),
            _model_subscription: model_subscription,
            _release_subscription: release_subscription,
        }
    }

    pub(crate) fn model(&self) -> &Entity<BufferModel> {
        &self.model
    }

    #[cfg(test)]
    fn text(&self, cx: &App) -> String {
        self.model.read(cx).text()
    }

    #[cfg(test)]
    pub(crate) fn completion_state(&self) -> Option<(usize, usize, usize, u64)> {
        let controller = self.completion.as_ref()?;
        let snapshot = controller.snapshot();
        Some((
            snapshot.items.len(),
            snapshot.pending_provider_count,
            snapshot.failures.len(),
            controller.generation(),
        ))
    }

    pub(crate) fn selected_byte_range(&self, cx: &App) -> Option<Range<usize>> {
        let model = self.model.read(cx);
        self.selection_range().map(|(start, end)| {
            to_flat_byte(model, start.0, start.1)..to_flat_byte(model, end.0, end.1)
        })
    }

    /// Retain deliberate scrolling when this editor receives a relocated
    /// viewport. Pending caret reveals from editing still take precedence.
    pub(crate) fn preserve_scroll_for_relocation(&mut self) {
        self.preserve_scroll_on_layout = true;
    }

    /// The restorable presentation state: selection ends and the top visible
    /// row as byte offsets, and horizontal scroll in pixels.
    pub(crate) fn view_state(&self, cx: &App) -> EditorViewState {
        let model = self.model.read(cx);
        let state = self.history_view_state(model);
        EditorViewState {
            anchor: state.anchor,
            caret: state.caret,
            top: self.top_offset(model),
            scroll_x: self.scroll_x,
        }
    }

    /// Apply a restored presentation state. Offsets clamp to the current text
    /// and snap back to a character boundary, so changed content never yields
    /// an invalid position; the restored scroll is kept instead of revealing
    /// the caret.
    pub(crate) fn restore_view_state(&mut self, state: EditorViewState, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
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
        self.sync_view_position(cx);
        cx.notify();
    }

    /// The selection's anchor and caret as byte offsets; equal without a
    /// selection.
    pub(crate) fn selection(&self, cx: &App) -> (usize, usize) {
        let state = self.history_view_state(self.model.read(cx));
        (state.anchor, state.caret)
    }

    /// The key contexts some live lifecycle has set on this editor.
    pub(crate) fn key_contexts(&self) -> Vec<&str> {
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
    pub(crate) fn set_key_context(
        &mut self,
        owner: crate::host::lifecycle::ExtensionKey,
        name: &str,
        active: bool,
    ) {
        self.key_contexts
            .retain(|(key, context)| *key != owner || context.as_ref() != name);
        if active {
            self.key_contexts
                .push((owner, SharedString::from(name.to_owned())));
        }
    }

    /// The shape the caret is drawn with: the latest lifecycle's, else a bar.
    pub(crate) fn caret_shape(&self) -> CaretShape {
        self.caret_shapes
            .last()
            .map_or(CaretShape::Bar, |(_, shape)| *shape)
    }

    /// Set `owner`'s caret shape, or clear it with `None` to reveal the
    /// previous lifecycle's.
    pub(crate) fn set_caret_shape(
        &mut self,
        owner: crate::host::lifecycle::ExtensionKey,
        shape: Option<CaretShape>,
        cx: &mut Context<Self>,
    ) {
        self.caret_shapes.retain(|(key, _)| *key != owner);
        if let Some(shape) = shape {
            self.caret_shapes.push((owner, shape));
        }
        cx.notify();
    }

    /// Whether an IME composition is in progress.
    pub(crate) fn is_composing(&self) -> bool {
        self.marked_range_utf16.is_some()
    }

    /// The command a text binding routes this editor's typed text to.
    pub(crate) fn text_command(&self, cx: &App) -> Option<crate::host::protocol::CommandName> {
        cx.try_global::<crate::app::commands::keymaps::ApplicationKeymaps>()?
            .0
            .resolve_text(&self.view_kind, &self.key_contexts())
    }

    /// Clear every key context and caret shape `owner` set.
    pub(crate) fn remove_owner_state(
        &mut self,
        owner: crate::host::lifecycle::ExtensionKey,
        cx: &mut Context<Self>,
    ) {
        self.key_contexts.retain(|(key, _)| *key != owner);
        let had_shape = self.caret_shapes.iter().any(|(key, _)| *key == owner);
        self.caret_shapes.retain(|(key, _)| *key != owner);
        if had_shape {
            cx.notify();
        }
    }

    /// Move the selection to byte offsets on character boundaries within the
    /// text. Like other caret moves, this ends the current typing undo group.
    pub(crate) fn set_selection(
        &mut self,
        anchor: usize,
        caret: usize,
        reveal: Reveal,
        cx: &mut Context<Self>,
    ) {
        self.break_history_group(cx);
        self.marked_range_utf16 = None;
        self.preferred_column = None;
        let model = self.model.read(cx);
        (self.anchor_line, self.anchor_col) = flat_byte_to_position(model, anchor);
        (self.cursor_line, self.cursor_col) = flat_byte_to_position(model, caret);
        self.has_selection = anchor != caret;
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
        self.finish_position_change(cx);
    }

    /// Select from `anchor` to `caret`, a caret alone when they are equal,
    /// as a click or drag does.
    pub(super) fn select_from(
        &mut self,
        anchor: (usize, usize),
        caret: (usize, usize),
        cx: &mut Context<Self>,
    ) {
        (self.anchor_line, self.anchor_col) = anchor;
        (self.cursor_line, self.cursor_col) = caret;
        self.has_selection = anchor != caret;
        self.marked_range_utf16 = None;
        self.preferred_column = None;
        self.finish_position_change(cx);
    }

    /// The first row wholly inside the viewport and how many rows are, from
    /// the last layout; always at least one row.
    pub(crate) fn viewport(&self) -> (usize, usize) {
        let last_row = self.wrap.row_count().saturating_sub(1);
        let first = (self.scroll_top.ceil() as usize).min(last_row);
        let end =
            ((self.scroll_top + self.viewport_rows).floor() as usize).min(self.wrap.row_count());
        (first, end.saturating_sub(first).max(1))
    }

    /// The buffer line the document row `row` belongs to and the byte column
    /// where the row starts in it; a row past the last is the last.
    pub(crate) fn row_start(&self, row: usize, cx: &App) -> (usize, usize) {
        let (line, sub) = self.wrap.position(row);
        let model = self.model.read(cx);
        let start = self.wrap.row_range(line, sub, line_end(model, line)).start;
        (line, start)
    }

    /// The document row showing byte `column` of `line`, with a column at a
    /// break on the next row. `None` past the last line or off a character
    /// boundary.
    pub(crate) fn row_of_position(&self, line: usize, column: usize, cx: &App) -> Option<usize> {
        let model = self.model.read(cx);
        if line >= model.line_count() || !line_text(model, line).is_char_boundary(column) {
            return None;
        }
        Some(self.wrap.row_of(line, column))
    }

    /// Scroll by `rows`, negative upward, without moving the caret or
    /// revealing it; the scroll stays within the text.
    pub(crate) fn scroll_by_rows(&mut self, rows: i64, cx: &mut Context<Self>) {
        self.scroll_top += rows as f32;
        self.reveal_caret = false;
        self.clamp_scroll();
        cx.notify();
    }

    /// Scroll by `rows`, which may be fractional, and `x` pixels sideways
    /// within `max_x`, without moving the caret, as the wheel does.
    pub(super) fn scroll_by(&mut self, rows: f32, x: f32, max_x: f32, cx: &mut Context<Self>) {
        self.scroll_top += rows;
        self.scroll_x = (self.scroll_x + x).clamp(0., max_x.max(0.));
        self.reveal_caret = false;
        self.clamp_scroll();
        cx.notify();
    }

    /// Record the viewport's height in rows, as the view laid it out, and
    /// scroll the caret into view when it moved or the viewport resized
    /// unless a relocation kept the scroll. Returns whether the caret is to
    /// be revealed horizontally. Changes nothing the view paints from, so it
    /// does not notify.
    pub(super) fn lay_out(&mut self, viewport_rows: f32, resized: bool) -> bool {
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
    pub(super) fn reveal_x(&mut self, x: f32, width: f32) {
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
        (
            self.decorations,
            self.gutter_markers,
            self.contribution_actions,
        ) = project_contributions(model, model.resolved_contributions());
        if let Some(position) = position {
            self.restore_view_position(model, position);
        }
    }

    fn sync_view_position(&mut self, cx: &mut Context<Self>) {
        let state = self.history_view_state(self.model.read(cx));
        let (anchor, caret) = (state.anchor, state.caret);
        self.selection_reversed = self.has_selection && caret < anchor;
        let range = anchor.min(caret)..anchor.max(caret);
        self.position_range = self.model.update(cx, |model, _| {
            model.replace_view_position(self.position_range, range)
        });
    }

    fn finish_position_change(&mut self, cx: &mut Context<Self>) {
        self.prune_auto_closed(cx);
        self.dismiss_completion();
        self.reveal_caret = true;
        self.ensure_cursor_visible();
        self.clamp_scroll();
        self.sync_view_position(cx);
        cx.notify();
    }

    pub(crate) fn break_history_group(&self, cx: &mut App) {
        self.model
            .update(cx, |model, _| model.break_history_group());
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

    pub(crate) fn history_view_state(&self, model: &BufferModel) -> HistoryViewState {
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
    pub(super) fn visible_rows(&self) -> Range<usize> {
        let count = self.wrap.row_count();
        let first = (self.scroll_top.floor() as usize).min(count);
        let rows = self.viewport_rows.ceil() as usize + 1;
        first..(first + rows).min(count)
    }

    /// The lines with a row in [`Self::visible_rows`].
    pub(super) fn visible_lines(&self) -> Range<usize> {
        let rows = self.visible_rows();
        if rows.is_empty() {
            return self.line_count..self.line_count;
        }
        self.wrap.position(rows.start).0..self.wrap.position(rows.end - 1).0 + 1
    }

    /// The row the caret shows on.
    pub(super) fn caret_row(&self) -> usize {
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
    fn selection_range(&self) -> Option<((usize, usize), (usize, usize))> {
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
        cx: &mut Context<Self>,
    ) {
        self.dismiss_completion();
        let model = self.model.clone();
        model.update(cx, |model, cx| {
            let changed = if let Some(kind) = grouping {
                model.replace_grouped_from_view(
                    range,
                    text,
                    EditGrouping {
                        context: self.history_context,
                        kind,
                    },
                    view,
                )
            } else {
                model.replace_from_view(range, text, view)
            }
            .unwrap_or(false);
            if changed {
                cx.notify();
            }
        });
        let model = model.read(cx);
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
        cx: &mut Context<Self>,
    ) {
        if edits.is_empty() || !self.model.read(cx).is_editable() {
            return;
        }
        self.break_history_group(cx);
        self.dismiss_completion();
        let view = HistoryViewChange {
            before: self.history_view_state(self.model.read(cx)),
            after: HistoryViewState {
                context: self.history_context,
                anchor,
                caret,
            },
        };
        let model = self.model.clone();
        let changed = model.update(cx, |model, cx| {
            let changed = model.apply_edits_from_view(edits, view).unwrap_or(false);
            if changed {
                cx.notify();
            }
            changed
        });
        if !changed {
            return;
        }
        let model = model.read(cx);
        self.seen_revision = model.revision();
        self.refresh(model, None);
        (self.anchor_line, self.anchor_col) = flat_byte_to_position(model, anchor);
        (self.cursor_line, self.cursor_col) = flat_byte_to_position(model, caret);
        self.has_selection = anchor != caret;
        self.marked_range_utf16 = None;
        self.preferred_column = None;
        self.finish_position_change(cx);
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
        cx: &mut Context<Self>,
    ) {
        if !self.model.read(cx).is_editable() {
            return;
        }
        self.preferred_column = None;
        let model = self.model.read(cx);
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
            cx,
        );
        self.marked_range_utf16 = None;
        (self.cursor_line, self.cursor_col) =
            flat_byte_to_position(self.model.read(cx), after_offset);
        self.has_selection = false;
        self.finish_position_change(cx);
    }

    /// Convert an optional platform UTF-16 range to bytes.
    fn bytes_for_utf16(&self, range: Option<Range<usize>>, cx: &App) -> Option<Range<usize>> {
        let model = self.model.read(cx);
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

/// Lines a syntax highlight query covers beyond the requested ones on each
/// side.
const SYNTAX_OVERSCAN: usize = 16;

#[cfg(test)]
mod tests;
