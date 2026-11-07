use gpui::{
    AppContext, EntityInputHandler, ScrollDelta, ScrollWheelEvent, TestAppContext, point, px, size,
};

use super::super::{
    BufferModel, Editor, EditorView, EditorViewState, layout::Row, line_end, to_flat_byte,
};
use crate::app::settings::{self, ApplicationSettings, EDITOR_SOFT_WRAP, SettingsScope};

fn wrap_everything(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mut settings = ApplicationSettings::default();
        settings
            .0
            .set(EDITOR_SOFT_WRAP, true.into(), SettingsScope::default())
            .unwrap();
        cx.set_global(settings);
    });
}

/// `count` words, `w00 w01 …`, which take four bytes with their space
/// below a hundred.
fn words(count: usize) -> String {
    (0..count)
        .map(|word| format!("w{word:02}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_row_shows_the_part_of_a_span_between_its_positions() {
    let row = |range, last| Row {
        index: 0,
        line: 1,
        range,
        last,
        indent: 0,
    };
    let middle = row(4..8, false);
    assert_eq!(middle.span((1, 5), (1, 6)), Some((Some(5), Some(6))));
    assert_eq!(middle.span((0, 0), (2, 0)), Some((None, None)));
    // A span ending where the next row starts covers this row to its end.
    assert_eq!(middle.span((1, 2), (1, 8)), Some((None, None)));
    assert_eq!(middle.span((1, 8), (2, 0)), None);
    assert_eq!(middle.span((1, 0), (1, 4)), None);
    assert!(middle.holds((1, 4)) && !middle.holds((1, 8)));
    let last = row(8..12, true);
    assert_eq!(last.span((1, 8), (1, 12)), Some((Some(8), Some(12))));
    assert_eq!(last.span((1, 12), (2, 0)), Some((Some(12), None)));
    assert!(last.holds((1, 12)));
}

#[gpui::test]
fn wrapped_rows_place_the_caret_clicks_ime_bounds_and_line_numbers(cx: &mut TestAppContext) {
    wrap_everything(cx);
    let model = cx.new(|_| BufferModel::from_text(format!("{}\nshort\n{}", words(60), words(60))));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.simulate_resize(size(px(400.), px(300.)));
    cx.run_until_parked();
    let state = cx.read(|cx| editor.read(cx).editor().clone());
    let second = state.update(cx, |state, cx| {
        let width = state.wrap.width().expect("the view wraps");
        let rows = state.wrap.rows(0).len();
        assert!(rows > 2);
        assert_eq!(state.wrap.rows(1).len(), 1);
        assert_eq!(state.wrap.row_count(), 2 * rows + 1);
        let model = state.model.read(cx);
        let (first, second) = (state.row(model, 0), state.row(model, 1));
        // Rows break after whole words and their space.
        assert_eq!(first.range.end, second.range.start);
        assert_eq!(first.range.len() % 4, 0);
        assert!(first.range.len() <= width.columns + 1);
        assert!(first.range.len() + 3 > width.columns);

        state.cursor_col = second.range.start;
        assert_eq!(state.caret_row(), 1);
        second
    });
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            let state = view.editor().read(cx);
            let rows = state.wrap.rows(0).len();
            assert_eq!(view.caret_x(state, window, cx), px(0.));

            let text = view.text_bounds();
            let line_height = view.metrics.line_height;
            let y = |row: usize| text.origin.y + px((row as f32 + 0.5) * line_height);
            let hit = |x, row| view.hit_test(state, point(text.origin.x + x, y(row)), window, cx);
            assert_eq!(hit(px(1.), 1), (0, second.range.start));
            // Past a row's end stays on it, before the position the next row
            // starts with.
            assert_eq!(
                hit(text.size.width - px(1.), 0),
                (0, second.range.start - 1)
            );
            assert_eq!(hit(px(1.), rows), (1, 0));
            assert_eq!(hit(text.size.width - px(1.), rows), (1, 5));
            assert_eq!(hit(px(1.), rows + 1), (2, 0));

            let numbers = view.gutter_numbers(state, 0..state.wrap.row_count());
            let numbers = numbers
                .iter()
                .map(|(row, label, _)| (*row, *label))
                .collect::<Vec<_>>();
            assert_eq!(numbers, [(0, 1), (rows, 2), (rows + 1, 3)]);
            assert!(view.gutter_numbers(state, 1..rows).is_empty());

            let caret = second.range.start;
            let bounds = view
                .bounds_for_range(caret..caret + 3, text, window, cx)
                .unwrap();
            assert_eq!(bounds.origin.y, text.origin.y + px(line_height));
            assert_eq!(bounds.origin.x, text.origin.x);
            assert!(bounds.size.width > px(0.));
        })
    });
}

