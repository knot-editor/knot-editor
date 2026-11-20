//! The startup window that asks which entitlements installed packages may
//! hold, before any workbench window opens.
//!
//! Each package gets Allow and Deny; Continue records the decisions made.
//! Closing the window, or continuing without deciding a package, records
//! nothing for it, so Knot asks again at the next launch. The window stays
//! open until the workbench opens, so quitting with the last window cannot
//! fire in between; [`close`] ends it.

use gpui::{
    AnyWindowHandle, App, AppContext, Bounds, Context, ElementId, FocusHandle, Global,
    InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled,
    Window, WindowBounds, WindowOptions, div, px, size,
};
use knot_model::extensions::entitlements::{Decision, UndecidedPackage};

use crate::app::commands::dispatch::ApplicationCommandSource;
use crate::app::convert::ThemePaint as _;
use crate::app::theme::{color, theme};

/// The packages the user decided on, with each decision.
pub(crate) type Decisions = Vec<(UndecidedPackage, Decision)>;

struct Row {
    package: UndecidedPackage,
    decision: Option<Decision>,
}

pub(crate) struct EntitlementConsentView {
    rows: Vec<Row>,
    focus: FocusHandle,
    /// Present until the user has answered.
    sender: Option<tokio::sync::oneshot::Sender<Decisions>>,
    #[cfg(test)]
    quit_requested: bool,
}

impl EntitlementConsentView {
    fn new(
        packages: Vec<UndecidedPackage>,
        sender: tokio::sync::oneshot::Sender<Decisions>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            rows: packages
                .into_iter()
                .map(|package| Row {
                    package,
                    decision: None,
                })
                .collect(),
            focus: cx.focus_handle(),
            sender: Some(sender),
            #[cfg(test)]
            quit_requested: false,
        }
    }

    fn answered(&self) -> bool {
        self.sender.is_none()
    }

    fn decide(&mut self, index: usize, decision: Decision, cx: &mut Context<Self>) {
        if self.answered() {
            return;
        }
        if let Some(row) = self.rows.get_mut(index) {
            row.decision = Some(decision);
            cx.notify();
        }
    }

    /// Answer with the decisions made, or with none when `record` is false.
    fn finish(&mut self, record: bool, cx: &mut Context<Self>) {
        let Some(sender) = self.sender.take() else {
            return;
        };
        let decisions = if record {
            self.rows
                .iter()
                .filter_map(|row| Some((row.package.clone(), row.decision?)))
                .collect()
        } else {
            Vec::new()
        };
        let _ = sender.send(decisions);
        cx.notify();
    }

    fn button(
        &self,
        id: impl Into<ElementId>,
        label: &'static str,
        selected: bool,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = theme(cx);
        div()
            .id(id)
            .cursor_pointer()
            .rounded_md()
            .border_1()
            .border_color(theme.paint_color(if selected {
                color::TEXT_ACCENT
            } else {
                color::BORDER
            }))
            .px_3()
            .py_1()
            .text_color(theme.paint_color(color::TEXT_ACCENT))
            .child(label)
    }
}

impl Render for EntitlementConsentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title("Knot extension permissions");
        let answered = self.answered();
        let rows = self
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let package = &row.package;
                let wants = package
                    .entitlements
                    .iter()
                    .map(|entitlement| format!("{}: {}", entitlement.name(), entitlement.summary()))
                    .collect::<Vec<_>>()
                    .join("\n");
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .py_2()
                    .border_b_1()
                    .border_color(theme(cx).paint_color(color::BORDER))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .child(format!("{} {}", package.display_name, package.version)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme(cx).paint_color(color::TEXT_MUTED))
                                    .child(package.name.clone()),
                            )
                            .child(div().text_sm().child(wants)),
                    )
                    .child(
                        self.button(
                            ("allow", index),
                            "Allow",
                            row.decision == Some(Decision::Granted),
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.decide(index, Decision::Granted, cx)
                        })),
                    )
                    .child(
                        self.button(
                            ("deny", index),
                            "Deny",
                            row.decision == Some(Decision::Denied),
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.decide(index, Decision::Denied, cx)
                        })),
                    )
            })
            .collect::<Vec<_>>();
        div()
            .track_focus(&self.focus)
            .key_context("entitlement-consent")
            .on_action(
                cx.listener(|_this, source: &ApplicationCommandSource, _, cx| {
                    if source.name.as_ref() == crate::app::commands::dispatch::QUIT_COMMAND {
                        #[cfg(test)]
                        {
                            _this.quit_requested = true;
                        }
                        crate::app::extensions::host::quit_after_storage(cx);
                    }
                }),
            )
            .size_full()
            .flex()
            .flex_col()
            .gap_4()
            .p_5()
            .bg(theme(cx).paint_color(color::BACKGROUND))
            .text_color(theme(cx).paint_color(color::TEXT))
            .child(div().text_lg().child("Extensions ask for permissions"))
            .child(
                div()
                    .text_sm()
                    .text_color(theme(cx).paint_color(color::TEXT_MUTED))
                    .child(
                        "An extension that is denied, or not decided, does not load. \
                         Knot asks again at the next launch about the ones left undecided.",
                    ),
            )
            .child(
                div()
                    .id("entitlement-consent-packages")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(rows),
            )
            .child(div().flex().justify_end().child(if answered {
                div().text_sm().child("Starting Knot…").into_any_element()
            } else {
                self.button("continue-entitlements", "Continue", false, cx)
                    .on_click(cx.listener(|this, _, _, cx| this.finish(true, cx)))
                    .into_any_element()
            }))
    }
}

