use gpui::{AppContext, TestAppContext, px, size};

use super::super::{Buffer, BufferModel, EditorView};
use crate::app::settings::{ApplicationSettings, EDITOR_RULERS, EDITOR_SOFT_WRAP, SettingsScope};

fn set_setting(key: &str, value: serde_json::Value, cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mut settings = ApplicationSettings::default();
        settings
            .0
            .set(key, value, SettingsScope::default())
            .unwrap();
        cx.set_global(settings);
    });
}

#[gpui::test]
fn a_view_draws_the_rulers_setting_unless_it_sets_its_own(cx: &mut TestAppContext) {
    set_setting(EDITOR_RULERS, serde_json::json!([50, 80]), cx);
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("text".to_owned())));
    let (view, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.read(|cx| assert_eq!(view.read(cx).ruler_columns(cx), [50, 80]));
    view.update(cx, |view, cx| view.set_rulers(Some(vec![72]), cx));
    cx.read(|cx| assert_eq!(view.read(cx).ruler_columns(cx), [72]));
    view.update(cx, |view, cx| view.set_rulers(Some(Vec::new()), cx));
    cx.read(|cx| assert!(view.read(cx).ruler_columns(cx).is_empty()));
    view.update(cx, |view, cx| view.set_rulers(None, cx));
    cx.read(|cx| assert_eq!(view.read(cx).ruler_columns(cx), [50, 80]));
}

#[gpui::test]
fn no_rulers_are_drawn_by_default(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("text".to_owned())));
    let (view, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.read(|cx| assert!(view.read(cx).ruler_columns(cx).is_empty()));
}

#[gpui::test]
fn a_ruler_follows_the_character_width_under_zoom_scroll_and_wrapping(cx: &mut TestAppContext) {
    for wrap in [false, true] {
        set_setting(EDITOR_SOFT_WRAP, wrap.into(), cx);
        let model = cx.new(|_| Buffer::new(BufferModel::from_text("text ".repeat(60))));
        let (view, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
        cx.simulate_resize(size(px(600.), px(300.)));
        cx.run_until_parked();
        let (cell, x) = cx.read(|cx| {
            let view = view.read(cx);
            (view.cell_width, view.ruler_x(72, 0.))
        });
        assert!(cell > 0.);
        assert_eq!(x, px(72. * cell));
        cx.read(|cx| assert_eq!(view.read(cx).ruler_x(72, 30.), px(72. * cell - 30.)));

        cx.update(|_, cx| crate::app::zoom::set_level(2, cx));
        cx.simulate_resize(size(px(600.), px(301.)));
        cx.run_until_parked();
        let zoomed = cx.read(|cx| view.read(cx).cell_width);
        assert!(zoomed > cell);
        cx.read(|cx| assert_eq!(view.read(cx).ruler_x(72, 0.), px(72. * zoomed)));
        cx.update(|_, cx| crate::app::zoom::set_level(0, cx));
    }
}
