//! A center tab comparing two texts.
//!
//! The view presents an original and a modified side, each an ordinary
//! [`EditorView`]. The original is a snapshot: a private read-only buffer with
//! a label and a language, outside document identity like a text frame's. The
//! modified side is a snapshot or a document, an editable view of the
//! document's shared buffer. The bands follow edits to either side.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use gpui::{prelude::*, *};

use knot_model::diff::{
    DiffOptions, DiffSide, LineHunk, adjacent_hunk, hunk_decorations, line_hunks, map_line,
};
use knot_model::editor::{Block, BlockLine, Reveal};
use knot_model::workbench::{DiffViewId, DocumentId};
use knot_protocol::{ExtensionId, ExtensionKey, ExtensionLifecycleId};

use super::convert::ThemePaint as _;
use super::editor::{EditorView, Scrolled};
use super::icons::AppIcon;
use super::model::{Buffer, BufferModel};
use super::settings::{self, DiffLayout};
use super::theme::{color, theme};
use super::zoom;
use knot_model::icons::IconDefinition;
use knot_model::theme::color as key;

/// Owns the bands a diff view puts on its editors.
const BAND_OWNER: ExtensionKey =
    ExtensionKey::new(ExtensionId::new(u64::MAX), ExtensionLifecycleId::new(1));

/// The key context of a diff's editors, for bindings that apply only
/// in a diff.
pub(crate) const DIFF_KEY_CONTEXT: &str = "diff";

/// How long after an edit the bands are recomputed, as the change gutter's.
const REDIFF_DELAY: Duration = Duration::from_millis(150);

static NEXT_DIFF_ID: AtomicU64 = AtomicU64::new(1);

/// The identity of a new diff view.
pub(crate) fn allocate_diff_id() -> DiffViewId {
    DiffViewId(NEXT_DIFF_ID.fetch_add(1, Ordering::Relaxed))
}

/// What a snapshot side shows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SnapshotSpec {
    pub label: String,
    /// The language the text is highlighted as, when it has one.
    pub language: Option<String>,
    pub text: String,
}

/// What the modified side shows.
#[derive(Clone)]
pub(crate) enum ModifiedSource {
    Snapshot(SnapshotSpec),
    /// The live buffer of an application document.
    Document {
        id: DocumentId,
        label: String,
        model: Entity<Buffer>,
    },
}

impl From<SnapshotSpec> for ModifiedSource {
    fn from(spec: SnapshotSpec) -> Self {
        Self::Snapshot(spec)
    }
}

struct Side {
    label: String,
    language: Option<String>,
    /// The document the side edits, when it is not a snapshot.
    document: Option<DocumentId>,
    model: Entity<Buffer>,
    editor: Entity<EditorView>,
}

impl Side {
    fn snapshot(spec: SnapshotSpec, cx: &mut Context<DiffView>) -> Self {
        let model = cx.new(|_| Buffer::new(BufferModel::from_read_only_text(spec.text)));
        if let Some(language) = spec.language.clone() {
            super::languages::set_language(&model, language, cx);
        }
        let editor = cx.new(|cx| EditorView::new(model.clone(), cx));
        Self {
            label: spec.label,
            language: spec.language,
            document: None,
            model,
            editor,
        }
    }

    fn document(
        id: DocumentId,
        label: String,
        model: Entity<Buffer>,
        cx: &mut Context<DiffView>,
    ) -> Self {
        let editor = cx.new(|cx| EditorView::new(model.clone(), cx));
        Self {
            label,
            language: None,
            document: Some(id),
            model,
            editor,
        }
    }

    fn spec(&self, cx: &App) -> SnapshotSpec {
        SnapshotSpec {
            label: self.label.clone(),
            language: self.language.clone(),
            text: self.model.read(cx).text(),
        }
    }
}

/// The editors of a diff and which one commands act on.
#[derive(Clone)]
pub(crate) struct DiffCommandTarget {
    editors: [Entity<EditorView>; 2],
    focused: Rc<Cell<DiffSide>>,
}

impl DiffCommandTarget {
    pub(crate) fn editor(&self) -> &Entity<EditorView> {
        &self.editors[usize::from(self.focused.get() == DiffSide::Modified)]
    }
}

