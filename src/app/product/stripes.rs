//! The tool stripes on the left and right window edges: a button per tool
//! window, grouped by the slot it sits in.

use super::*;
use crate::app::context_menu::ContextMenu;
use crate::app::convert::ThemePaint as _;
use crate::app::theme::{chrome, color, theme};
use crate::app::tokens;
use crate::app::workbench::{Slot, StripeClick, Stripes};
use crate::app::zoom;
use gpui_component::menu::PopupMenuItem;
use knot_model::icons::IconDefinition;

/// A stripe button being dragged to another stripe.
#[derive(Clone)]
struct StripeDrag {
    tool: ToolFrameId,
    title: SharedString,
    icon: Option<IconDefinition>,
}

/// The dragged button's label, which ends the window's stripe drag when it is released.
struct StripeDragLabel(StripeDrag);

impl Render for StripeDragLabel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        stripe_face(self.0.icon.clone(), &self.0.title)
            .size(zoom::rems(tokens::chrome::STRIPE_BUTTON))
            .rounded(crate::app::zoom::rems(crate::app::tokens::radius::SM))
            .bg(theme(cx).paint_color(color::LIST_SELECTED_BACKGROUND))
            .text_color(theme(cx).paint_color(color::LIST_SELECTED_FOREGROUND))
    }
}

/// A button's icon, or the initial of `title` when it has none.
fn stripe_face(icon: Option<IconDefinition>, title: &str) -> Div {
    let face = div().flex().items_center().justify_center().text_base();
    match icon {
        Some(icon) => face.child(crate::app::icons::AppIcon::new(icon)),
        None => face.child(
            title
                .chars()
                .next()
                .map(|initial| initial.to_uppercase().to_string())
                .unwrap_or_default(),
        ),
    }
}

