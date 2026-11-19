//! Dragging a row onto another row of the same tree.
//!
//! A `draggable` row starts a drag; a `dropTarget` row accepts it. Dropping
//! runs the tree's `drop` command, which decides what the drop means.

use std::time::Duration;

use gpui::{prelude::FluentBuilder, *};

use super::{
    CommandArgumentValue, TreeCollapsibleState, TreeItem, TreeView, TreeViewEvent, ViewId, color,
    theme, tokens,
};
use crate::app::convert::ThemePaint as _;

/// How long a drag rests on a collapsed target before it expands.
pub(super) const EXPAND_DELAY: Duration = Duration::from_millis(700);
/// How near the list's top or bottom edge a drag scrolls it.
const EDGE_ZONE: f32 = 28.;
/// Pixels scrolled per tick while a drag rests in the edge zone.
const EDGE_STEP: f32 = 14.;
pub(super) const EDGE_TICK: Duration = Duration::from_millis(40);

/// The row being dragged.
#[derive(Clone)]
pub(super) struct TreeDrag {
    view: ViewId,
    id: String,
    arguments: CommandArgumentValue,
    label: SharedString,
}

/// What follows the pointer during a drag.
struct TreeDragLabel(TreeDrag);

impl Render for TreeDragLabel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme(cx);
        div()
            .px_2()
            .py_0p5()
            .rounded(crate::app::zoom::rems(tokens::radius::SM))
            .text_size(crate::app::zoom::rems(tokens::list::TEXT_SIZE))
            .bg(theme.paint_color(color::LIST_SELECTED_BACKGROUND))
            .text_color(theme.paint_color(color::LIST_SELECTED_FOREGROUND))
            .child(self.0.label.clone())
    }
}

/// Drag and drop state of a tree.
#[derive(Default)]
pub(super) struct DragState {
    /// The `drop` command of the contribution.
    pub(super) command: Option<String>,
    /// The row the pointer rests on, and a count that invalidates its timer.
    hover: Option<String>,
    hover_epoch: u64,
    /// -1 scrolls up, 1 down, 0 not at all.
    edge: i8,
    edge_task: Option<Task<()>>,
}

/// Add the drag source and drop target behavior of `item` to its `row`.
pub(super) fn row_hooks(
    row: Stateful<Div>,
    entity: &Entity<TreeView>,
    item: &TreeItem,
    view: ViewId,
    can_drop: bool,
    highlight: Hsla,
) -> Stateful<Div> {
    let id = item.id.clone();
    let row = row.when(item.draggable, |row| {
        let drag = TreeDrag {
            view,
            id: id.clone(),
            arguments: item.arguments.clone(),
            label: item.label.clone().into(),
        };
        let tree = entity.downgrade();
        row.on_drag(drag, move |drag, _, _, cx| {
            let tree = tree.clone();
            cx.new(|cx| {
                // The drag ends with its label, however it ended.
                cx.on_release(move |_, cx| {
                    tree.update(cx, |tree, cx| tree.end_drag(cx)).ok();
                })
                .detach();
                TreeDragLabel(drag.clone())
            })
        })
    });
    if !(can_drop && item.drop_target) {
        return row;
    }
    let accepts = move |drag: &TreeDrag| drag.view == view && drag.id != id;
    let target = item.id.clone();
    let hovered = entity.clone();
    let dropped = entity.clone();
    row.drag_over::<TreeDrag>({
        let accepts = accepts.clone();
        move |style, drag, _, _| {
            if accepts(drag) {
                style.bg(highlight)
            } else {
                style
            }
        }
    })
    .on_drag_move::<TreeDrag>({
        let target = target.clone();
        let accepts = accepts.clone();
        move |event, _, cx| {
            if !accepts(event.drag(cx)) {
                return;
            }
            let inside = event.bounds.contains(&event.event.position);
            hovered.update(cx, |tree, cx| tree.drag_hover(&target, inside, cx));
        }
    })
    .on_drop::<TreeDrag>(move |drag, window, cx| {
        if accepts(drag) {
            let copy = window.modifiers().alt;
            dropped.update(cx, |tree, cx| {
                tree.focus_handle(cx).focus(window, cx);
                tree.drop_on(drag, &target, copy, cx);
            });
        }
    })
}

