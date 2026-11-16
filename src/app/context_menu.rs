//! The one native context menu: a gpui-component [`PopupMenu`] that takes
//! focus when it opens and reports when it closes.
//!
//! An owner keeps the open [`ContextMenu`], renders it with
//! [`ContextMenu::element`], and drops it from its `on_close` callback. The
//! window gives an open menu its own Knot-side focus layer (`Focus::Popup`),
//! so the keymap still matches first and a key it leaves goes to gpui's
//! dispatch, where the menu's own actions run.

use gpui::{
    Context, DismissEvent, Entity, FocusHandle, Focusable as _, IntoElement, ParentElement as _,
    Pixels, Point, Subscription, Window, anchored, deferred, px,
};
use gpui_component::menu::PopupMenu;

/// An open context menu, which closes when dismissed or blurred.
pub(crate) struct ContextMenu {
    menu: Entity<PopupMenu>,
    position: Point<Pixels>,
    _subscriptions: [Subscription; 2],
}

impl ContextMenu {
    /// Build a menu with `build`, show it at `position`, and give it focus.
    /// `action_context` is where its actions are dispatched. `on_close` runs
    /// when the menu is dismissed or loses focus.
    pub(crate) fn open<T: 'static>(
        window: &mut Window,
        cx: &mut Context<T>,
        position: Point<Pixels>,
        action_context: Option<FocusHandle>,
        build: impl FnOnce(PopupMenu) -> PopupMenu + 'static,
        on_close: fn(&mut T, &mut Context<T>),
    ) -> Self {
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            let menu = match action_context {
                Some(focus) => menu.action_context(focus),
                None => menu,
            };
            build(menu)
        });
        let menu_focus = menu.focus_handle(cx);
        let subscriptions = [
            cx.subscribe_in(&menu, window, move |this, _, _: &DismissEvent, _, cx| {
                on_close(this, cx)
            }),
            cx.on_blur(&menu_focus, window, move |this, _, cx| on_close(this, cx)),
        ];
        menu_focus.focus(window, cx);
        Self {
            menu,
            position,
            _subscriptions: subscriptions,
        }
    }

    /// The menu, anchored at its position above the other elements.
    pub(crate) fn element(&self) -> impl IntoElement + use<> {
        deferred(
            anchored()
                .position(self.position)
                .snap_to_window_with_margin(px(8.))
                .child(self.menu.clone()),
        )
        .with_priority(1)
    }
}