pub(crate) struct DiffView {
    original: Side,
    modified: Side,
    /// The revisions of both sides the bands describe or a pending rediff is
    /// for.
    requested: (u64, u64),
    pending: Option<Task<()>>,
    /// The hunks between the sides as of the revisions the bands describe.
    hunks: Vec<LineHunk>,
    /// The revisions of both sides `hunks` describes.
    hunk_revisions: (u64, u64),
    layout: DiffLayout,
    /// The modified editor takes focus on the next render, as the layout
    /// change removed the one that had it.
    refocus: bool,
    /// Each editor's scroll as of the last sync, to tell which one moved.
    synced: [f32; 2],
    /// The side whose editor last had focus, which hunk commands move.
    focused: Rc<Cell<DiffSide>>,
    /// The modified side's editor refuses edits.
    locked: bool,
    /// Watches each editor's focus; set up on the first render, which has
    /// the window.
    _focus_subscriptions: Vec<Subscription>,
    _subscriptions: Vec<Subscription>,
}

/// How wide the strip between the editors is, which joins each hunk's lines.
const CONNECTOR_WIDTH: f32 = 32.;

impl DiffView {
    pub(crate) fn new(
        original: SnapshotSpec,
        modified: impl Into<ModifiedSource>,
        cx: &mut Context<Self>,
    ) -> Self {
        let original = Side::snapshot(original, cx);
        let modified = match modified.into() {
            ModifiedSource::Snapshot(spec) => Side::snapshot(spec, cx),
            ModifiedSource::Document { id, label, model } => Side::document(id, label, model, cx),
        };
        let mut subscriptions = Vec::new();
        for side in [&original, &modified] {
            subscriptions.push(cx.observe(&side.model, |this, _, cx| this.rediff_if_changed(cx)));
            let editor = side.editor.read(cx).editor().clone();
            subscriptions.push(cx.observe(&editor, |this, _, cx| this.sync_scroll(cx)));
            subscriptions
                .push(cx.subscribe(&editor, |this, _, _: &Scrolled, cx| this.sync_scroll(cx)));
        }
        // The original's syntax can arrive after its text, which recolors the
        // lines shown above the modified text.
        subscriptions.push(cx.observe(&original.model, |this, _, cx| this.refresh_blocks(cx)));
        for side in [&original, &modified] {
            side.editor
                .read(cx)
                .editor()
                .clone()
                .update(cx, |editor, _| {
                    editor.set_key_context(BAND_OWNER, DIFF_KEY_CONTEXT, true)
                });
        }
        let mut view = Self {
            requested: (
                original.model.read(cx).revision(),
                modified.model.read(cx).revision(),
            ),
            original,
            modified,
            pending: None,
            hunks: Vec::new(),
            hunk_revisions: (0, 0),
            layout: settings::diff_layout(cx),
            refocus: false,
            synced: [0.; 2],
            focused: Rc::new(Cell::new(DiffSide::Modified)),
            locked: false,
            _focus_subscriptions: Vec::new(),
            _subscriptions: subscriptions,
        };
        if view.layout == DiffLayout::Inline {
            view.focused.set(DiffSide::Modified);
        }
        view.rediff(None, cx);
        view
    }

    /// The document the modified side edits, if it is one.
    pub(crate) fn document(&self) -> Option<DocumentId> {
        self.modified.document
    }

    fn revisions(&self, cx: &App) -> (u64, u64) {
        (
            self.original.model.read(cx).revision(),
            self.modified.model.read(cx).revision(),
        )
    }

    /// Rediff after a debounce when an edit changed either side. Buffers
    /// also notify for syntax updates, which leave the revisions alone.
    fn rediff_if_changed(&mut self, cx: &mut Context<Self>) {
        let revisions = self.revisions(cx);
        if revisions != self.requested {
            self.requested = revisions;
            self.rediff(Some(REDIFF_DELAY), cx);
        }
    }

