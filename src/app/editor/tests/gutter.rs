use std::{cell::RefCell, rc::Rc};

use gpui::{
    AppContext, Entity, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point,
    TestAppContext, VisualTestContext, point, px,
};

use super::super::{
    Buffer, BufferModel, ContributionSource, EditorContributionAction, EditorView, GutterLayout,
    gutter::GutterLane,
};
use crate::app::settings::{
    ApplicationSettings, EDITOR_FOLDING, EDITOR_LINE_NUMBERS, EDITOR_SHOW_FOLDING_CONTROLS,
    Folding, LineNumbers, SettingsScope,
};
use knot_protocol::{ByteRange, EditorContribution};

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
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("a\nb\nc")));
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
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(text.as_str())));
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
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("a\nb\nc\nd\ne")));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        editor
            .read(cx)
            .editor()
            .clone()
            .update(cx, |editor, cx| editor.place_for_test((2, 0), (2, 0), cx))
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
            let caret = editor.caret();
            editor.place_for_test((1, 0), caret, cx)
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
            GutterLane::Folds => view.gutter.folds_start + 2.,
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
    press_with(
        cx,
        editor,
        position,
        Modifiers {
            shift,
            ..Modifiers::default()
        },
    )
}

fn press_with(
    cx: &mut VisualTestContext,
    editor: &Entity<EditorView>,
    position: Point<Pixels>,
    modifiers: Modifiers,
) -> Option<std::ops::Range<usize>> {
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.on_mouse_down(
                &MouseDownEvent {
                    position,
                    modifiers,
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
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("one\ntwo\nthree")));
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
        Buffer::new(model)
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

fn set_setting(cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| {
        cx.global_mut::<ApplicationSettings>()
            .0
            .set(key, value, SettingsScope::default())
            .unwrap();
    });
    cx.run_until_parked();
}

const NESTED: &str = "a\n  b\n    c\n      d\n    e\n  f\ng";

#[gpui::test]
fn the_fold_lane_toggles_folds(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(NESTED)));
    let (view, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    let folds = |cx: &mut VisualTestContext| cx.read(|cx| view.read(cx).editor().read(cx).folds());
    let chevron = |cx: &mut VisualTestContext, line| lane_point(cx, &view, GutterLane::Folds, line);
    let click = |cx: &mut VisualTestContext, line, alt| {
        let at = chevron(cx, line);
        press_with(
            cx,
            &view,
            at,
            Modifiers {
                alt,
                ..Modifiers::default()
            },
        );
        cx.refresh().unwrap();
    };

    // A click collapses the region on the line, a second one expands it, and
    // neither selects text.
    click(cx, 0, false);
    assert_eq!(folds(cx), vec![0..6]);
    cx.read(|cx| {
        assert!(
            view.read(cx)
                .editor()
                .read(cx)
                .selected_byte_range(cx)
                .is_none()
        )
    });
    click(cx, 0, false);
    assert!(folds(cx).is_empty());

    // Alt-click collapses the nested regions too, and expands them again.
    click(cx, 0, true);
    assert_eq!(folds(cx), [0..6, 1..5, 2..4]);
    click(cx, 0, true);
    assert!(folds(cx).is_empty());

    // A line that starts no region does nothing.
    click(cx, 3, false);
    assert!(folds(cx).is_empty());

    // The placeholder of a collapsed fold expands it.
    click(cx, 1, false);
    assert_eq!(folds(cx), vec![1..5]);
    let (bounds, line) = cx.read(|cx| view.read(cx).fold_placeholders[0]);
    assert_eq!(line, 1);
    press(cx, &view, bounds.center(), false);
    assert!(folds(cx).is_empty());
}

#[gpui::test]
fn fold_marks_list_visible_foldable_lines(cx: &mut TestAppContext) {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(NESTED)));
    let (view, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    let marks = |cx: &mut VisualTestContext| {
        cx.read(|cx| view.read(cx).editor().read(cx).fold_marks(0..7, cx))
    };
    assert_eq!(marks(cx), [(0, false), (1, false), (2, false)]);
    cx.update(|_, cx| {
        view.read(cx)
            .editor()
            .clone()
            .update(cx, |editor, cx| editor.fold(1..5, cx));
    });
    // The collapsed fold's nested region is hidden.
    assert_eq!(marks(cx), [(0, false), (1, true)]);
}

#[gpui::test]
fn folding_settings_shape_the_fold_lane(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(NESTED)));
    let (view, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    assert_eq!(gutter(cx, &view).folding, Folding::Mouseover);
    let with_lane = gutter(cx, &view).width;

    set_setting(cx, EDITOR_SHOW_FOLDING_CONTROLS, "always".into());
    assert_eq!(gutter(cx, &view).folding, Folding::Always);

    set_setting(cx, EDITOR_FOLDING, false.into());
    let off = gutter(cx, &view);
    assert_eq!(off.folding, Folding::Off);
    assert!(off.width < with_lane);
    assert_eq!(off.lane_at(off.numbers_end), Some(GutterLane::Numbers));
}

#[gpui::test]
fn the_pointer_over_the_gutter_is_tracked(cx: &mut TestAppContext) {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(NESTED)));
    let (view, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    let hover = |cx: &mut VisualTestContext, position| {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.on_mouse_move(
                    &MouseMoveEvent {
                        position,
                        modifiers: Modifiers::default(),
                        pressed_button: None,
                    },
                    window,
                    cx,
                );
                view.gutter_hovered
            })
        })
    };
    let in_gutter = lane_point(cx, &view, GutterLane::Numbers, 0);
    assert!(hover(cx, in_gutter));
    let in_text = cx.read(|cx| view.read(cx).text_bounds().center());
    assert!(!hover(cx, in_text));
}