#[gpui::test]
fn scrolling_counts_rows_and_keeps_the_top_row_across_resizes(cx: &mut TestAppContext) {
    wrap_everything(cx);
    let text = vec![words(60); 40].join("\n");
    let model = cx.new(|_| BufferModel::from_text(text));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.simulate_resize(size(px(400.), px(300.)));
    cx.run_until_parked();
    let state = cx.read(|cx| editor.read(cx).editor().clone());
    let top = state.update(cx, |view, cx| {
        let rows = view.wrap.rows(0).len();
        view.scroll_top = 1e9;
        view.clamp_scroll();
        assert_eq!(
            view.scroll_top,
            view.wrap.row_count() as f32 - view.viewport_rows
        );
        assert_eq!(view.visible_rows().end, view.wrap.row_count());

        // The view state's top is the start of the top row, mid-line.
        view.scroll_top = (3 * rows + 2) as f32;
        let model = view.model.read(cx);
        let start = view.wrap.row_range(3, 2, line_end(model, 3)).start;
        let top = to_flat_byte(model, 3, start);
        // A resize keeps the caret visible, so park it on the top row.
        (view.cursor_line, view.cursor_col) = (3, start);
        assert_eq!(view.view_state(cx).top, top);
        // The top row is mid-line, so the first wholly visible row is the
        // next one, still on line 3.
        assert_eq!(view.row_start(view.viewport().0, cx).0, 3);
        top
    });

    // Sideways scrolling stays off while rows fit the width.
    cx.simulate_event(ScrollWheelEvent {
        position: cx.read(|cx| editor.read(cx).interaction_bounds().center()),
        delta: ScrollDelta::Pixels(point(px(-200.), px(0.))),
        ..Default::default()
    });
    cx.run_until_parked();
    assert_eq!(cx.read(|cx| state.read(cx).scroll_x), 0.);

    // Narrowing rewraps into more rows and keeps the top row's text on top.
    let rows = cx.read(|cx| state.read(cx).wrap.row_count());
    cx.simulate_resize(size(px(250.), px(300.)));
    cx.run_until_parked();
    cx.read(|cx| {
        let view = state.read(cx);
        assert!(view.wrap.row_count() > rows);
        let model = view.model.read(cx);
        let (line, sub) = view.wrap.position(view.visible_rows().start);
        let range = view.wrap.row_range(line, sub, line_end(model, line));
        assert_eq!(line, 3);
        assert!((range.start..range.end).contains(&(top - to_flat_byte(model, 3, 0))));
    });
}

