//! The matched-bracket highlight and Jump to Matching Bracket.

use gpui::{AppContext, TestAppContext};

use super::indentation::rust_editor;
use super::{BufferModel, EditorView};
use crate::app::settings::ApplicationSettings;

const JUMP: &str = "editor.jump-to-matching-bracket";

//                  0123456789012345
const SOURCE: &str = "fn f(a) { g(); }";

#[gpui::test]
fn the_bracket_at_the_caret_and_its_partner_are_highlighted(cx: &mut TestAppContext) {
    let (editor, _, cx) = rust_editor(cx, "", SOURCE, false);
    cx.update(|_, cx| {
        editor.update(cx, |view, cx| {
            let mut at = |col| {
                view.cursor_col = col;
                view.matched_brackets(view.model.read(cx))
            };
            assert_eq!(at(5), Some([4..5, 6..7]));
            assert_eq!(at(4), Some([4..5, 6..7]));
            assert_eq!(at(7), Some([6..7, 4..5]));
            // The bracket before the caret wins.
            assert_eq!(at(12), Some([11..12, 12..13]));
            assert_eq!(at(10), None);
            assert_eq!(at(16), Some([15..16, 8..9]));
        })
    });
}

#[gpui::test]
fn the_highlight_follows_edits(cx: &mut TestAppContext) {
    let (editor, _, cx) = rust_editor(cx, "", SOURCE, false);
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.cursor_col = 5;
            assert_eq!(
                view.matched_brackets(view.model.read(cx)),
                Some([4..5, 6..7])
            );
            view.cursor_col = 0;
            view.replace_text_with_grouping(None, "x", None, window, cx);
            view.cursor_col = 6;
            assert_eq!(
                view.matched_brackets(view.model.read(cx)),
                Some([5..6, 7..8])
            );
        })
    });
}

#[gpui::test]
fn jump_moves_before_the_partner_and_back(cx: &mut TestAppContext) {
    let (editor, _, cx) = rust_editor(cx, "", SOURCE, false);
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            let mut jump = |view: &mut EditorView| {
                view.execute_editing_command(JUMP, window, cx);
                (view.cursor_line, view.cursor_col, view.has_selection)
            };
            view.cursor_col = 5;
            assert_eq!(jump(view), (0, 6, false));
            assert_eq!(jump(view), (0, 4, false));
            assert_eq!(jump(view), (0, 6, false));
            // Between an adjacent pair, before the opener.
            view.cursor_col = 12;
            assert_eq!(jump(view), (0, 11, false));
            // Away from brackets, before the enclosing closer.
            view.cursor_col = 10;
            assert_eq!(jump(view), (0, 15, false));
            // With a selection, from the caret, clearing the selection.
            view.anchor_col = 0;
            view.cursor_col = 5;
            view.has_selection = true;
            assert_eq!(jump(view), (0, 6, false));
        })
    });
}

#[gpui::test]
fn a_document_without_a_language_matches_nothing(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let model = cx.new(|_| BufferModel::from_text(SOURCE));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.cursor_col = 5;
            assert_eq!(view.matched_brackets(view.model.read(cx)), None);
            view.execute_editing_command(JUMP, window, cx);
            assert_eq!((view.cursor_line, view.cursor_col), (0, 5));
        })
    });
}