    /// Diff the sides off the foreground after `delay`, then band both
    /// editors if neither side has changed since. A newer request replaces a
    /// pending one.
    fn rediff(&mut self, delay: Option<Duration>, cx: &mut Context<Self>) {
        self.pending = Some(cx.spawn(async move |this, cx| {
            if let Some(delay) = delay {
                cx.background_executor().timer(delay).await;
            }
            let Ok((revisions, old, new)) = this.update(cx, |this, cx| {
                (
                    this.revisions(cx),
                    this.original.model.read(cx).text(),
                    this.modified.model.read(cx).text(),
                )
            }) else {
                return;
            };
            let (hunks, old, new) = cx
                .background_spawn(async move {
                    let hunks = line_hunks(&old, &new, DiffOptions::default());
                    (hunks, old, new)
                })
                .await;
            this.update(cx, |this, cx| {
                if this.revisions(cx) != revisions {
                    return;
                }
                this.hunks = hunks.clone();
                this.hunk_revisions = revisions;
                for (side, text, kind) in [
                    (&this.original, &old, DiffSide::Original),
                    (&this.modified, &new, DiffSide::Modified),
                ] {
                    let items = hunk_decorations(&hunks, kind, text);
                    let editor = side.editor.read(cx).editor().clone();
                    let revision = side.model.read(cx).revision();
                    editor.update(cx, |editor, cx| {
                        let _ = editor.set_decorations(BAND_OWNER, &items, revision, cx);
                    });
                }
                this.refresh_blocks(cx);
            })
            .ok();
        }));
    }

    /// Show each hunk's original lines above the modified text, in the inline
    /// layout, once the hunks describe the current texts.
    fn refresh_blocks(&mut self, cx: &mut Context<Self>) {
        if self.layout != DiffLayout::Inline || self.hunk_revisions != self.revisions(cx) {
            return;
        }
        let original = self.original.model.read(cx);
        let blocks = self
            .hunks
            .iter()
            .filter(|hunk| !hunk.old.is_empty())
            .map(|hunk| Block {
                line: hunk.new.start as usize,
                lines: (hunk.old.start..hunk.old.end)
                    .map(|line| {
                        let range = original.line_range(line as usize);
                        let text = original.read(range.clone());
                        let text = text.trim_end_matches(['\n', '\r']);
                        BlockLine {
                            highlights: original
                                .syntax_highlights(range.start..range.start + text.len())
                                .into_iter()
                                .map(|(span, highlight)| {
                                    (span.start - range.start..span.end - range.start, highlight)
                                })
                                .collect(),
                            text: text.to_owned(),
                            key: Some(key::DIFF_REMOVED_BACKGROUND.to_owned()),
                        }
                    })
                    .collect(),
            })
            .collect();
        self.set_blocks(blocks, cx);
    }

    fn set_blocks(&self, blocks: Vec<Block>, cx: &mut Context<Self>) {
        let editor = self.modified.editor.read(cx).editor().clone();
        editor.update(cx, |editor, cx| editor.set_blocks(blocks, cx));
    }

    /// How the texts are laid out.
    #[cfg(test)]
    pub(crate) fn layout(&self) -> DiffLayout {
        self.layout
    }

    /// Lay the texts out side by side or inline. Inline shows only the
    /// modified editor, so it is the one hunk commands act on.
    pub(crate) fn set_layout(&mut self, layout: DiffLayout, cx: &mut Context<Self>) {
        if layout == self.layout {
            return;
        }
        self.layout = layout;
        match layout {
            DiffLayout::Inline => {
                self.refocus = self.focused.replace(DiffSide::Modified) == DiffSide::Original;
                self.refresh_blocks(cx);
            }
            DiffLayout::SideBySide => {
                self.set_blocks(Vec::new(), cx);
                // The modified editor is the one to follow, as the original's
                // scroll was not tracked while it was hidden.
                self.synced[1] = f32::NAN;
                self.sync_scroll(cx);
            }
        }
        cx.notify();
    }

    /// Switch to the other layout.
    pub(crate) fn toggle_layout(&mut self, cx: &mut Context<Self>) {
        self.set_layout(
            match self.layout {
                DiffLayout::SideBySide => DiffLayout::Inline,
                DiffLayout::Inline => DiffLayout::SideBySide,
            },
            cx,
        );
    }

    fn scrolls(&self, cx: &App) -> [f32; 2] {
        [&self.original, &self.modified]
            .map(|side| side.editor.read(cx).editor().read(cx).scroll_top())
    }

    /// Scroll the other editor to where the one that moved is, through the
    /// hunk mapping. Scrolling the other notifies this again, which finds
    /// both where they were left and stops there.
    fn sync_scroll(&mut self, cx: &mut Context<Self>) {
        if self.layout == DiffLayout::Inline {
            return;
        }
        let scrolls = self.scrolls(cx);
        if scrolls == self.synced {
            return;
        }
        let (from, to, side) = if scrolls[0] != self.synced[0] {
            (&self.original, &self.modified, DiffSide::Original)
        } else {
            (&self.modified, &self.original, DiffSide::Modified)
        };
        let line = from.editor.read(cx).editor().read(cx).scroll_line();
        let target = map_line(&self.hunks, side, line);
        to.editor
            .read(cx)
            .editor()
            .clone()
            .update(cx, |editor, cx| editor.scroll_to_line(target, cx));
        self.synced = self.scrolls(cx);
        cx.notify();
    }

