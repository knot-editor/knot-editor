//! A center tab comparing two texts.
//!
//! The view presents an original and a modified side, each an ordinary
//! [`EditorView`]. A side is a snapshot: a private read-only buffer with a
//! label and a language, outside document identity like a text frame's.

use std::sync::atomic::{AtomicU64, Ordering};

use gpui::{prelude::*, *};

use knot_model::diff::{DiffOptions, DiffSide, hunk_decorations, line_hunks};
use knot_model::workbench::DiffViewId;
use knot_protocol::{ExtensionId, ExtensionKey, ExtensionLifecycleId};

use super::convert::ThemePaint as _;
use super::editor::EditorView;
use super::model::{Buffer, BufferModel};
use super::theme::{color, theme};
use super::zoom;

/// Owns the bands a diff view puts on its editors.
const BAND_OWNER: ExtensionKey =
    ExtensionKey::new(ExtensionId::new(u64::MAX), ExtensionLifecycleId::new(1));

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

struct Side {
    label: String,
    language: Option<String>,
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

pub(crate) struct DiffView {
    original: Side,
    modified: Side,
}

impl DiffView {
    pub(crate) fn new(
        original: SnapshotSpec,
        modified: SnapshotSpec,
        cx: &mut Context<Self>,
    ) -> Self {
        let view = Self {
            original: Side::snapshot(original, cx),
            modified: Side::snapshot(modified, cx),
        };
        view.band_hunks(cx);
        view
    }

    /// Diff the sides off the foreground, then band both editors.
    fn band_hunks(&self, cx: &mut Context<Self>) {
        let old = self.original.model.read(cx).text();
        let new = self.modified.model.read(cx).text();
        cx.spawn(async move |this, cx| {
            let (hunks, old, new) = cx
                .background_spawn(async move {
                    let hunks = line_hunks(&old, &new, DiffOptions::default());
                    (hunks, old, new)
                })
                .await;
            this.update(cx, |this, cx| {
                for (side, text, kind) in [
                    (&this.original, &old, DiffSide::Original),
                    (&this.modified, &new, DiffSide::Modified),
                ] {
                    let items = hunk_decorations(&hunks, kind, text);
                    let revision = side.model.read(cx).revision();
                    let editor = side.editor.read(cx).editor().clone();
                    editor.update(cx, |editor, cx| {
                        // Snapshots are read-only, so the revision holds.
                        let _ = editor.set_decorations(BAND_OWNER, &items, revision, cx);
                    });
                }
            })
            .ok();
        })
        .detach();
    }

    /// The tab title, from both labels.
    pub(crate) fn title(&self) -> String {
        format!("{} ↔ {}", self.original.label, self.modified.label)
    }

    /// The sides as they are now, to open the same comparison again.
    pub(crate) fn specs(&self, cx: &App) -> (SnapshotSpec, SnapshotSpec) {
        (self.original.spec(cx), self.modified.spec(cx))
    }

    /// The editor that commands act on: the modified side's.
    pub(crate) fn command_editor(&self) -> &Entity<EditorView> {
        &self.modified.editor
    }

    #[cfg(test)]
    pub(crate) fn editors(&self) -> [&Entity<EditorView>; 2] {
        [&self.original.editor, &self.modified.editor]
    }
}

impl Focusable for DiffView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.command_editor().read(cx).focus_handle(cx)
    }
}

impl Render for DiffView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        div()
            .size_full()
            .flex()
            .flex_row()
            .child(side(&self.original.label, &self.original.editor))
            .child(div().w(px(1.)).h_full().bg(border))
            .child(side(&self.modified.label, &self.modified.editor))
    }
}
