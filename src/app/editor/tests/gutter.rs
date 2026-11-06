use std::{cell::RefCell, rc::Rc};

use gpui::{
    AppContext, Entity, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point,
    TestAppContext, VisualTestContext, point, px,
};

use super::super::{
    BufferModel, ContributionSource, EditorContributionAction, EditorView, GutterLayout,
    gutter::GutterLane,
};
use crate::app::settings::{ApplicationSettings, EDITOR_LINE_NUMBERS, LineNumbers, SettingsScope};
use crate::host::protocol::{ByteRange, EditorContribution};

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
                    document: None,
                },
            )
            .unwrap();
    });
    cx.run_until_parked();
}

fn gutter(cx: &mut VisualTestContext, editor: &Entity<EditorView>) -> GutterLayout {
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
        editor.read(cx).editor().clone().update(cx, |editor, cx| {
            editor.cursor_line = 2;
            editor.sync_view_position(cx);
        })
    });
    let numbers = |cx: &mut VisualTestContext| {
        cx.read(|cx| {
            let view = editor.read(cx);
            view.gutter_numbers(view.editor().read(cx), 0..5)
        })
    };
    let highlighted = |cx: &mut VisualTestContext| {
        cx.read(|cx| editor.read(cx).editor().read(cx).highlighted_line())
    };
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
    assert_eq!(highlighted(cx), Some(2));

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
        editor.read(cx).editor().clone().update(cx, |editor, cx| {
            (editor.anchor_line, editor.anchor_col) = (1, 0);
            editor.has_selection = true;
            editor.sync_view_position(cx);
        })
    });
    assert_eq!(highlighted(cx), None);
    assert!(numbers(cx).contains(&(2, 3, true)));

    set_line_numbers(cx, "off", None);
    assert!(numbers(cx).is_empty());
}

/// Where a click in `lane` lands on `line`.
fn lane_point(
    cx: &mut VisualTestContext,
    editor: &Entity<EditorView>,
    lane: GutterLane,
    line: usize,
) -> Point<Pixels> {
    cx.read(|cx| {
        let view = editor.read(cx);
        let bounds = view.interaction_bounds();
        let x = match lane {
            GutterLane::Markers => 4.,
            GutterLane::Numbers => view.gutter.markers_width + 2.,
        };
        point(
            bounds.origin.x + px(x),
            bounds.origin.y + px(line as f32 * view.metrics.line_height + 5.),
        )
    })
}

fn press(
    cx: &mut VisualTestContext,
    editor: &Entity<EditorView>,
    position: Point<Pixels>,
    shift: bool,
) -> Option<std::ops::Range<usize>> {
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.on_mouse_down(
                &MouseDownEvent {
                    position,
                    modifiers: Modifiers {
                        shift,
                        ..Modifiers::default()
                    },
                    button: MouseButton::Left,
                    click_count: 1,
                    first_mouse: false,
                },
                window,
                cx,
            );
            view.editor().read(cx).selected_byte_range(cx)
        })
    })
}

fn drag_to(
    cx: &mut VisualTestContext,
    editor: &Entity<EditorView>,
    position: Point<Pixels>,
) -> Option<std::ops::Range<usize>> {
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.on_mouse_move(
                &MouseMoveEvent {
                    position,
                    modifiers: Modifiers::default(),
                    pressed_button: Some(MouseButton::Left),
                },
                window,
                cx,
            );
            view.editor().read(cx).selected_byte_range(cx)
        })
    })
}

#[gpui::test]
fn line_numbers_select_whole_lines(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("one\ntwo\nthree"));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    let number =
        |cx: &mut VisualTestContext, line| lane_point(cx, &editor, GutterLane::Numbers, line);

    // A click selects the line with its line break, or to the end of text.
    let at = number(cx, 1);
    assert_eq!(press(cx, &editor, at, false), Some(4..8));
    let at = number(cx, 2);
    assert_eq!(press(cx, &editor, at, false), Some(8..13));

    // Shift extends from the selection's anchor by lines.
    let at = number(cx, 0);
    assert_eq!(press(cx, &editor, at, false), Some(0..4));
    let at = number(cx, 2);
    assert_eq!(press(cx, &editor, at, true), Some(0..13));

    // A drag extends by whole lines either way and keeps the clicked line.
    let at = number(cx, 1);
    assert_eq!(press(cx, &editor, at, false), Some(4..8));
    let at = number(cx, 2);
    assert_eq!(drag_to(cx, &editor, at), Some(4..13));
    let at = number(cx, 0);
    assert_eq!(drag_to(cx, &editor, at), Some(0..8));
}

#[gpui::test]
fn the_marker_lane_runs_the_top_marker_command_and_selects_nothing(cx: &mut TestAppContext) {
    let model = cx.new(|_| {
        let mut model = BufferModel::from_text("one\ntwo\nthree");
        let marker = |start, key: &str, command: Option<&str>| EditorContribution {
            range: ByteRange {
                start_byte_offset: start,
                end_byte_offset: start + 1,
            },
            decoration: None,
            gutter: Some(key.to_owned()),
            command: command.map(Into::into),
        };
        model
            .replace_contributions(
                ContributionSource::fixture(),
                &[
                    marker(0, "error", Some("fixture.error")),
                    marker(1, "info", Some("fixture.info")),
                    marker(4, "warning", None),
                ],
                0,
            )
            .unwrap();
        model
    });
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    let commands = Rc::new(RefCell::new(Vec::new()));
    let seen = commands.clone();
    cx.update(|_, cx| {
        cx.subscribe(&editor, move |_, action: &EditorContributionAction, _| {
            seen.borrow_mut().push(action.command.clone());
        })
        .detach();
    });

    let at = lane_point(cx, &editor, GutterLane::Markers, 0);
    assert_eq!(press(cx, &editor, at, false), None);
    cx.run_until_parked();
    assert_eq!(*commands.borrow(), ["fixture.error"]);

    // A marker without a command, or no marker, leaves the selection alone.
    let at = lane_point(cx, &editor, GutterLane::Numbers, 2);
    assert_eq!(press(cx, &editor, at, false), Some(8..13));
    for line in [1, 2] {
        let at = lane_point(cx, &editor, GutterLane::Markers, line);
        assert_eq!(press(cx, &editor, at, false), Some(8..13));
        let at = lane_point(cx, &editor, GutterLane::Numbers, 0);
        assert_eq!(drag_to(cx, &editor, at), Some(8..13));
    }
    assert_eq!(commands.borrow().len(), 1);
}