    /// The strip between the editors, with a band joining each hunk's lines
    /// on one side to its lines on the other.
    fn connectors(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let view = cx.entity();
        let [original, modified] = [&self.original, &self.modified].map(|side| side.editor.clone());
        canvas(
            |_, _, _| (),
            move |bounds, (), window, cx| {
                let (hunks, paints) = {
                    let theme = theme(cx);
                    let paints = [
                        theme.paint_color(key::DIFF_ADDED_BACKGROUND),
                        theme.paint_color(key::DIFF_MODIFIED_BACKGROUND),
                        theme.paint_color(key::DIFF_REMOVED_BACKGROUND),
                    ];
                    (view.read(cx).hunks.clone(), paints)
                };
                let (left, right) = (original.read(cx), modified.read(cx));
                let text = left.text_bounds();
                let mask = Bounds {
                    origin: point(bounds.origin.x, text.origin.y),
                    size: size(bounds.size.width, text.size.height),
                };
                window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
                    let (x0, x1) = (bounds.left(), bounds.right());
                    for hunk in &hunks {
                        let [a, b] = [
                            (
                                left.line_top(hunk.old.start as usize, cx),
                                left.line_top(hunk.old.end as usize, cx),
                            ),
                            (
                                right.line_top(hunk.new.start as usize, cx),
                                right.line_top(hunk.new.end as usize, cx),
                            ),
                        ];
                        if (a.1 < mask.top() && b.1 < mask.top())
                            || (a.0 > mask.bottom() && b.0 > mask.bottom())
                        {
                            continue;
                        }
                        let paint = if hunk.old.is_empty() {
                            paints[0]
                        } else if hunk.new.is_empty() {
                            paints[2]
                        } else {
                            paints[1]
                        };
                        let mut path = PathBuilder::fill();
                        path.move_to(point(x0, a.0));
                        path.line_to(point(x1, b.0));
                        path.line_to(point(x1, b.1));
                        path.line_to(point(x0, a.1));
                        path.close();
                        if let Ok(path) = path.build() {
                            window.paint_path(path, paint);
                        }
                    }
                });
            },
        )
        .w(px(CONNECTOR_WIDTH))
        .h_full()
    }

    /// Replace the text of a snapshot side; a document side is left alone.
    /// Returns whether the side is a snapshot.
    pub(crate) fn replace_text(
        &mut self,
        side: DiffSide,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let side = match side {
            DiffSide::Original => &self.original,
            DiffSide::Modified => &self.modified,
        };
        if side.document.is_some() {
            return false;
        }
        side.model.update(cx, |model, cx| {
            if model.replace_generated_text(text) {
                cx.notify();
            }
        });
        true
    }

    /// Put the focused side's caret on its first line of the hunk after (or
    /// before) the caret, wrapping at the ends. Without hunks it stays.
    pub(crate) fn go_to_hunk(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let focused = self.focused.get();
        let editor = self.command_editor().read(cx).editor().clone();
        let line = editor.read(cx).caret().0 as u32;
        let Some(target) = adjacent_hunk(&self.hunks, focused, line, forward) else {
            return false;
        };
        editor.update(cx, |editor, cx| {
            editor.move_to_line(target as usize, Reveal::Center, cx)
        });
        true
    }

    /// Whether the modified side's editor refuses edits.
    pub(crate) fn is_locked(&self) -> bool {
        self.locked
    }

    /// Make the modified side's editor refuse edits, or accept them again.
    /// Only this view is affected; the document's other views stay editable.
    /// A snapshot side is read-only already, so this reports whether the side
    /// is a document.
    pub(crate) fn set_locked(&mut self, locked: bool, cx: &mut Context<Self>) -> bool {
        if self.modified.document.is_none() {
            return false;
        }
        self.locked = locked;
        self.modified
            .editor
            .read(cx)
            .editor()
            .clone()
            .update(cx, |editor, _| editor.set_read_only(locked));
        cx.notify();
        true
    }

    /// Put the modified side's caret at the start of `line`, 0-based.
    pub(crate) fn go_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let editor = self.modified.editor.read(cx).editor().clone();
        editor.update(cx, |editor, cx| {
            editor.move_to_line(line, Reveal::Center, cx)
        });
    }

    /// The tab title, from both labels.
    pub(crate) fn title(&self) -> String {
        format!("{} ↔ {}", self.original.label, self.modified.label)
    }

    /// The sides as they are now, to open the same comparison again. A
    /// document side stays that document.
    pub(crate) fn specs(&self, cx: &App) -> (SnapshotSpec, ModifiedSource) {
        let modified = match self.modified.document {
            Some(id) => ModifiedSource::Document {
                id,
                label: self.modified.label.clone(),
                model: self.modified.model.clone(),
            },
            None => ModifiedSource::Snapshot(self.modified.spec(cx)),
        };
        (self.original.spec(cx), modified)
    }

    /// The editor that commands act on: the side that last had focus.
    pub(crate) fn command_editor(&self) -> &Entity<EditorView> {
        match self.focused.get() {
            DiffSide::Original => &self.original.editor,
            DiffSide::Modified => &self.modified.editor,
        }
    }

    /// Which editor commands act on, readable without the view.
    pub(crate) fn command_target(&self) -> DiffCommandTarget {
        DiffCommandTarget {
            editors: [self.original.editor.clone(), self.modified.editor.clone()],
            focused: self.focused.clone(),
        }
    }

    /// Treat `side` as the one with focus, as its editor gaining focus does.
    #[cfg(test)]
    pub(crate) fn set_focused_side(&mut self, side: DiffSide) {
        self.focused.set(side);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn editors(&self) -> [&Entity<EditorView>; 2] {
        [&self.original.editor, &self.modified.editor]
    }
}

