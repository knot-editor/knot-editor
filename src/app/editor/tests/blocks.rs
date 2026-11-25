use gpui::{AppContext, TestAppContext, point, px, size};

use std::sync::Arc;

use knot_protocol::{ExtensionId, ExtensionKey, ExtensionLifecycleId};

use super::super::{Block, BlockLine, BlockRows, Buffer, BufferModel, Editor, EditorView};
use crate::app::syntax::{SyntaxLanguage, SyntaxState};

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

fn owner() -> ExtensionKey {
    ExtensionKey::new(ExtensionId::new(1), ExtensionLifecycleId::new(1))
}

/// The text of each block row, with its band and highlighted words, in order.
fn block_texts(editor: &Editor) -> Vec<(usize, String, Option<String>, usize)> {
    let wrap = editor.wrap();
    (0..wrap.row_count())
        .filter_map(|row| wrap.block_row(row))
        .map(|row| {
            (
                row.line,
                row.block_line.text[row.range.clone()].to_owned(),
                row.block_line.key.clone(),
                row.block_line.highlights.len(),
            )
        })
        .collect()
}

#[gpui::test]
fn source_rows_show_lines_of_another_buffer_with_its_highlighting(cx: &mut TestAppContext) {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("zero\none\ntwo")));
    let source = cx.new(|_| Buffer::new(BufferModel::from_text("fn a() {}\nfn b() {}\nplain\n")));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    cx.simulate_resize(size(px(400.), px(300.)));
    let language = SyntaxLanguage::new("rust", "\"fn\" @keyword\n", "").unwrap();
    source.update(cx, |source, cx| {
        source.set_syntax(
            Some(SyntaxState::new(Arc::new(language), Arc::new(|_| None))),
            cx,
        )
    });
    cx.run_until_parked();
    let state = cx.read(|cx| editor.read(cx).editor().clone());
    let rows = |lines| BlockRows {
        line: 2,
        source: source.clone(),
        lines,
        key: Some("diff.removed.background".into()),
    };
    state.update(cx, |state, cx| {
        // The range clamps to the source's lines; the last is the empty one
        // after the trailing newline.
        state.set_block_rows(owner(), vec![rows(1..9)], cx);
    });
    let band = Some("diff.removed.background".to_owned());
    cx.read(|cx| {
        let state = state.read(cx);
        assert_eq!(
            block_texts(state),
            [
                (2, "fn b() {}".into(), band.clone(), 1),
                (2, "plain".into(), band.clone(), 0),
                (2, String::new(), band.clone(), 0),
            ]
        );
        assert_eq!(state.wrap().row_count(), 6);
    });

    // A change of the source's text shows at once.
    source.update(cx, |source, cx| {
        source.replace(0..9, "fn first() {}").unwrap();
        cx.notify();
    });
    state.update(cx, |state, cx| {
        state.set_block_rows(owner(), vec![rows(0..1)], cx)
    });
    cx.read(|cx| {
        assert_eq!(
            block_texts(state.read(cx)),
            [(2, "fn first() {}".into(), band.clone(), 1)]
        );
    });
    source.update(cx, |source, cx| {
        source.replace(0..13, "gone").unwrap();
        cx.notify();
    });
    cx.read(|cx| {
        assert_eq!(
            block_texts(state.read(cx)),
            [(2, "gone".into(), band.clone(), 0)]
        );
    });

    // The rows follow their line through edits above it.
    model.update(cx, |model, cx| {
        model.replace(0..0, "new\n").unwrap();
        cx.notify();
    });
    cx.read(|cx| {
        assert_eq!(
            block_texts(state.read(cx)),
            [(3, "gone".into(), band.clone(), 0)]
        );
    });

    // The application's blocks stay beside them, and clearing the owner's
    // rows leaves only those.
    state.update(cx, |state, cx| {
        state.set_blocks(
            vec![Block {
                line: 0,
                lines: vec![BlockLine {
                    text: "native".into(),
                    highlights: Vec::new(),
                    key: None,
                }],
            }],
            cx,
        );
    });
    cx.read(|cx| assert_eq!(block_texts(state.read(cx)).len(), 2));
    state.update(cx, |state, cx| state.remove_owner_state(owner(), cx));
    cx.read(|cx| {
        assert_eq!(block_texts(state.read(cx)), [(0, "native".into(), None, 0)]);
    });
}

#[gpui::test]
fn source_rows_gain_highlights_when_the_sources_syntax_arrives(cx: &mut TestAppContext) {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("zero")));
    let source = cx.new(|_| Buffer::new(BufferModel::from_text("fn a() {}")));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    let state = cx.read(|cx| editor.read(cx).editor().clone());
    state.update(cx, |state, cx| {
        let rows = BlockRows {
            line: 0,
            source: source.clone(),
            lines: 0..1,
            key: None,
        };
        state.set_block_rows(owner(), vec![rows], cx);
    });
    cx.read(|cx| assert_eq!(block_texts(state.read(cx))[0].3, 0));
    let language = SyntaxLanguage::new("rust", "\"fn\" @keyword\n", "").unwrap();
    source.update(cx, |source, cx| {
        source.set_syntax(
            Some(SyntaxState::new(Arc::new(language), Arc::new(|_| None))),
            cx,
        )
    });
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(block_texts(state.read(cx))[0].3, 1));
}