/// The open consent window, until startup closes it.
struct ApplicationConsentWindow(AnyWindowHandle);

impl Global for ApplicationConsentWindow {}

/// Open the window for `packages`. The receiver yields the decisions the user
/// made, or none when the window is closed.
pub(crate) fn open(
    packages: Vec<UndecidedPackage>,
    cx: &mut App,
) -> tokio::sync::oneshot::Receiver<Decisions> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let bounds = Bounds::centered(None, size(px(760.), px(480.)), cx);
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(560.), px(320.))),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| EntitlementConsentView::new(packages, sender, cx));
                let closing = view.downgrade();
                window.on_window_should_close(cx, move |_, cx| {
                    let _ = closing.update(cx, |view, cx| view.finish(false, cx));
                    false
                });
                let focus = view.read(cx).focus.clone();
                focus.focus(window, cx);
                view
            },
        )
        .expect("entitlement consent window must open");
    cx.set_global(ApplicationConsentWindow(handle.into()));
    cx.activate(true);
    receiver
}

/// Close the window once the workbench has opened, or startup has failed.
pub(crate) fn close(cx: &mut App) {
    if let Some(ApplicationConsentWindow(handle)) = cx.try_global::<ApplicationConsentWindow>() {
        let handle = *handle;
        let _ = handle.update(cx, |_, window, _| window.remove_window());
        cx.remove_global::<ApplicationConsentWindow>();
    }
}

#[cfg(test)]
impl EntitlementConsentView {
    pub(crate) fn decide_for_test(
        &mut self,
        name: &str,
        decision: Decision,
        cx: &mut Context<Self>,
    ) {
        let index = self
            .rows
            .iter()
            .position(|row| row.package.name == name)
            .expect("package is listed");
        self.decide(index, decision, cx);
    }

    pub(crate) fn continue_for_test(&mut self, cx: &mut Context<Self>) {
        self.finish(true, cx);
    }

    /// What the window's close button does.
    pub(crate) fn close_for_test(&mut self, cx: &mut Context<Self>) {
        self.finish(false, cx);
    }

    pub(crate) fn listed_for_test(&self) -> Vec<&UndecidedPackage> {
        self.rows.iter().map(|row| &row.package).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::product::bind_product_keys;
    use gpui::TestAppContext;
    use knot_model::extensions::package::Entitlement;

    fn package(name: &str) -> UndecidedPackage {
        UndecidedPackage {
            name: name.into(),
            display_name: name.into(),
            version: "1.0.0".into(),
            entitlements: vec![Entitlement::Process],
        }
    }

    fn window(cx: &mut TestAppContext) -> gpui::WindowHandle<EntitlementConsentView> {
        cx.windows()[0]
            .downcast::<EntitlementConsentView>()
            .unwrap()
    }

    #[gpui::test]
    fn quit_command_is_bound_in_the_consent_window(cx: &mut TestAppContext) {
        cx.update(bind_product_keys);
        cx.update(|cx| open(vec![package("@a/b")], cx));
        let window = window(cx);
        cx.simulate_keystrokes(*window, "secondary-q");
        cx.read(|cx| assert!(window.read(cx).unwrap().quit_requested));
    }

    #[gpui::test]
    fn continue_answers_with_the_packages_that_were_decided(cx: &mut TestAppContext) {
        let mut receiver = cx.update(|cx| open(vec![package("@a/b"), package("@c/d")], cx));
        let window = window(cx);
        window
            .update(cx, |view, _, cx| {
                view.decide_for_test("@a/b", Decision::Granted, cx);
                view.continue_for_test(cx);
            })
            .unwrap();
        let decisions = receiver.try_recv().unwrap();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].0.name, "@a/b");
        assert_eq!(decisions[0].1, Decision::Granted);
    }

    #[gpui::test]
    fn closing_the_window_answers_with_nothing_and_keeps_it_open(cx: &mut TestAppContext) {
        let mut receiver = cx.update(|cx| open(vec![package("@a/b")], cx));
        let window = window(cx);
        window
            .update(cx, |view, _, cx| {
                view.decide_for_test("@a/b", Decision::Denied, cx)
            })
            .unwrap();
        window
            .update(cx, |view, _, cx| view.close_for_test(cx))
            .unwrap();
        assert!(receiver.try_recv().unwrap().is_empty());
        assert_eq!(cx.windows().len(), 1);
        cx.update(close);
        assert!(cx.windows().is_empty());
    }
}
