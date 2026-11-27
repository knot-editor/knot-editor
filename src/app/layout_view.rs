//! A tool window whose layout of views a package describes and replaces per
//! window.
//!
//! The package registers a view with layout content and sends the layout and
//! toolbar to each window with `update`. The window owns the views the layout
//! holds, which are ordinary views, so commands, keymaps, and editor-driven
//! packages work in them as anywhere else. Until the first layout arrives the
//! tool window is empty.

use std::rc::Rc;
use std::sync::atomic::Ordering;

use gpui::{prelude::*, *};
use knot_protocol::ViewId;

use super::view_layout::{
    GroupSpec, Layout, LayoutChange, RunCommand, Shared, Sizing, ToolbarButton, toolbar,
};
use super::view_tree::{ViewFocusChanged, ViewKind, ViewNode, ViewTree, ViewTreeChanged};
use super::workbench::CommandView;

/// Which view commands act on in a layout tool window, which stays the same
/// while the layout is replaced and is empty before the first one.
#[derive(Clone, Default)]
pub(crate) struct LayoutTarget(Rc<std::cell::RefCell<Option<Rc<Shared>>>>);

impl LayoutTarget {
    /// The view that has focus in the layout, if it has views.
    pub(crate) fn command_view(&self) -> Option<CommandView> {
        Some(self.0.borrow().as_ref()?.focused_leaf().command_view())
    }
}

/// One contributed layout tool window in a window.
pub(crate) struct LayoutView {
    view_id: String,
    instance_id: ViewId,
    window: String,
    toolbar: Vec<ToolbarButton>,
    layout: Option<Layout>,
    target: LayoutTarget,
    focus: FocusHandle,
}

impl LayoutView {
    pub(crate) fn new(
        view_id: impl Into<String>,
        window: impl Into<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.on_release(|this, cx| {
            if let Some(layout) = &mut this.layout {
                let hosted = layout.take_hosted();
                crate::app::view_layout::detach_views(hosted, cx);
            }
        })
        .detach();
        Self {
            view_id: view_id.into(),
            instance_id: ViewId::new(
                super::tree_view::NEXT_VIEW_ID.fetch_add(1, Ordering::Relaxed),
            ),
            window: window.into(),
            toolbar: Vec::new(),
            layout: None,
            target: LayoutTarget::default(),
            focus: cx.focus_handle(),
        }
    }

    pub(crate) fn kind(&self) -> &str {
        &self.view_id
    }

    pub(crate) fn instance_id(&self) -> ViewId {
        self.instance_id
    }

    pub(crate) fn target(&self) -> LayoutTarget {
        self.target.clone()
    }

    /// Replace the layout and the toolbar, each when given. Returns what the
    /// new layout added, which the host binds.
    pub(crate) fn update(
        &mut self,
        spec: Option<GroupSpec>,
        toolbar: Option<Vec<ToolbarButton>>,
        cx: &mut Context<Self>,
    ) -> Option<LayoutChange> {
        let this = cx.weak_entity();
        let run: RunCommand = Rc::new(move |window, command, cx| {
            if let Some(view) = this.upgrade() {
                crate::app::product::run_layout_command(window, view, command, cx);
            }
        });
        if let Some(toolbar) = toolbar {
            self.toolbar = toolbar;
        }
        let change = spec.map(|spec| match &mut self.layout {
            Some(layout) => layout.update(spec, &self.window, Sizing::Fill, cx),
            None => {
                let layout = Layout::new(spec, &self.window, Sizing::Fill, run, cx);
                *self.target.0.borrow_mut() = Some(layout.shared());
                let attached = layout.hosted_views();
                self.layout = Some(layout);
                LayoutChange {
                    attached,
                    focus_moved: false,
                }
            }
        });
        if change.is_some() {
            cx.emit(ViewTreeChanged);
        }
        if change.as_ref().is_some_and(|change| change.focus_moved) {
            cx.emit(ViewFocusChanged);
        }
        cx.notify();
        change
    }

    /// Drop the layout, as when the package that described it goes.
    pub(crate) fn clear(&mut self, cx: &mut Context<Self>) {
        if let Some(mut layout) = self.layout.take() {
            crate::app::view_layout::detach_views(layout.take_hosted(), cx);
        }
        *self.target.0.borrow_mut() = None;
        self.toolbar.clear();
        cx.emit(ViewTreeChanged);
        cx.notify();
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn toolbar(&self) -> &[ToolbarButton] {
        &self.toolbar
    }

    /// The views of the layout in layout order.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn leaves(&self) -> Vec<crate::app::view_layout::Leaf> {
        self.layout
            .as_ref()
            .map(|layout| layout.leaves().to_vec())
            .unwrap_or_default()
    }

    /// Treat the view with the focus handle `focus` as the layout's focused
    /// one, as it gaining focus in a rendered window does.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn set_focused_leaf_holding(&self, focus: &FocusHandle, cx: &App) {
        let Some(layout) = &self.layout else {
            return;
        };
        let leaves = layout.leaves();
        if let Some(index) = leaves
            .iter()
            .position(|leaf| leaf.focus_handle(cx) == *focus)
        {
            layout.set_focused_leaf(index);
        }
    }

    /// The contributed views the layout holds.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn hosted_views(&self) -> Vec<super::workbench::ExtensionView> {
        self.layout
            .as_ref()
            .map(Layout::hosted_views)
            .unwrap_or_default()
    }
}

impl ViewTree for Entity<LayoutView> {
    fn view_node(&self, cx: &App) -> ViewNode {
        match &self.read(cx).layout {
            Some(layout) => layout.node(self.entity_id(), ViewKind::Composite, cx),
            None => ViewNode::leaf(self.entity_id(), ViewKind::Composite),
        }
    }
}

impl EventEmitter<ViewTreeChanged> for LayoutView {}
impl EventEmitter<ViewFocusChanged> for LayoutView {}

impl Focusable for LayoutView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match &self.layout {
            Some(layout) => layout.focused_handle(cx),
            None => self.focus.clone(),
        }
    }
}

impl Render for LayoutView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(layout) = &mut self.layout else {
            return div().size_full().track_focus(&self.focus);
        };
        layout.watch_focus(
            window,
            cx,
            |this: &Self| this.layout.as_ref().expect("the layout being watched"),
            |_, cx| cx.emit(ViewFocusChanged),
        );
        let view = cx.entity();
        let toolbar = toolbar(
            &self.toolbar,
            "layout-toolbar",
            move |window, command, cx| {
                crate::app::product::run_layout_command(window, view.clone(), command, cx);
            },
            cx,
        );
        div().size_full().flex().flex_col().children(toolbar).child(
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .child(self.layout.as_ref().expect("checked above").render(cx)),
        )
    }
}
