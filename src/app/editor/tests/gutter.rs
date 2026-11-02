use gpui::{AppContext, TestAppContext, VisualTestContext, px};

use super::super::{BufferModel, EditorView, GutterLayout};
use crate::app::settings::{ApplicationSettings, EDITOR_LINE_NUMBERS, LineNumbers, SettingsScope};

fn set_line_numbers(cx: &mut VisualTestContext, value: &str, view: Option<&str>) {
    cx.update(|_, cx| {
        cx.global_mut::<ApplicationSettings>()
            .0
            .set(
                EDITOR_LINE_NUMBERS,
                value.into(),
                SettingsScope {
                    language: None,
                    view: view.map(Into::into),
                },
            )
            .unwrap();
    });
    cx.run_until_parked();
}

fn gutter(cx: &mut VisualTestContext, editor: &gpui::Entity<EditorView>) -> GutterLayout {
    cx.read(|cx| editor.read(cx).gutter)
}

#[gpui::test]
fn line_numbers_follow_the_setting_for_the_view(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let model = cx.new(|_| BufferModel::from_text("a\nb\nc"));
    let (editor_view, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    cx.run_until_parked();
    let on = gutter(cx, &editor_view);
    assert_eq!(on.line_numbers, LineNumbers::On);

    // A value scoped to another view leaves this one alone.
    set_line_numbers(cx, "off", Some("example.frame"));
    assert_eq!(gutter(cx, &editor_view), on);

    set_line_numbers(cx, "off", Some("editor"));
    let off = gutter(cx, &editor_view);
    assert_eq!(off.line_numbers, LineNumbers::Off);
    assert!(off.width < on.width);
    cx.read(|cx| {
        let view = editor_view.read(cx);
        let bounds = view.interaction_bounds();
        assert_eq!(view.text_bounds().origin.x - bounds.origin.x, px(off.width));
    });

    set_line_numbers(cx, "relative", Some("editor"));
    let relative = gutter(cx, &editor_view);
    assert_eq!(relative.line_numbers, LineNumbers::Relative);
    assert_eq!(relative.width, on.width);

    // Unknown text numbers lines as "on" does.
    set_line_numbers(cx, "sometimes", Some("editor"));
    assert_eq!(gutter(cx, &editor_view), on);
}

#[gpui::test]
fn the_number_lane_widens_with_the_line_count(cx: &mut TestAppContext) {
    let text = "x\n".repeat(98);
    let model = cx.new(|_| BufferModel::from_text(text.as_str()));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    cx.run_until_parked();
    let two_digits = gutter(cx, &editor);
    assert_eq!(cx.read(|cx| model.read(cx).line_count()), 99);

    model.update(cx, |model, cx| {
        let end = model.len();
        model.replace(end..end, "\n").unwrap();
        cx.notify();
    });
    cx.run_until_parked();
    let three_digits = gutter(cx, &editor);
    assert!(three_digits.width > two_digits.width);
    assert_eq!(
        three_digits.width - two_digits.width,
        three_digits.numbers_end - two_digits.numbers_end
    );
}

#[gpui::test]
fn numbers_and_the_line_highlight_follow_the_cursor(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let model = cx.new(|_| BufferModel::from_text("a\nb\nc\nd\ne"));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        editor.update(cx, |view, cx| {
            view.cursor_line = 2;
            view.sync_view_position(cx);
        })
    });
    let numbers = |cx: &mut VisualTestContext| cx.read(|cx| editor.read(cx).gutter_numbers(0..5));
    assert_eq!(
        numbers(cx),
        [
            (0, 1, false),
            (1, 2, false),
            (2, 3, true),
            (3, 4, false),
            (4, 5, false)
        ]
    );
    assert_eq!(cx.read(|cx| editor.read(cx).highlighted_line()), Some(2));

    set_line_numbers(cx, "relative", None);
    assert_eq!(
        numbers(cx),
        [
            (0, 2, false),
            (1, 1, false),
            (2, 3, true),
            (3, 1, false),
            (4, 2, false)
        ]
    );

    // A selection hides the line highlight but keeps the active number.
    cx.update(|_, cx| {
        editor.update(cx, |view, cx| {
            (view.anchor_line, view.anchor_col) = (1, 0);
            view.has_selection = true;
            view.sync_view_position(cx);
        })
    });
    assert_eq!(cx.read(|cx| editor.read(cx).highlighted_line()), None);
    assert!(numbers(cx).contains(&(2, 3, true)));

    set_line_numbers(cx, "off", None);
    assert!(numbers(cx).is_empty());
}
