use gpui::{AppContext, TestAppContext, point, px, size};

use super::super::{Block, BlockLine, Buffer, BufferModel, EditorView};

#[gpui::test]
fn block_rows_paint_apart_from_text_and_take_no_clicks(cx: &mut TestAppContext) {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("zero\none\ntwo")));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.simulate_resize(size(px(400.), px(300.)));
    cx.run_until_parked();
    let line = |text: &str| BlockLine {
        text: text.into(),
        highlights: Vec::new(),
        key: Some("diff.removed.background".into()),
    };
    let state = cx.read(|cx| editor.read(cx).editor().clone());
    state.update(cx, |state, cx| {
        state.set_blocks(
            vec![Block {
                line: 1,
                lines: vec![line("removed one"), line("removed two")],
            }],
            cx,
        );
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            let state = view.editor().read(cx);
            assert_eq!(state.wrap().row_count(), 5);
            let text = view.text_bounds();
            let y = |row: usize| text.origin.y + px((row as f32 + 0.5) * view.metrics.line_height);
            let hit = |x, row| view.hit_test(state, point(text.origin.x + x, y(row)), window, cx);
            // zero, two block rows, one, two.
            assert_eq!(hit(px(1.), 0), (0, 0));
            assert_eq!(hit(px(1.), 3), (1, 0));
            assert_eq!(hit(px(100.), 3), (1, 3));
            // A point on a block row is the start of the line below it, however
            // far along the row.
            assert_eq!(hit(px(1.), 1), (1, 0));
            assert_eq!(hit(px(100.), 2), (1, 0));
            // The numbers sit on the text rows.
            let numbers = view.gutter_numbers(state, 0..state.wrap().row_count());
            let rows: Vec<_> = numbers.iter().map(|(row, ..)| *row).collect();
            assert_eq!(rows, [0, 3, 4]);
        })
    });
}
