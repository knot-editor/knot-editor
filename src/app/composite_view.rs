//! A center tab whose layout of child views a package describes.
//!
//! A package registers a kind and opens instances; the instance owns the
//! views it contains, which the user cannot rearrange. Each child is an
//! ordinary view, so commands, keymaps, and editor-driven packages work in
//! it as anywhere else.

use std::sync::atomic::{AtomicU64, Ordering};

use gpui::{prelude::*, *};
use knot_model::workbench::CompositeViewId;
use knot_protocol::IconRef;

use super::editor::EditorView;
use super::model::Buffer;
use super::resource::ResourceUri;
use super::view_tree::ViewTreeChanged;

static NEXT_COMPOSITE_ID: AtomicU64 = AtomicU64::new(1);

/// The identity of a new composite view.
pub(crate) fn allocate_composite_id() -> CompositeViewId {
    CompositeViewId(NEXT_COMPOSITE_ID.fetch_add(1, Ordering::Relaxed))
}

/// What the tab of a composite view shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CompositeTab {
    pub title: String,
    pub icon: Option<IconRef>,
    /// The resource whose decorations the tab shows, as a document tab does.
    pub resource: Option<ResourceUri>,
}

pub(crate) struct CompositeView {
    tab: CompositeTab,
    editor: Entity<EditorView>,
}

impl CompositeView {
    pub(crate) fn new(tab: CompositeTab, buffer: Entity<Buffer>, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| EditorView::new(buffer, cx));
        Self { tab, editor }
    }

    pub(crate) fn tab(&self) -> &CompositeTab {
        &self.tab
    }

    pub(crate) fn set_tab(&mut self, tab: CompositeTab, cx: &mut Context<Self>) {
        self.tab = tab;
        cx.notify();
    }

    /// The editor child.
    pub(crate) fn editor(&self) -> &Entity<EditorView> {
        &self.editor
    }
}

impl EventEmitter<ViewTreeChanged> for CompositeView {}

impl Focusable for CompositeView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.read(cx).focus_handle(cx)
    }
}

impl Render for CompositeView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.editor.clone())
    }
}
