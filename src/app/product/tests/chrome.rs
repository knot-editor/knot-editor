//! The chrome's shape follows the theme and the `chrome.*` settings: floating
//! rounded cards, or flat cards that meet at borders.
use super::operations::empty_product_window;
use super::*;
use crate::app::settings::{ApplicationSettings, SettingsScope};
use crate::app::theme::Chrome;
use gpui::{Pixels, VisualTestContext, px};
use knot_model::workbench::{Area, DockLocation, PaneId};

fn set_chrome(key: &str, value: f64, cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.default_global::<ApplicationSettings>()
            .0
            .set(
                &format!("chrome.{key}"),
                serde_json::json!(value),
                SettingsScope::default(),
            )
            .unwrap();
        cx.refresh_windows();
    });
}

/// The dock's right edge, the editor pane's left edge, and the dock divider's
/// width, once the layout has settled.
fn measure(
    native_window: gpui::AnyWindowHandle,
    cx: &mut TestAppContext,
) -> (Pixels, Pixels, Pixels) {
    cx.run_until_parked();
    cx.update_window(native_window, |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    let mut visual = VisualTestContext::from_window(native_window, cx);
    let dock = visual.debug_bounds("dock-0").unwrap();
    let center = visual.debug_bounds("pane-1").unwrap();
    let divider = visual.debug_bounds("dock-divider-0").unwrap();
    (dock.right(), center.left(), divider.size.width)
}

#[gpui::test]
fn islands_leave_a_gap_between_cards_and_flat_cards_meet_at_borders(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let (window, native_window) = empty_product_window(&documents, cx);
    toggle_test_tool(&window, cx);

    let islands = Chrome::default();
    let (dock_right, center_left, handle) = measure(native_window, cx);
    assert_eq!(
        center_left - dock_right,
        px(islands.card_gap + islands.card_border)
    );
    assert_eq!(handle, px(islands.card_gap));

    set_chrome("cardGap", 0., cx);
    set_chrome("cardBorder", 1., cx);
    set_chrome("cardInset", 0., cx);
    set_chrome("cardRadius", 0., cx);
    let (dock_right, center_left, handle) = measure(native_window, cx);
    // No gap: the center card starts where the dock's ends, and its content
    // starts after the card's own border. The handle stays grabbable.
    assert_eq!(center_left - dock_right, px(1.));
    assert_eq!(handle, px(crate::app::tokens::chrome::MIN_RESIZE_HANDLE));
}

#[gpui::test]
fn settings_override_the_theme_and_a_negative_value_follows_it(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(ApplicationSettings::default());
        assert_eq!(crate::app::theme::chrome(cx), Chrome::default());
    });
    set_chrome("cardRadius", 12., cx);
    cx.update(|cx| assert_eq!(crate::app::theme::chrome(cx).card_radius, 12.));
    set_chrome("cardRadius", -1., cx);
    cx.update(|cx| assert_eq!(crate::app::theme::chrome(cx), Chrome::default()));
}

#[gpui::test]
fn only_the_focused_pane_draws_the_focus_ring(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let (window, native_window) = empty_product_window(&documents, cx);
    toggle_test_tool(&window, cx);

    let pane = |area, cx: &mut TestAppContext| {
        window.update(cx, |window, cx| {
            window
                .workbench
                .read(cx)
                .panes()
                .iter()
                .find(|pane| pane.area() == area)
                .map(|pane| pane.id())
                .unwrap()
        })
    };
    let center = pane(Area::Center, cx);
    let dock = pane(Area::Dock(DockLocation::Left), cx);
    for (focused, other) in [(center, dock), (dock, center)] {
        window.update(cx, |window, cx| {
            window
                .workbench
                .update(cx, |workbench, _| workbench.focus_pane(focused));
            cx.notify();
        });
        cx.run_until_parked();
        cx.update_window(native_window, |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(native_window, cx);
        let ring = visual.debug_bounds("focus-ring").unwrap();
        let bounds = |pane: PaneId, visual: &mut VisualTestContext| {
            visual
                .debug_bounds(Box::leak(format!("pane-{}", pane.value()).into_boxed_str()))
                .unwrap()
        };
        let focused = bounds(focused, &mut visual);
        assert_eq!(ring, focused);
        assert_ne!(ring.origin, bounds(other, &mut visual).origin);
    }
}
