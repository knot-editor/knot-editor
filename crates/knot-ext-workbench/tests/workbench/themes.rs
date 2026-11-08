//! The bundled themes, painting gpui-component as well as native surfaces.
use gpui::TestAppContext;

use super::bundled::PACKAGES;
use knot::test_support::TestProduct;

#[gpui::test]
fn components_take_the_bundled_theme_colors_and_follow_a_switch(cx: &mut TestAppContext) {
    let product = TestProduct::start(PACKAGES, cx);
    product.apply_startup_theme(cx);
    assert!(product.theme_is_dark(cx));
    assert_eq!(product.component_theme_drift(cx), Vec::<String>::new());
    let dark_popover = product.theme_color("popover.background", cx);

    product.set_setting("theme.mode", "light".into(), cx);
    assert!(!product.theme_is_dark(cx));
    assert_ne!(product.theme_color("popover.background", cx), dark_popover);
    assert_eq!(product.component_theme_drift(cx), Vec::<String>::new());
}