impl Focusable for DiffView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.command_editor().read(cx).focus_handle(cx)
    }
}

impl DiffView {
    /// The bar above the sides: previous and next hunk, and a lock for the
    /// modified side when it is a document.
    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = theme(cx);
        let text = theme.paint_color(color::TEXT);
        let hover = theme.paint_color(color::BUTTON_HOVER_BACKGROUND);
        let active = theme.paint_color(color::LIST_SELECTED_BACKGROUND);
        let button = |id: &'static str,
                      glyph: &'static str,
                      pressed: bool,
                      on_click: fn(&mut DiffView, &mut Context<DiffView>)| {
            div()
                .id(id)
                .p(zoom::rems(3.))
                .rounded(zoom::rems(4.))
                .text_color(text)
                .cursor_pointer()
                .when(pressed, |button| button.bg(active))
                .hover(|style| style.bg(hover))
                .child(AppIcon::new(IconDefinition::lucide(glyph)))
                .on_click(cx.listener(move |this, _, _, cx| on_click(this, cx)))
        };
        div()
            .id("diff-toolbar")
            .flex()
            .flex_row()
            .items_center()
            .gap(zoom::rems(2.))
            .px(zoom::rems(6.))
            .py(zoom::rems(2.))
            .child(button(
                "diff-previous-hunk",
                "arrow-up",
                false,
                |this, cx| {
                    this.go_to_hunk(false, cx);
                },
            ))
            .child(button("diff-next-hunk", "arrow-down", false, |this, cx| {
                this.go_to_hunk(true, cx);
            }))
            .child(button(
                "diff-layout",
                match self.layout {
                    DiffLayout::SideBySide => "rows-2",
                    DiffLayout::Inline => "columns-2",
                },
                false,
                |this, cx| this.toggle_layout(cx),
            ))
            .when(self.modified.document.is_some(), |bar| {
                let glyph = if self.locked { "lock" } else { "lock-open" };
                bar.child(button("diff-lock", glyph, self.locked, |this, cx| {
                    this.set_locked(!this.locked, cx);
                }))
            })
    }
}