impl KnotWindow {
    /// The left and right stripes; none when `workbench.toolStripes` is off,
    /// and a stripe without buttons is absent, except while a drag could
    /// drop a button on it. The left stripe shows the left top group, a
    /// separator, the left bottom group, and the bottom left group at its
    /// lower end; the right stripe mirrors it with the right slots and the
    /// bottom right group.
    pub(super) fn render_stripes(
        &self,
        cx: &mut Context<Self>,
    ) -> (Option<Stateful<Div>>, Option<Stateful<Div>>) {
        use crate::app::settings;
        if !settings::boolean(settings::WORKBENCH_TOOL_STRIPES, cx) {
            return (None, None);
        }
        let workbench = self.workbench.read(cx);
        let stripes = workbench.stripes();
        let more = workbench.off_sidebar_tools();
        let stripe_width = chrome(cx).stripe_width;
        let stripe = |id: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.to_owned())
                .flex_none()
                .flex()
                .flex_col()
                .h_full()
                .w(zoom::rems(stripe_width))
                .py_1()
                .gap_1()
                .items_center()
        };
        let dragging = self.stripe_dragging.get();
        let populated = |slots: [Slot; 3]| {
            dragging || slots.iter().any(|slot| !stripes.group(*slot).is_empty())
        };
        let line = theme(cx).paint_color(color::STRIPE_SEPARATOR);
        let separator = move || {
            div()
                .h(px(1.))
                .my(zoom::rems(6.))
                .w(zoom::rems(tokens::chrome::STRIPE_BUTTON * 0.8))
                .bg(line)
        };
        // A separator sits between two groups that both have buttons.
        let has = |slot: Slot| !stripes.group(slot).is_empty();
        let split = |earlier: &[Slot], slot: Slot| has(slot) && earlier.iter().any(|s| has(*s));
        let left = (populated([Slot::LeftTop, Slot::LeftBottom, Slot::BottomLeft])
            || !more.is_empty())
        .then(|| {
            stripe("stripe-left")
                .child(self.render_stripe_group(Slot::LeftTop, &stripes, cx))
                .when(split(&[Slot::LeftTop], Slot::LeftBottom), |s| {
                    s.child(separator())
                })
                .child(self.render_stripe_group(Slot::LeftBottom, &stripes, cx))
                .child(div().flex_1())
                .child(self.render_stripe_group(Slot::BottomLeft, &stripes, cx))
                .when(!more.is_empty(), |stripe| {
                    stripe.child(self.render_more_button(more, cx))
                })
        });
        let right = populated([Slot::RightTop, Slot::RightBottom, Slot::BottomRight]).then(|| {
            stripe("stripe-right")
                .child(self.render_stripe_group(Slot::RightTop, &stripes, cx))
                .when(split(&[Slot::RightTop], Slot::RightBottom), |s| {
                    s.child(separator())
                })
                .child(self.render_stripe_group(Slot::RightBottom, &stripes, cx))
                .child(div().flex_1())
                .child(self.render_stripe_group(Slot::BottomRight, &stripes, cx))
        });
        (left, right)
    }

    /// The buttons of tool windows in `slot`, which also takes a dragged
    /// button as its tool window's new slot.
    fn render_stripe_group(
        &self,
        slot: Slot,
        stripes: &Stripes,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let drop_hint = theme(cx).paint_color(color::LIST_SELECTED_BACKGROUND);
        let buttons = stripes
            .group(slot)
            .iter()
            .map(|tool| self.render_stripe_button(*tool, cx))
            .collect::<Vec<_>>();
        let dragging = self.stripe_dragging.get();
        div()
            .id(("stripe-group", slot.index()))
            .debug_selector(move || format!("stripe-group-{}", slot.index()))
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .w_full()
            .when(dragging, |group| {
                group.min_h(zoom::rems(tokens::chrome::STRIPE_GROUP_MIN_HEIGHT))
            })
            .drag_over::<StripeDrag>(move |style, _, _, _| style.bg(drop_hint))
            .on_drop(cx.listener(move |this, drag: &StripeDrag, _, cx| {
                this.move_tool_to_slot(drag.tool, slot, cx);
            }))
            .children(buttons)
    }

    fn render_stripe_button(&self, id: ToolFrameId, cx: &mut Context<Self>) -> AnyElement {
        let workbench = self.workbench.read(cx);
        let tool = workbench.tool(id).expect("stripe buttons name live tools");
        let icon = crate::app::icons::defined_icon(&format!("tool.{}", tool.stable_id), cx);
        let (key, title) = (tool.stable_id.clone(), tool.title.clone());
        let visible = workbench.tool_visible(id);
        let focused = workbench
            .focused_pane_id()
            .is_some_and(|pane| workbench.pane_tool(pane) == Some(id));
        let hover = theme(cx).paint_color(color::LIST_HOVER_BACKGROUND);
        let selected = theme(cx).paint_color(if focused {
            color::STRIPE_SELECTED_BACKGROUND
        } else {
            color::STRIPE_SHOWN_BACKGROUND
        });
        let selected_text = theme(cx).paint_color(color::TEXT);
        let title = SharedString::from(title);
        let drag = StripeDrag {
            tool: id,
            title: title.clone(),
            icon: icon.clone(),
        };
        let selector = format!("stripe-button-{key}");
        let tooltip_title = title.clone();
        stripe_face(icon, &title)
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .size(zoom::rems(tokens::chrome::STRIPE_BUTTON))
            .flex_none()
            .rounded(crate::app::zoom::rems(crate::app::tokens::radius::SM))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .when(visible, |element| {
                element.bg(selected).text_color(selected_text)
            })
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip_title.clone())
                    .text_xs()
                    .build(window, cx)
            })
            .on_drag(drag, {
                let dragging = self.stripe_dragging.clone();
                move |drag, _, _, cx| {
                    dragging.set(true);
                    let dragging = dragging.clone();
                    cx.new(|cx| {
                        cx.on_release(move |_, _| dragging.set(false)).detach();
                        StripeDragLabel(drag.clone())
                    })
                }
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.open_tool_menu(id, true, event.position, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.click_stripe_button(id, cx)))
            .into_any_element()
    }

    /// The More tool windows button, which lists the tool windows without a
    /// stripe button.
    fn render_more_button(&self, tools: Vec<ToolFrameId>, cx: &mut Context<Self>) -> AnyElement {
        let hover = theme(cx).paint_color(color::LIST_HOVER_BACKGROUND);
        let titles = tools
            .into_iter()
            .filter_map(|id| Some((id, self.workbench.read(cx).tool(id)?.title.clone())))
            .collect::<Vec<_>>();
        stripe_face(None, "⋯")
            .id("stripe-more")
            .debug_selector(|| "stripe-more".to_owned())
            .size(zoom::rems(tokens::chrome::STRIPE_BUTTON))
            .flex_none()
            .rounded(crate::app::zoom::rems(crate::app::tokens::radius::SM))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("More tool windows")
                    .text_xs()
                    .build(window, cx)
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                let position = event.position();
                this.open_more_menu(titles.clone(), position, window, cx);
            }))
            .into_any_element()
    }

    /// A click on a tool's stripe button: a shown window hides, a hidden one
    /// shows and takes focus. A container without tabs opens its first.
    pub(super) fn click_stripe_button(&mut self, id: ToolFrameId, cx: &mut Context<Self>) {
        let click = self
            .workbench
            .update(cx, |workbench, _| workbench.stripe_click(id));
        match click {
            StripeClick::Shown(..) => self.refresh_tool_tree(id, cx),
            StripeClick::Empty => self.run_new_tab(id, cx),
            StripeClick::Hidden => {}
        }
        self.publish_tool_layout(cx);
        self.focus_active_surface(cx);
        cx.notify();
    }

    /// Run the new-tab command of a container tool window, which opens a tab
    /// in it.
    pub(super) fn run_new_tab(&mut self, id: ToolFrameId, cx: &mut Context<Self>) {
        let Some(stable_id) = self
            .workbench
            .read(cx)
            .tool(id)
            .map(|tool| tool.stable_id.clone())
        else {
            return;
        };
        #[cfg(test)]
        let fallback = (stable_id == crate::app::terminal::TERMINAL_TOOL_ID)
            .then(|| Some("terminal.new".to_owned()));
        #[cfg(not(test))]
        let fallback = None;
        let Some(Some(command)) = self
            .contributed_containers
            .get(&stable_id)
            .cloned()
            .or(fallback)
        else {
            return;
        };
        self.dispatch_source(
            knot_protocol::Command {
                name: command.into(),
                arguments: knot_protocol::CommandArgumentValue::Null,
            },
            cx,
        );
    }

    /// Refresh the tree of a contributed tool window that just showed.
    pub(super) fn refresh_tool_tree(&mut self, id: ToolFrameId, cx: &mut Context<Self>) {
        let Some(stable_id) = self
            .workbench
            .read(cx)
            .tool(id)
            .map(|tool| tool.stable_id.clone())
        else {
            return;
        };
        if let Some(crate::app::workbench::ExtensionView::Tree(tree)) = self
            .contributed_views
            .get(&stable_id)
            .map(|instance| instance.content.clone())
        {
            tree.update(cx, |tree, cx| tree.refresh_root(cx));
        }
    }

    /// Move a tool window to `slot`, replacing the window shown there when
    /// this one shows. Publishes the new slot.
    pub(super) fn move_tool_to_slot(
        &mut self,
        id: ToolFrameId,
        slot: Slot,
        cx: &mut Context<Self>,
    ) {
        self.workbench
            .update(cx, |workbench, _| workbench.place_tool(id, slot));
        self.publish_tool_layout(cx);
        self.focus_active_surface(cx);
        cx.notify();
    }

    /// Hide a tool window and take its button off the stripes.
    pub(super) fn remove_tool_from_sidebar(&mut self, id: ToolFrameId, cx: &mut Context<Self>) {
        self.workbench
            .update(cx, |workbench, _| workbench.remove_from_sidebar(id));
        self.publish_tool_layout(cx);
        self.focus_active_surface(cx);
        cx.notify();
    }

    /// Bring a tool window off the sidebar back: it shows in its slot and its
    /// button returns to that slot's stripe group.
    pub(super) fn restore_tool_to_sidebar(&mut self, id: ToolFrameId, cx: &mut Context<Self>) {
        let shown = self
            .workbench
            .update(cx, |workbench, _| workbench.show_tool(id).is_some());
        if shown {
            self.refresh_tool_tree(id, cx);
        } else {
            self.workbench
                .update(cx, |workbench, _| workbench.add_to_sidebar(id));
        }
        self.publish_tool_layout(cx);
        self.focus_active_surface(cx);
        cx.notify();
    }

    /// Open the menu of a tool window at `position`: from its stripe button
    /// (`with_hide`), Hide, and always Move to each other slot and Remove
    /// from Sidebar. It takes focus, which returns to the active surface when
    /// it closes.
    pub(super) fn open_tool_menu(
        &mut self,
        id: ToolFrameId,
        with_hide: bool,
        position: Point<Pixels>,
        native_window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workbench = self.workbench.read(cx);
        let Some(tool) = workbench.tool(id) else {
            return;
        };
        let (visible, current) = (workbench.tool_visible(id), tool.slot());
        let window = cx.entity().downgrade();
        let menu = ContextMenu::open(
            native_window,
            cx,
            position,
            None,
            move |menu| {
                let mut items = Vec::new();
                if with_hide && visible {
                    let window = window.clone();
                    items.push(PopupMenuItem::new("Hide").on_click(move |_, _, cx| {
                        let _ = window.update(cx, |this, cx| this.click_stripe_button(id, cx));
                    }));
                }
                for slot in Slot::ALL.into_iter().filter(|slot| *slot != current) {
                    let window = window.clone();
                    items.push(
                        PopupMenuItem::new(format!("Move to {}", slot_name(slot))).on_click(
                            move |_, _, cx| {
                                let _ = window
                                    .update(cx, |this, cx| this.move_tool_to_slot(id, slot, cx));
                            },
                        ),
                    );
                }
                let window = window.clone();
                items.push(
                    PopupMenuItem::new("Remove from Sidebar").on_click(move |_, _, cx| {
                        let _ = window.update(cx, |this, cx| this.remove_tool_from_sidebar(id, cx));
                    }),
                );
                items.into_iter().fold(menu, |menu, item| menu.item(item))
            },
            Self::close_stripe_menu,
        );
        self.stripe_menu = Some(menu);
        self.set_focus(Focus::Popup, cx);
        cx.notify();
    }

    /// Open the list of tool windows without a stripe button.
    fn open_more_menu(
        &mut self,
        tools: Vec<(ToolFrameId, String)>,
        position: Point<Pixels>,
        native_window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let window = cx.entity().downgrade();
        let menu = ContextMenu::open(
            native_window,
            cx,
            position,
            None,
            move |menu| {
                tools.iter().fold(menu, |menu, (id, title)| {
                    let (window, id) = (window.clone(), *id);
                    menu.item(PopupMenuItem::new(title.clone()).on_click(move |_, _, cx| {
                        let _ = window.update(cx, |this, cx| this.restore_tool_to_sidebar(id, cx));
                    }))
                })
            },
            Self::close_stripe_menu,
        );
        self.stripe_menu = Some(menu);
        self.set_focus(Focus::Popup, cx);
        cx.notify();
    }

    fn close_stripe_menu(&mut self, cx: &mut Context<Self>) {
        if self.stripe_menu.take().is_some() {
            self.close_popup_focus(cx);
            cx.notify();
        }
    }
}

/// How the menus name a slot.
fn slot_name(slot: Slot) -> &'static str {
    match slot {
        Slot::LeftTop => "Left Top",
        Slot::LeftBottom => "Left Bottom",
        Slot::RightTop => "Right Top",
        Slot::RightBottom => "Right Bottom",
        Slot::BottomLeft => "Bottom Left",
        Slot::BottomRight => "Bottom Right",
    }
}