#[gpui::test]
fn a_state_restored_before_the_first_layout_keeps_its_row(cx: &mut TestAppContext) {
    wrap_everything(cx);
    let text = vec![words(400); 30].join("\n");
    let model = cx.new(|_| BufferModel::from_text(text));
    let (measured, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    cx.run_until_parked();
    let measured = cx.read(|cx| measured.read(cx).editor().clone());
    let state = measured.update(cx, |view, cx| {
        assert!(view.wrap.rows(0).len() > 2);
        view.scroll_top = (view.wrap.first_row(5) + 2) as f32;
        EditorViewState {
            anchor: 0,
            caret: 0,
            ..view.view_state(cx)
        }
    });
    let (restored, cx) = cx.add_window_view(|_, cx| {
        let view = EditorView::new(model, cx);
        view.editor()
            .update(cx, |editor, cx| editor.restore_view_state(state, cx));
        view
    });
    cx.run_until_parked();
    cx.read(|cx| {
        let view = restored.read(cx).editor().read(cx);
        assert_eq!(view.view_state(cx).top, state.top);
        assert_eq!(view.visible_rows().start, view.wrap.first_row(5) + 2);
    });
}

#[gpui::test]
fn toggle_soft_wrap_writes_the_document_value_every_view_follows(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let document = uuid::Uuid::new_v4();
    let model = cx.new(|_| {
        let mut model = BufferModel::from_text(words(400));
        model.set_document(document);
        model
    });
    let first = cx.add_window(|_, cx| EditorView::new(model.clone(), cx));
    let second = cx.add_window(|_, cx| EditorView::new(model.clone(), cx));
    let other =
        cx.add_window(|_, cx| EditorView::new(cx.new(|_| BufferModel::from_text(words(400))), cx));
    cx.run_until_parked();
    let wraps = |cx: &mut TestAppContext| {
        [first, second, other].map(|window| {
            window
                .read_with(cx, |view, cx| view.editor().read(cx).wrap.width().is_some())
                .unwrap()
        })
    };
    assert_eq!(wraps(cx), [false; 3]);

    let toggle = |cx: &mut TestAppContext| {
        first
            .update(cx, |view, _, cx| {
                view.editor().clone().update(cx, |editor, cx| {
                    editor.execute_editing_command("editor.toggle-soft-wrap", cx)
                })
            })
            .unwrap()
    };
    assert!(toggle(cx));
    cx.run_until_parked();
    assert_eq!(wraps(cx), [true, true, false]);
    assert!(toggle(cx));
    cx.run_until_parked();
    assert_eq!(wraps(cx), [false; 3]);

    // Releasing the document drops its value.
    assert!(toggle(cx));
    cx.update(|cx| settings::release_document(document, cx));
    let scope = SettingsScope {
        document: Some(document.to_string()),
        ..Default::default()
    };
    assert!(!cx.read(|cx| settings::editor_soft_wrap(&scope, cx)));

    // A view without a document has nothing to write the value for.
    let frame = cx.add_window(|_, cx| {
        EditorView::for_contributed_view(cx.new(|_| BufferModel::from_text("a")), "frame", cx)
    });
    assert!(
        !frame
            .update(cx, |view, _, cx| {
                view.editor().clone().update(cx, |editor, cx| {
                    editor.execute_editing_command("editor.toggle-soft-wrap", cx)
                })
            })
            .unwrap()
    );
}

/// Run the movement `command` from the caret position `from`.
fn run(
    view: &mut Editor,
    from: (usize, usize),
    command: &str,
    cx: &mut gpui::Context<Editor>,
) -> (usize, usize) {
    (view.cursor_line, view.cursor_col) = from;
    view.has_selection = false;
    view.preferred_column = None;
    assert!(view.execute_editing_command(command, cx), "{command}");
    (view.cursor_line, view.cursor_col)
}

#[gpui::test]
fn visual_movement_follows_rows_and_logical_twins_follow_lines(cx: &mut TestAppContext) {
    wrap_everything(cx);
    let long = words(60);
    let text = format!("{long}\nshort\n{}", [long.as_str(); 11].join("\n"));
    let model = cx.new(|_| BufferModel::from_text(text));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.simulate_resize(size(px(400.), px(300.)));
    cx.run_until_parked();
    let state = cx.read(|cx| editor.read(cx).editor().clone());
    state.update(cx, |view, cx| {
        {
            let model = view.model.read(cx);
            let rows = view.wrap.rows(0).len();
            let [first, second, third, last] =
                [0, 1, 2, rows - 1].map(|index| view.row(model, index));
            let end = line_end(model, 0);
            let page = view.viewport_rows.floor() as usize;
            let last_line = model.line_count() - 1;
            let late_row = view.wrap.first_row(last_line);
            let on_second = (0, second.range.start + 16);

            // Up and Down keep the caret's x across rows; the twins move by
            // lines and land on the target line's first row.
            assert_eq!(run(view, on_second, "editor.move-up", cx), (0, 16));
            assert_eq!(
                run(view, on_second, "editor.move-down", cx),
                (0, third.range.start + 16)
            );
            assert_eq!(run(view, on_second, "editor.move-down-logical", cx), (1, 5));
            assert_eq!(
                run(view, (1, 2), "editor.move-up", cx),
                (0, last.range.start + 2)
            );
            assert_eq!(run(view, (1, 2), "editor.move-up-logical", cx), (0, 2));
            assert_eq!(run(view, (2, 4), "editor.move-up", cx).0, 1);
            // The preferred x survives the short line on the way down.
            run(view, on_second, "editor.move-down", cx);
            for _ in 1..rows {
                view.execute_editing_command("editor.move-down", cx);
            }
            assert_eq!((view.cursor_line, view.cursor_col), (2, 16));

            // Home and End reach the caret's row edges; a row that does not
            // end its line ends before the space its break follows.
            assert_eq!(
                run(view, on_second, "editor.move-line-start", cx),
                (0, second.range.start)
            );
            assert_eq!(
                run(view, on_second, "editor.move-smart-home", cx),
                (0, second.range.start)
            );
            assert_eq!(
                run(view, on_second, "editor.move-line-end", cx),
                (0, second.range.end - 1)
            );
            assert_eq!(
                run(view, (0, 4), "editor.move-line-end", cx),
                (0, first.range.end - 1)
            );
            assert_eq!(
                run(view, (0, last.range.start), "editor.move-line-end", cx),
                (0, end)
            );
            assert_eq!(
                run(view, on_second, "editor.move-line-start-logical", cx),
                (0, 0)
            );
            assert_eq!(
                run(view, on_second, "editor.move-smart-home-logical", cx),
                (0, 0)
            );
            assert_eq!(
                run(view, on_second, "editor.move-line-end-logical", cx),
                (0, end)
            );
            run(view, on_second, "editor.select-line-end", cx);
            assert_eq!(
                view.selected_byte_range(cx),
                Some(on_second.1..second.range.end - 1)
            );

            // Pages count rows, and the twins count lines.
            run(view, (0, 0), "editor.move-page-down", cx);
            assert_eq!(view.caret_row(), page);
            assert_eq!(
                run(view, (0, 0), "editor.move-page-down-logical", cx).0,
                page.min(last_line)
            );
            run(view, (last_line, 0), "editor.move-page-up", cx);
            assert_eq!(view.caret_row(), late_row - page);
            assert_eq!(
                run(view, (last_line, 0), "editor.move-page-up-logical", cx).0,
                last_line.saturating_sub(page)
            );
        }
    });
}

#[gpui::test]
fn without_wrapping_each_movement_and_its_twin_agree(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let text = (0..60)
        .map(|line| format!("    line {line} {}", words(line % 7)))
        .collect::<Vec<_>>()
        .join("\n");
    let model = cx.new(|_| BufferModel::from_text(text));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.simulate_resize(size(px(400.), px(300.)));
    cx.run_until_parked();
    let state = cx.read(|cx| editor.read(cx).editor().clone());
    state.update(cx, |view, cx| {
        assert_eq!(view.wrap.width(), None);
        for movement in [
            "up",
            "down",
            "line-start",
            "smart-home",
            "line-end",
            "page-up",
            "page-down",
        ] {
            for prefix in ["move", "select"] {
                let mut results = Vec::new();
                for suffix in ["", "-logical"] {
                    (view.cursor_line, view.cursor_col) = (30, 6);
                    view.has_selection = false;
                    view.preferred_column = None;
                    let command = format!("editor.{prefix}-{movement}{suffix}");
                    assert!(view.execute_editing_command(&command, cx), "{command}");
                    results.push(view.selection(cx));
                }
                assert_eq!(results[0], results[1], "{prefix}-{movement}");
            }
        }
    });
}

#[gpui::test]
fn row_queries_and_scrolling_follow_wrapped_rows(cx: &mut TestAppContext) {
    wrap_everything(cx);
    let text = ["short", &words(60), "tail", &words(60), "end"].join("\n");
    let model = cx.new(|_| BufferModel::from_text(text));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.simulate_resize(size(px(400.), px(300.)));
    cx.run_until_parked();
    let state = cx.read(|cx| editor.read(cx).editor().clone());
    state.update(cx, |view, cx| {
        let long = view.wrap.rows(1).len();
        assert!(long > 3);
        let count = view.wrap.row_count();
        assert_eq!(count, 3 + 2 * long);
        view.viewport_rows = 4.;

        // Rows map to their lines, and past the end to the last line.
        assert_eq!(view.row_start(0, cx).0, 0);
        assert_eq!(view.row_start(1, cx).0, 1);
        assert_eq!(view.row_start(long, cx).0, 1);
        assert_eq!(view.row_start(long + 1, cx).0, 2);
        assert_eq!(view.row_start(1_000_000, cx).0, 4);
        // A continuation row starts where the line breaks.
        let second = view.wrap.row_range(1, 1, 239).start;
        assert_eq!(view.row_start(2, cx), (1, second));
        assert_eq!(view.row_start(long + 1, cx), (2, 0));

        // A column shows on its row; a break starts the next row.
        assert_eq!(view.row_of_position(1, 0, cx), Some(1));
        assert_eq!(view.row_of_position(1, second - 1, cx), Some(1));
        assert_eq!(view.row_of_position(1, second, cx), Some(2));
        assert_eq!(view.row_of_position(5, 0, cx), None);
        assert_eq!(view.row_of_position(0, 99, cx), None);

        // The viewport counts whole rows, with a long line at the top, in
        // the middle, and at the end of the text.
        view.scroll_by_rows(-100, cx);
        assert_eq!(view.viewport(), (0, 4));
        view.scroll_by_rows(2, cx);
        assert_eq!(view.viewport(), (2, 4));
        assert_eq!(view.row_start(view.viewport().0, cx).0, 1);
        view.scroll_by_rows(1_000, cx);
        assert_eq!(view.viewport(), (count - 4, 4));
        assert_eq!(view.row_start(view.viewport().0 + 3, cx).0, 4);
        view.scroll_by_rows(-1, cx);
        assert_eq!(view.viewport().0, count - 5);

        // A scrolled view shows a partial top row only when scrolled by hand.
        view.scroll_top = 1.5;
        assert_eq!(view.viewport(), (2, 3));
    });
}