/// The scroll direction a drag at `y` asks of a list spanning `bounds`.
fn edge_direction(bounds: Bounds<Pixels>, y: Pixels) -> i8 {
    if y < bounds.top() + px(EDGE_ZONE) {
        -1
    } else if y > bounds.bottom() - px(EDGE_ZONE) {
        1
    } else {
        0
    }
}

impl TreeView {
    /// Set the command a drop runs; none when the tree takes no drops.
    pub(crate) fn set_drop_command(&mut self, command: Option<String>) {
        self.drag.command = command;
    }

    /// Whether rows of this tree accept drops.
    pub(super) fn takes_drops(&self) -> bool {
        self.drag.command.is_some() && self.provider.is_some()
    }

    /// The pointer of a drag moved over the tree, which scrolls it near an
    /// edge.
    pub(super) fn drag_moved(
        &mut self,
        drag: &TreeDrag,
        bounds: Bounds<Pixels>,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if drag.view != self.instance_id {
            return;
        }
        self.drag.edge = if bounds.contains(&position) {
            edge_direction(bounds, position.y)
        } else {
            0
        };
        if self.drag.edge == 0 || self.drag.edge_task.is_some() {
            return;
        }
        self.drag.edge_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(EDGE_TICK).await;
                let scrolling = this.update(cx, |tree, cx| {
                    let direction = tree.drag.edge;
                    if direction == 0 {
                        tree.drag.edge_task = None;
                        return false;
                    }
                    tree.list.scroll_by(px(EDGE_STEP * f32::from(direction)));
                    cx.notify();
                    true
                });
                if !scrolling.unwrap_or(false) {
                    break;
                }
            }
        }));
    }

    /// The pointer of a drag entered or left row `id`; resting on a collapsed
    /// row expands it.
    fn drag_hover(&mut self, id: &str, inside: bool, cx: &mut Context<Self>) {
        if !inside {
            if self.drag.hover.as_deref() == Some(id) {
                self.drag.hover = None;
                self.drag.hover_epoch += 1;
            }
            return;
        }
        if self.drag.hover.as_deref() == Some(id) {
            return;
        }
        self.drag.hover = Some(id.to_owned());
        self.drag.hover_epoch += 1;
        let epoch = self.drag.hover_epoch;
        let collapsed = self.item(id).is_some_and(|item| {
            item.collapsible_state != TreeCollapsibleState::None && !self.expanded.contains(id)
        });
        if !collapsed {
            return;
        }
        let id = id.to_owned();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(EXPAND_DELAY).await;
            this.update(cx, |tree, cx| {
                if tree.drag.hover_epoch == epoch && tree.drag.hover.as_deref() == Some(&id) {
                    tree.expand(&id, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Run the drop command for `drag` dropped onto row `target`.
    fn drop_on(&mut self, drag: &TreeDrag, target: &str, copy: bool, cx: &mut Context<Self>) {
        let (Some(provider), Some(command)) = (self.provider, self.drag.command.clone()) else {
            return;
        };
        let Some(target_item) = self.item(target).filter(|item| item.drop_target) else {
            return;
        };
        let row = |id: &str, arguments: &CommandArgumentValue| {
            CommandArgumentValue::Object(
                [
                    ("id".to_owned(), CommandArgumentValue::String(id.to_owned())),
                    ("arguments".to_owned(), arguments.clone()),
                ]
                .into(),
            )
        };
        let arguments = CommandArgumentValue::Object(
            [
                ("source".to_owned(), row(&drag.id, &drag.arguments)),
                ("target".to_owned(), row(target, &target_item.arguments)),
                ("copy".to_owned(), CommandArgumentValue::Boolean(copy)),
            ]
            .into(),
        );
        cx.emit(TreeViewEvent::InvokeCommand {
            provider,
            command,
            arguments,
            window: self.window.clone(),
            view: self.instance_id,
        });
    }

    /// The drag ended, by a drop or a cancel.
    fn end_drag(&mut self, cx: &mut Context<Self>) {
        self.drag.hover = None;
        self.drag.hover_epoch += 1;
        self.drag.edge = 0;
        cx.notify();
    }
}
