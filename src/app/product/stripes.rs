//! The tool stripes on the left and right window edges: a button per tool
//! and one for the dock terminals, grouped by home dock.

use super::*;
use crate::app::convert::ThemePaint as _;
use crate::app::theme::{color, theme};
use crate::app::workbench::{StripeButton, Stripes};
use crate::app::zoom;
use knot_model::icons::IconDefinition;

/// A stripe button being dragged to another stripe.
#[derive(Clone)]
struct StripeDrag {
    button: StripeButton,
    title: SharedString,
    icon: Option<IconDefinition>,
}

struct StripeDragLabel(StripeDrag);

impl Render for StripeDragLabel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        stripe_face(self.0.icon.clone(), &self.0.title)
            .size(zoom::rems(26.))
            .rounded_sm()
            .bg(theme(cx).paint_color(color::LIST_SELECTED_BACKGROUND))
            .text_color(theme(cx).paint_color(color::LIST_SELECTED_FOREGROUND))
    }
}

/// A button's icon, or the initial of `title` when it has none.
fn stripe_face(icon: Option<IconDefinition>, title: &str) -> Div {
    let face = div().flex().items_center().justify_center().text_sm();
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

impl ProductShell {
    /// Both stripes, as the left and right elements; none when
    /// `workbench.toolStripes` is off.
    pub(super) fn render_stripes(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<(Stateful<Div>, Stateful<Div>)> {
        use crate::app::settings;
        if !settings::boolean(settings::WORKBENCH_TOOL_STRIPES, cx) {
            return None;
        }
        let stripes = self.workbench.read(cx).stripes();
        let border = theme(cx).paint_color(color::BORDER);
        let stripe = |id: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.to_owned())
                .flex_none()
                .flex()
                .flex_col()
                .h_full()
                .w(zoom::rems(30.))
                .py_1()
                .gap_1()
                .items_center()
        };
        let left = stripe("stripe-left")
            .border_r_1()
            .border_color(border)
            .child(
                self.render_stripe_group(DockLocation::Left, &stripes, cx)
                    .flex_1(),
            )
            .child(self.render_stripe_group(DockLocation::Bottom, &stripes, cx));
        let right = stripe("stripe-right")
            .border_l_1()
            .border_color(border)
            .child(
                self.render_stripe_group(DockLocation::Right, &stripes, cx)
                    .flex_1(),
            );
        Some((left, right))
    }

    /// The buttons whose home is `dock`, which also takes a dragged button
    /// as its new home.
    fn render_stripe_group(
        &self,
        dock: DockLocation,
        stripes: &Stripes,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let drop_hint = theme(cx).paint_color(color::LIST_SELECTED_BACKGROUND);
        let buttons = stripes
            .group(dock)
            .iter()
            .map(|button| self.render_stripe_button(*button, cx))
            .collect::<Vec<_>>();
        div()
            .id(("stripe-group", dock.index()))
            .debug_selector(move || format!("stripe-group-{}", dock.index()))
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .w_full()
            .when(dock == DockLocation::Bottom, |group| {
                group.min_h(zoom::rems(60.)).justify_end()
            })
            .drag_over::<StripeDrag>(move |style, _, _, _| style.bg(drop_hint))
            .on_drop(cx.listener(move |this, drag: &StripeDrag, _, cx| {
                this.rehome_stripe_button(drag.button, dock, cx);
            }))
            .children(buttons)
    }

    fn render_stripe_button(&self, button: StripeButton, cx: &mut Context<Self>) -> AnyElement {
        let workbench = self.workbench.read(cx);
        let (key, title, icon) = match button {
            StripeButton::Tool(id) => {
                let tool = workbench.tool(id).expect("stripe buttons name live tools");
                let icon = crate::app::icons::defined_icon(&format!("tool.{}", tool.stable_id), cx);
                (tool.stable_id.clone(), tool.title.clone(), icon)
            }
            StripeButton::Terminal => (
                "terminal".to_owned(),
                "Terminal".to_owned(),
                crate::app::icons::defined_icon("terminal", cx),
            ),
        };
        let visible = workbench.stripe_button_visible(button);
        let hover = theme(cx).paint_color(color::LIST_HOVER_BACKGROUND);
        let selected = theme(cx).paint_color(color::LIST_SELECTED_BACKGROUND);
        let selected_text = theme(cx).paint_color(color::LIST_SELECTED_FOREGROUND);
        let title = SharedString::from(title);
        let drag = StripeDrag {
            button,
            title: title.clone(),
            icon: icon.clone(),
        };
        let selector = format!("stripe-button-{key}");
        let tooltip_title = title.clone();
        let stable_id = key.clone();
        stripe_face(icon, &title)
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .size(zoom::rems(26.))
            .flex_none()
            .rounded_sm()
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
            .on_drag(drag, |drag, _, _, cx| {
                cx.new(|_| StripeDragLabel(drag.clone()))
            })
            .on_click(cx.listener(move |this, _, _, cx| match button {
                StripeButton::Tool(_) => this.toggle_tool(&stable_id, cx),
                StripeButton::Terminal => {
                    this.toggle_terminal(cx).ok();
                }
            }))
            .into_any_element()
    }

    /// Make `dock` the home of a stripe button's surface: a shown tool moves
    /// there, a hidden one stays hidden and returns there, and the terminal
    /// home changes for the next terminal. Publishes the new home.
    fn rehome_stripe_button(
        &mut self,
        button: StripeButton,
        dock: DockLocation,
        cx: &mut Context<Self>,
    ) {
        match button {
            StripeButton::Terminal => {
                self.workbench
                    .update(cx, |workbench, _| workbench.set_terminal_home(dock));
            }
            StripeButton::Tool(id) => {
                let shown = self.workbench.read(cx).tool_tab(id);
                if let Some((pane, tab)) = shown {
                    // A tool already in `dock` is unavailable to move: nothing to do.
                    let _ = self.move_tab_to_area(pane, tab, Area::Dock(dock), cx);
                } else {
                    self.workbench
                        .update(cx, |workbench, _| workbench.rehome_hidden_tool(id, dock));
                }
            }
        }
        self.publish_tool_layout(cx);
        cx.notify();
    }
}
