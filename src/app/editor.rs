//! Native editors over an authoritative shared buffer model.
//!
//! An [`Editor`] entity owns one [`EditorState`] over a shared [`Buffer`]:
//! it resolves the editor's settings, runs the state's operations against the
//! buffer, carries out the effects they return, and notifies. An
//! [`EditorView`] renders it and handles pointer and platform text input;
//! pointer hit testing uses shaped coordinates.

mod bars;
mod completion;
mod gutter;
mod input;
mod layout;
pub(super) mod native_commands;
mod paint;
mod view;

pub(crate) use bars::BarFieldEdited;
pub use view::EditorView;

use std::{
    cell::RefCell,
    ops::{Deref, DerefMut, Range},
    sync::Arc,
};

use gutter::GutterLayout;
use layout::*;

use gpui::{prelude::*, *};
use unicode_segmentation::UnicodeSegmentation;

use knot_protocol::{
    BufferHandle, ByteRange, CompletionProviderRegistrationId, CompletionRequest,
    CompletionResponse, DecorationStyle,
};

pub(crate) use knot_model::editor::*;

use super::commands::dispatch as commands;
use super::typography::text_font as make_font;

use super::{
    CompletionAccept, CompletionDismiss, CompletionNext, CompletionPrevious, EDITOR_KEY_CONTEXT,
    completion::CompletionProviderRegistration,
    model::{Buffer, BufferModel, ContributionSource},
    settings::{self, ApplicationSettings, SettingsScope},
    theme::{Highlight, Theme, color, theme},
};

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

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct EditorContributionAction {
    pub command: String,
    pub source: ContributionSource,
    pub range: ByteRange,
    pub model: Entity<Buffer>,
    pub window: AnyWindowHandle,
    pub focus: WeakFocusHandle,
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

/// Lines a syntax highlight query covers beyond the requested ones on each
/// side.
const SYNTAX_OVERSCAN: usize = 16;

/// The entity of one editor: its [`EditorState`], which it derefs to, over a
/// shared buffer entity. Operations that read the buffer or change the editor
/// take the gpui context; the state's own queries are reached through deref.
pub struct Editor {
    state: EditorState,
    model: Entity<Buffer>,
    /// The in-flight provider requests of the open completion, by its
    /// generation; dropped when the completion closes or moves on.
    completion_tasks: Vec<(u64, CompletionProviderRegistrationId, Task<()>)>,
    /// The text field behind each field of the open bars.
    bar_inputs: Vec<bars::BarInput>,
    _model_subscription: Subscription,
    _release_subscription: Subscription,
}

impl Deref for Editor {
    type Target = EditorState;

    fn deref(&self) -> &EditorState {
        &self.state
    }
}

impl DerefMut for Editor {
    fn deref_mut(&mut self) -> &mut EditorState {
        &mut self.state
    }
}

impl Editor {
    /// Build an editor over `model` whose settings resolve for view `kind`.
    pub(crate) fn new(
        model: Entity<Buffer>,
        kind: impl Into<Arc<str>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let kind: Arc<str> = kind.into();
        let settings = editor_settings(&kind, model.read(cx), cx);
        let state = model.update(cx, |buffer, _| EditorState::new(kind, settings, buffer));
        let model_subscription = cx.observe(&model, |this, _, cx| {
            this.update_state(cx, |state, buffer| state.buffer_changed(buffer));
        });
        let release_subscription = cx.on_release(|this, cx| {
            this.model
                .update(cx, |buffer, _| this.state.release(buffer));
        });
        Self {
            state,
            model,
            completion_tasks: Vec::new(),
            bar_inputs: Vec::new(),
            _model_subscription: model_subscription,
            _release_subscription: release_subscription,
        }
    }

    /// Replace the state with a fresh one over the same buffer, as a new
    /// editor would start: no selection, scroll, folds, completion, or
    /// lifecycle presentation survives. For reusing an editor in tests.
    #[cfg(test)]
    pub(crate) fn reset_state(&mut self, cx: &mut Context<Self>) {
        let model = self.model.clone();
        model.update(cx, |buffer, _| self.state.release(buffer));
        let kind: Arc<str> = Arc::from(self.state.view_kind());
        let settings = editor_settings(&kind, model.read(cx), cx);
        self.state = model.update(cx, |buffer, _| EditorState::new(kind, settings, buffer));
        self.completion_tasks.clear();
        self.bar_inputs.clear();
        cx.notify();
    }

    pub(crate) fn model(&self) -> &Entity<Buffer> {
        &self.model
    }

    /// Run `operation` on the state against the buffer with freshly resolved
    /// settings, then notify the buffer when its text changed, carry out the
    /// queued effects, and notify this editor.
    fn update_state<R>(
        &mut self,
        cx: &mut Context<Self>,
        operation: impl FnOnce(&mut EditorState, &mut BufferModel) -> R,
    ) -> R {
        let settings = editor_settings(self.state.view_kind(), self.model.read(cx), cx);
        self.state.set_settings(settings);
        let model = self.model.clone();
        let result = model.update(cx, |buffer, cx| {
            let revision = buffer.revision();
            let result = operation(&mut self.state, buffer);
            if buffer.revision() != revision {
                cx.notify();
            }
            result
        });
        self.run_effects(cx);
        let generation = self.state.completion_generation();
        self.completion_tasks
            .retain(|(task, _, _)| Some(*task) == generation);
        cx.notify();
        result
    }

    fn run_effects(&mut self, cx: &mut Context<Self>) {
        for effect in self.state.take_effects() {
            match effect {
                EditorEffect::WriteClipboard(text) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
                EditorEffect::SetDocumentSoftWrap { document, wrap } => {
                    settings::set_document_soft_wrap(document, wrap, cx);
                }
                EditorEffect::SetDocumentInsertSpaces { document, spaces } => {
                    settings::set_document_insert_spaces(document, spaces, cx);
                    crate::app::application_observation::changed(cx);
                }
                EditorEffect::DetectIndentation { document } => {
                    settings::reset_document_indentation(document, cx);
                    crate::app::documents::detect_indentation(&self.model, cx);
                    crate::app::application_observation::changed(cx);
                }
            }
        }
    }

    /// Select from `anchor` to `caret` directly, for a test; see
    /// [`EditorState::place_for_test`].
    #[cfg(test)]
    fn place_for_test(
        &mut self,
        anchor: (usize, usize),
        caret: (usize, usize),
        cx: &mut Context<Self>,
    ) {
        self.update_state(cx, |editor, buffer| {
            editor.place_for_test(anchor, caret, buffer)
        });
    }

    #[cfg(test)]
    fn text(&self, cx: &App) -> String {
        self.model.read(cx).text()
    }

    /// The settings scope of this editor's view kind and its buffer's
    /// language and document.
    pub(super) fn settings_scope(&self, model: &BufferModel, cx: &App) -> SettingsScope {
        settings_scope(self.state.view_kind(), model, cx)
    }

    /// Columns per tab stop for this editor's view kind and its buffer's
    /// language.
    pub(crate) fn tab_size(&self, model: &BufferModel, cx: &App) -> usize {
        settings::editor_tab_size(&self.settings_scope(model, cx), cx)
    }

    pub(crate) fn selected_byte_range(&self, cx: &App) -> Option<Range<usize>> {
        self.state.selected_byte_range(self.model.read(cx))
    }

    /// The restorable presentation state: selection ends and the top visible
    /// row as byte offsets, and horizontal scroll in pixels.
    pub(crate) fn view_state(&self, cx: &App) -> EditorViewState {
        self.state.view_state(self.model.read(cx))
    }

    /// Apply a restored presentation state; see
    /// [`EditorState::restore_view_state`].
    pub(crate) fn restore_view_state(&mut self, state: EditorViewState, cx: &mut Context<Self>) {
        self.update_state(cx, |editor, buffer| {
            editor.restore_view_state(state, buffer)
        });
    }

    /// The selection's anchor and caret as byte offsets; equal without a
    /// selection.
    pub(crate) fn selection(&self, cx: &App) -> (usize, usize) {
        self.state.selection(self.model.read(cx))
    }

    /// Set `owner`'s caret shape, or clear it with `None` to reveal the
    /// previous lifecycle's.
    pub(crate) fn set_caret_shape(
        &mut self,
        owner: knot_protocol::ExtensionKey,
        shape: Option<CaretShape>,
        cx: &mut Context<Self>,
    ) {
        self.state.set_caret_shape(owner, shape);
        cx.notify();
    }

    /// The command a text binding routes this editor's typed text to.
    pub(crate) fn text_command(&self, cx: &App) -> Option<knot_protocol::CommandName> {
        cx.try_global::<crate::app::commands::keymaps::ApplicationKeymaps>()?
            .0
            .resolve_text(self.state.view_kind(), &self.state.key_contexts())
    }

    /// Clear every key context, caret shape, bar, and decoration `owner` set.
    pub(crate) fn remove_owner_state(
        &mut self,
        owner: knot_protocol::ExtensionKey,
        cx: &mut Context<Self>,
    ) {
        let model = self.model.clone();
        model.update(cx, |buffer, _| self.state.remove_owner_state(owner, buffer));
        self.sync_bar_inputs(cx);
        cx.notify();
    }

    /// Replace `owner`'s decorations on this view at buffer revision
    /// `if_revision`.
    pub(crate) fn set_decorations(
        &mut self,
        owner: knot_protocol::ExtensionKey,
        items: &[knot_model::editor::ViewDecoration],
        if_revision: u64,
        cx: &mut Context<Self>,
    ) -> Result<(), knot_model::buffer::ContributionError> {
        let model = self.model.clone();
        model.update(cx, |buffer, _| {
            self.state
                .set_decorations(owner, items, if_revision, buffer)
        })?;
        cx.notify();
        Ok(())
    }

    /// Move the selection to byte offsets on character boundaries within the
    /// text. Like other caret moves, this ends the current typing undo group.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn set_selection(
        &mut self,
        anchor: usize,
        caret: usize,
        reveal: Reveal,
        cx: &mut Context<Self>,
    ) {
        self.update_state(cx, |editor, buffer| {
            editor.set_selection(anchor, caret, reveal, buffer)
        });
    }

    /// Move the selection to extension-named offsets; see
    /// [`EditorState::set_selection_checked`].
    pub(crate) fn set_selection_checked(
        &mut self,
        anchor: u64,
        caret: u64,
        reveal: Reveal,
        cx: &mut Context<Self>,
    ) -> bool {
        self.update_state(cx, |editor, buffer| {
            editor.set_selection_checked(anchor, caret, reveal, buffer)
        })
    }

    /// Select from `anchor` to `caret`, a caret alone when they are equal,
    /// as a click or drag does.
    pub(super) fn select_from(
        &mut self,
        anchor: (usize, usize),
        caret: (usize, usize),
        cx: &mut Context<Self>,
    ) {
        self.update_state(cx, |editor, buffer| {
            editor.select_from(anchor, caret, buffer)
        });
    }

    /// The buffer line the document row `row` belongs to and the byte column
    /// where the row starts in it; a row past the last is the last.
    pub(crate) fn row_start(&self, row: usize, cx: &App) -> (usize, usize) {
        self.state.row_start(row, self.model.read(cx))
    }

    /// The document row showing byte `column` of `line`; see
    /// [`EditorState::row_of_position`].
    pub(crate) fn row_of_position(&self, line: usize, column: usize, cx: &App) -> Option<usize> {
        self.state
            .row_of_position(line, column, self.model.read(cx))
    }

    /// Scroll by `rows`, negative upward, without moving the caret or
    /// revealing it; the scroll stays within the text.
    pub(crate) fn scroll_by_rows(&mut self, rows: i64, cx: &mut Context<Self>) {
        self.state.scroll_by_rows(rows);
        cx.notify();
    }

    /// Scroll by `rows`, which may be fractional, and `x` pixels sideways
    /// within `max_x`, without moving the caret, as the wheel does.
    pub(super) fn scroll_by(&mut self, rows: f32, x: f32, max_x: f32, cx: &mut Context<Self>) {
        self.state.scroll_by(rows, x, max_x);
        cx.notify();
    }

    pub(crate) fn break_history_group(&self, cx: &mut App) {
        self.model
            .update(cx, |model, _| model.break_history_group());
    }

    /// Run the native editing or movement command `command`; `false` when
    /// it is not one or does not apply. Paste reads the clipboard.
    pub(crate) fn execute_editing_command(
        &mut self,
        command: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if command == commands::PASTE_COMMAND {
            let text = cx.read_from_clipboard().and_then(|item| item.text());
            self.update_state(cx, |editor, buffer| editor.paste(text.as_deref(), buffer));
            return true;
        }
        self.update_state(cx, |editor, buffer| {
            editor.execute_editing_command(command, buffer)
        })
    }

    /// Open a completion at the caret; returns the requests to send its
    /// providers.
    pub(crate) fn start_completion(
        &mut self,
        handle: BufferHandle,
        providers: Vec<CompletionProviderRegistration>,
        cx: &mut Context<Self>,
    ) -> Vec<(CompletionProviderRegistration, CompletionRequest)> {
        self.update_state(cx, |editor, buffer| {
            editor.start_completion(handle, providers, buffer)
        })
    }

    pub(crate) fn apply_completion_response(
        &mut self,
        handle: BufferHandle,
        response: CompletionResponse,
        cx: &mut Context<Self>,
    ) -> bool {
        let applied = self
            .state
            .apply_completion_response(handle, response, self.model.read(cx));
        if applied {
            cx.notify();
        }
        applied
    }

    /// Keep `task`, a provider request of the completion of `generation`,
    /// while that completion stays open.
    pub(crate) fn retain_completion_task(
        &mut self,
        generation: u64,
        registration: CompletionProviderRegistrationId,
        task: Task<()>,
    ) {
        if self.state.completion_generation() == Some(generation) {
            self.completion_tasks.push((generation, registration, task));
        }
    }

    pub(crate) fn cancel_completion_provider(
        &mut self,
        registration: CompletionProviderRegistrationId,
        cx: &mut Context<Self>,
    ) {
        self.completion_tasks
            .retain(|(_, provider, _)| *provider != registration);
        self.state.cancel_completion_provider(registration);
        cx.notify();
    }

    fn move_completion_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.state.move_completion_selection(delta);
        cx.notify();
    }

    pub(super) fn accept_selected_completion(&mut self, cx: &mut Context<Self>) {
        self.update_state(cx, |editor, buffer| {
            editor.accept_selected_completion(buffer)
        });
    }

    fn dismiss_completion(&mut self, cx: &mut Context<Self>) {
        self.state.dismiss_completion();
        self.completion_tasks.clear();
        cx.notify();
    }

    /// Collapse the lines `lines`; see [`EditorState::fold`].
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn fold(&mut self, lines: Range<usize>, cx: &mut Context<Self>) -> bool {
        self.update_state(cx, |editor, buffer| editor.fold(lines, buffer))
    }

    /// Expand the collapsed fold over exactly `lines`, if there is one.
    pub(crate) fn unfold(&mut self, lines: &Range<usize>, cx: &mut Context<Self>) {
        self.update_state(cx, |editor, buffer| editor.unfold(lines, buffer));
    }

    /// Collapse the lines `start` through `last` inclusive; see
    /// [`EditorState::fold_through`].
    pub(crate) fn fold_through(&mut self, start: u64, last: u64, cx: &mut Context<Self>) -> bool {
        self.update_state(cx, |editor, buffer| {
            editor.fold_through(start, last, buffer)
        })
    }

    /// Expand the fold over the lines `start` through `last` inclusive.
    pub(crate) fn unfold_through(&mut self, start: u64, last: u64, cx: &mut Context<Self>) {
        self.update_state(cx, |editor, buffer| {
            editor.unfold_through(start, last, buffer)
        });
    }

    /// The regions the language's `folds` query or, without one, the text's
    /// indentation offers, as line ranges ordered by first line.
    pub(crate) fn fold_regions(&self, cx: &App) -> Arc<[Range<usize>]> {
        self.model.read(cx).fold_regions()
    }

    /// The foldable lines among `lines` that are not hidden, each with whether
    /// it starts a collapsed fold.
    pub(super) fn fold_marks(&self, lines: Range<usize>, cx: &App) -> Vec<(usize, bool)> {
        self.state.fold_marks(lines, self.model.read(cx))
    }

    pub(super) fn toggle_fold_at_line(
        &mut self,
        line: usize,
        recursive: bool,
        cx: &mut Context<Self>,
    ) {
        self.update_state(cx, |editor, buffer| {
            editor.toggle_fold_at_line(line, recursive, buffer)
        });
    }

    /// Select whole lines from a click on the line numbers; returns what a
    /// drag then extends from.
    pub(super) fn select_lines_from_gutter(
        &mut self,
        line: usize,
        extend: bool,
        cx: &mut Context<Self>,
    ) -> DragAnchor {
        self.update_state(cx, |editor, buffer| {
            editor.select_lines_from_gutter(line, extend, buffer)
        })
    }

    /// Insert typed or committed `text`; see [`EditorState::type_text`].
    pub(crate) fn type_text(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        self.update_state(cx, |editor, buffer| editor.type_text(range, text, buffer));
    }

    /// Mark composing `new_text`; see [`EditorState::mark_text`]. Text a
    /// binding claims never reaches the buffer, so neither does its
    /// composition; the commit arrives as ordinary text.
    pub(super) fn mark_text(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        cx: &mut Context<Self>,
    ) {
        if self.text_command(cx).is_some() {
            return;
        }
        self.update_state(cx, |editor, buffer| {
            editor.mark_text(range, new_text, new_selected_range, buffer)
        });
    }

    /// Commit the preedit marking without changing its text or selection.
    pub(super) fn unmark_text(&mut self, cx: &mut Context<Self>) {
        self.update_state(cx, |editor, buffer| editor.unmark_text(buffer));
    }

    /// The selection as document-wide UTF-16 offsets.
    fn selected_utf16(&self, cx: &App) -> UTF16Selection {
        let (range, reversed) = self.state.selected_utf16(self.model.read(cx));
        UTF16Selection { range, reversed }
    }

    /// Clean the text up for a save as the settings of this editor's document
    /// ask; see [`EditorState::clean_up_for_save`].
    pub(crate) fn clean_up_for_save(&mut self, cx: &mut Context<Self>) {
        self.update_state(cx, |editor, buffer| editor.clean_up_for_save(buffer));
    }

    /// Wrap lines to `wrap`, or stop wrapping with `None`.
    pub(super) fn set_wrap(&mut self, wrap: Option<WrapWidth>, cx: &App) {
        self.state.set_wrap(wrap, self.model.read(cx));
    }

    /// Lay out as a view `rows` rows tall and `columns` cells wide would,
    /// wrapping when `editor.softWrap` is on, for a test without a window.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn lay_out_for_test(&mut self, rows: f32, columns: usize, cx: &App) {
        let settings = editor_settings(self.state.view_kind(), self.model.read(cx), cx);
        self.state.set_settings(settings);
        self.state
            .lay_out_for_test(rows, columns, self.model.read(cx));
    }

    /// The display cells from the start of buffer line `line` to byte `column`
    /// in it, with tabs expanded; see [`EditorState::line_cells_before`].
    pub(crate) fn line_cells_before(&self, line: usize, column: usize, cx: &App) -> Option<usize> {
        let model = self.model.read(cx);
        EditorState::line_cells_before(line, column, self.tab_size(model, cx), model)
    }

    /// The byte column at display cell `cells` of `line`; see
    /// [`EditorState::line_column_at_cells`].
    pub(crate) fn line_column_at_cells(
        &self,
        line: usize,
        cells: usize,
        cx: &App,
    ) -> Option<usize> {
        let model = self.model.read(cx);
        EditorState::line_column_at_cells(line, cells, self.tab_size(model, cx), model)
    }

    /// The brackets highlighted at the caret.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn matched_brackets_for_test(&self, cx: &App) -> Option<[Range<usize>; 2]> {
        self.state.matched_brackets(self.model.read(cx))
    }
}

/// The settings scope of an editor of view `kind` over `model`.
fn settings_scope(kind: &str, model: &BufferModel, cx: &App) -> SettingsScope {
    SettingsScope {
        language: Some(crate::app::languages::effective_language(model, cx)),
        view: Some(kind.to_string()),
        document: model.document().map(|document| document.to_string()),
    }
}

/// The settings an editor of view `kind` over `model` follows.
fn editor_settings(kind: &str, model: &BufferModel, cx: &App) -> EditorSettings {
    let scope = settings_scope(kind, model, cx);
    EditorSettings {
        tab_size: settings::editor_tab_size(&scope, cx),
        insert_spaces: settings::editor_insert_spaces(&scope, cx),
        auto_closing_brackets: settings::editor_auto_closing_brackets(&scope, cx),
        auto_surround: settings::editor_auto_surround(&scope, cx),
        soft_wrap: settings::editor_soft_wrap(&scope, cx),
        trim_trailing_whitespace: settings::files_trim_trailing_whitespace(&scope, cx),
        insert_final_newline: settings::files_insert_final_newline(&scope, cx),
    }
}

#[cfg(test)]
mod tests;