impl Render for DiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self._focus_subscriptions.is_empty() {
            for (side, kind) in [
                (&self.original, DiffSide::Original),
                (&self.modified, DiffSide::Modified),
            ] {
                let handle = side.editor.focus_handle(cx);
                self._focus_subscriptions.push(cx.on_focus_in(
                    &handle,
                    window,
                    move |this, _, _| this.focused.set(kind),
                ));
            }
        }
        if std::mem::take(&mut self.refocus) {
            let handle = self.modified.editor.focus_handle(cx);
            window.focus(&handle, cx);
        }
        let border = theme(cx).paint_color(color::BORDER);
        let muted = theme(cx).paint_color(color::TEXT_MUTED);
        let side = |label: &str, editor: &Entity<EditorView>| {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(
                    div()
                        .px(zoom::rems(8.))
                        .py(zoom::rems(2.))
                        .text_size(zoom::rems(12.))
                        .text_color(muted)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label.to_owned()),
                )
                .child(div().flex_1().min_h_0().child(editor.clone()))
        };
        let toolbar = self.toolbar(cx);
        let content = div().flex_1().min_h_0().flex().flex_row();
        let content = match self.layout {
            DiffLayout::Inline => content.child(side(&self.title(), &self.modified.editor)),
            DiffLayout::SideBySide => content
                .child(side(&self.original.label, &self.original.editor))
                .child(div().w(px(1.)).h_full().bg(border))
                .child(self.connectors(cx))
                .child(div().w(px(1.)).h_full().bg(border))
                .child(side(&self.modified.label, &self.modified.editor)),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(toolbar)
            .child(div().h(px(1.)).w_full().bg(border))
            .child(content)
    }
}

/// The command that moves to the next changed hunk of a diff.
pub(crate) const NEXT_HUNK_COMMAND: &str = "diff.next-hunk";
/// The command that moves to the previous changed hunk of a diff.
pub(crate) const PREVIOUS_HUNK_COMMAND: &str = "diff.previous-hunk";

/// The command that switches a diff between side by side and inline.
pub(crate) const TOGGLE_LAYOUT_COMMAND: &str = "diff.toggle-layout";

/// The command that toggles whether a diff's modified side refuses edits.
pub(crate) const TOGGLE_READ_ONLY_COMMAND: &str = "diff.toggle-read-only";

/// Switch a diff tab between side by side and inline; `diff.layout` is how new
/// diff tabs open.
#[crate::app::commands::declarations::command(TOGGLE_LAYOUT_COMMAND, title = "Toggle Diff Layout", description = "Switch the diff between side by side and inline.", participant = Editor)]
pub(crate) async fn toggle_layout(
    context: crate::app::commands::declarations::CommandContext,
    (): (),
) -> Result<(), knot_protocol::CommandOutcome> {
    use knot_protocol::CommandOutcome;
    context
        .foreground(move |target, cx| {
            let view = target.diff_view(cx).ok_or(CommandOutcome::InvalidTarget)?;
            view.update(cx, |view, cx| view.toggle_layout(cx));
            Ok(())
        })
        .await
}

/// Lock or unlock the modified side of a diff tab. A diff whose modified side
/// is a snapshot is read-only already, so the command is unavailable there.
#[crate::app::commands::declarations::command(TOGGLE_READ_ONLY_COMMAND, title = "Toggle Diff Read-Only", description = "Make the diff's modified side refuse edits, or accept them again.", participant = Editor)]
pub(crate) async fn toggle_read_only(
    context: crate::app::commands::declarations::CommandContext,
    (): (),
) -> Result<(), knot_protocol::CommandOutcome> {
    use knot_protocol::CommandOutcome;
    context
        .foreground(move |target, cx| {
            let view = target.diff_view(cx).ok_or(CommandOutcome::InvalidTarget)?;
            if view.update(cx, |view, cx| view.set_locked(!view.is_locked(), cx)) {
                Ok(())
            } else {
                Err(CommandOutcome::Unavailable)
            }
        })
        .await
}

/// Move the modified side's caret to the next or previous hunk, wrapping.
/// Only a diff tab has hunks; a diff without any leaves the caret.
#[crate::app::commands::declarations::command(NEXT_HUNK_COMMAND, title = "Go to Next Difference", description = "Move the cursor to the next hunk of the diff, wrapping at the end.", participant = Editor)]
#[crate::app::commands::declarations::command(PREVIOUS_HUNK_COMMAND, title = "Go to Previous Difference", description = "Move the cursor to the previous hunk of the diff, wrapping at the start.", participant = Editor)]
pub(crate) async fn go_to_hunk(
    context: crate::app::commands::declarations::CommandContext,
    (): (),
) -> Result<(), knot_protocol::CommandOutcome> {
    use knot_protocol::CommandOutcome;
    let forward = context.name == NEXT_HUNK_COMMAND;
    context
        .foreground(move |target, cx| {
            let view = target.diff_view(cx).ok_or(CommandOutcome::InvalidTarget)?;
            if view.update(cx, |view, cx| view.go_to_hunk(forward, cx)) {
                Ok(())
            } else {
                Err(CommandOutcome::Unavailable)
            }
        })
        .await
}
