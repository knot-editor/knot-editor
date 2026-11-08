//! The matched-bracket highlight, Jump to Matching Bracket, and typing
//! bracket pairs.

use gpui::{AppContext, Entity, TestAppContext};

use super::indentation::rust_editor;
use super::{BufferModel, Editor};
use crate::app::languages::{
    self, LanguageDefinition, LanguageOwner, LanguageQueries,
    editing::{BracketPair, EditingDefinition, SyntaxFamily},
};
use crate::app::settings::{
    ApplicationSettings, EDITOR_AUTO_CLOSING_BRACKETS, EDITOR_AUTO_SURROUND, SettingsScope,
};
use knot_protocol::{ExtensionId, ExtensionLifecycleId, LanguageInfo};

const JUMP: &str = "editor.jump-to-matching-bracket";

//                  0123456789012345
const SOURCE: &str = "fn f(a) { g(); }";

#[gpui::test]
fn the_bracket_at_the_caret_and_its_partner_are_highlighted(cx: &mut TestAppContext) {
    let (editor, _, cx) = rust_editor(cx, "", SOURCE, false);
    cx.update(|cx| {
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
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            view.cursor_col = 5;
            assert_eq!(
                view.matched_brackets(view.model.read(cx)),
                Some([4..5, 6..7])
            );
            view.cursor_col = 0;
            view.replace_text_with_grouping(None, "x", None, cx);
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
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            let mut jump = |view: &mut Editor| {
                view.execute_editing_command(JUMP, cx);
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
    let editor = cx.new(|cx| Editor::new(model, crate::app::EDITOR_KEY_CONTEXT, cx));
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            view.cursor_col = 5;
            assert_eq!(view.matched_brackets(view.model.read(cx)), None);
            view.execute_editing_command(JUMP, cx);
            assert_eq!((view.cursor_line, view.cursor_col), (0, 5));
        })
    });
}

/// An editor over `text` in Rust with string and comment highlights,
/// bracket pairs, a quote that does not close inside strings or comments,
/// and a single quote that only surrounds, with the caret at `caret`.
fn pairs_editor<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    caret: usize,
) -> (Entity<Editor>, &'a mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(ApplicationSettings::default());
        let pair = |open: &str, close: &str| BracketPair {
            open: open.into(),
            close: close.into(),
            auto_close: None,
            newline: None,
            not_in: None,
        };
        let rust = LanguageDefinition {
            info: LanguageInfo {
                id: "rust".into(),
                name: "Rust".into(),
                grammar: "rust".into(),
                file_extensions: Vec::new(),
                file_names: Vec::new(),
                interpreters: Vec::new(),
            },
            queries: LanguageQueries {
                highlights: "(string_literal) @string\n(line_comment) @comment\n(block_comment) @comment.documentation".into(),
                injections: String::new(),
                indents: String::new(),
                folds: String::new(),
            },
            editing: EditingDefinition {
                comments: None,
                brackets: Some(vec![
                    pair("{", "}"),
                    pair("(", ")"),
                    BracketPair {
                        not_in: Some(vec![SyntaxFamily::String, SyntaxFamily::Comment]),
                        ..pair("\"", "\"")
                    },
                    BracketPair {
                        auto_close: Some(false),
                        ..pair("'", "'")
                    },
                ]),
                auto_close_before: Some(";:.,=}])>".into()),
            },
        };
        let owner = LanguageOwner {
            extension: ExtensionId::new(1),
            lifecycle: ExtensionLifecycleId::new(1),
        };
        languages::register(owner, rust.compile().unwrap(), cx);
    });
    let model = cx.new(|_| BufferModel::from_text(text));
    cx.update(|cx| languages::set_language(&model, "rust".into(), cx));
    let editor = cx.new(|cx| Editor::new(model, crate::app::EDITOR_KEY_CONTEXT, cx));
    cx.update(|cx| editor.update(cx, |view, cx| move_to(view, caret, cx)));
    (editor, cx)
}

fn move_to(view: &mut Editor, offset: usize, cx: &mut gpui::Context<Editor>) {
    (view.cursor_line, view.cursor_col) = (0, offset);
    view.has_selection = false;
    view.finish_position_change(cx);
}

fn select(view: &mut Editor, anchor: usize, caret: usize, cx: &mut gpui::Context<Editor>) {
    (view.anchor_line, view.anchor_col) = (0, anchor);
    (view.cursor_line, view.cursor_col) = (0, caret);
    view.has_selection = anchor != caret;
    view.finish_position_change(cx);
}

/// Type `text` one character at a time.
fn type_text(view: &mut Editor, text: &str, cx: &mut gpui::Context<Editor>) {
    for character in text.chars() {
        view.type_text(None, &character.to_string(), cx);
    }
}

/// The text with `|` at the caret, or `[` and `]` around the selection.
fn marked(view: &Editor, cx: &gpui::App) -> String {
    let mut text = view.text(cx);
    let caret = view.cursor_col;
    if view.has_selection {
        let (start, end) = (view.anchor_col.min(caret), view.anchor_col.max(caret));
        text.insert(end, ']');
        text.insert(start, '[');
    } else {
        text.insert(caret, '|');
    }
    text
}

fn set(cx: &mut TestAppContext, key: &str, value: bool) {
    cx.update(|cx| {
        cx.global_mut::<ApplicationSettings>()
            .0
            .set(
                key,
                value.into(),
                SettingsScope {
                    language: Some("rust".into()),
                    view: None,
                    document: None,
                },
            )
            .unwrap();
    });
}

#[gpui::test]
fn an_opener_inserts_its_closer_as_one_undo_step(cx: &mut TestAppContext) {
    let (editor, cx) = pairs_editor(cx, "", 0);
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            type_text(view, "f(", cx);
            assert_eq!(marked(view, cx), "f(|)");
            type_text(view, "{", cx);
            assert_eq!(marked(view, cx), "f({|})");
            view.execute_editing_command("editor.undo", cx);
            assert_eq!(marked(view, cx), "f(|)");
            view.execute_editing_command("editor.undo", cx);
            assert_eq!(view.text(cx), "");
        })
    });
}

#[gpui::test]
fn an_opener_before_a_word_does_not_close(cx: &mut TestAppContext) {
    let (editor, cx) = pairs_editor(cx, "x", 0);
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            type_text(view, "(", cx);
            assert_eq!(marked(view, cx), "(|x");
        })
    });
}

#[gpui::test]
fn typing_a_tracked_closer_steps_over_it(cx: &mut TestAppContext) {
    let (editor, cx) = pairs_editor(cx, ";", 0);
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            type_text(view, "f((a", cx);
            assert_eq!(marked(view, cx), "f((a|));");
            type_text(view, ")", cx);
            assert_eq!(marked(view, cx), "f((a)|);");
            type_text(view, ")", cx);
            assert_eq!(marked(view, cx), "f((a))|;");
            type_text(view, ")", cx);
            assert_eq!(marked(view, cx), "f((a)))|;");
        })
    });
}

#[gpui::test]
fn an_untracked_closer_is_typed(cx: &mut TestAppContext) {
    let (editor, cx) = pairs_editor(cx, "f()", 2);
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            type_text(view, ")", cx);
            assert_eq!(marked(view, cx), "f()|)");
        })
    });
}

#[gpui::test]
fn leaving_the_pair_stops_tracking_it(cx: &mut TestAppContext) {
    let (editor, cx) = pairs_editor(cx, "", 0);
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            type_text(view, "(", cx);
            move_to(view, 0, cx);
            move_to(view, 1, cx);
            type_text(view, ")", cx);
            assert_eq!(marked(view, cx), "()|)");
            // A selection stops tracking too.
            move_to(view, 3, cx);
            type_text(view, " (", cx);
            select(view, 0, 4, cx);
            move_to(view, 5, cx);
            type_text(view, ")", cx);
            assert_eq!(marked(view, cx), "()) ()|)");
        })
    });
}

#[gpui::test]
fn backspace_between_a_tracked_pair_deletes_both(cx: &mut TestAppContext) {
    let (editor, cx) = pairs_editor(cx, "", 0);
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            type_text(view, "f(", cx);
            view.execute_editing_command("editor.delete-backward", cx);
            assert_eq!(marked(view, cx), "f|");
            // Once text was typed inside, the pair is no longer empty.
            type_text(view, "(a", cx);
            view.execute_editing_command("editor.delete-backward", cx);
            view.execute_editing_command("editor.delete-backward", cx);
            assert_eq!(marked(view, cx), "f|");
        })
    });
}

#[gpui::test]
fn backspace_between_an_untracked_pair_deletes_one_character(cx: &mut TestAppContext) {
    let (editor, cx) = pairs_editor(cx, "()", 1);
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            view.execute_editing_command("editor.delete-backward", cx);
            assert_eq!(marked(view, cx), "|)");
        })
    });
}

#[gpui::test]
fn an_opener_wraps_the_selection_and_keeps_it_selected(cx: &mut TestAppContext) {
    let (editor, cx) = pairs_editor(cx, "a bc d", 0);
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            select(view, 2, 4, cx);
            type_text(view, "(", cx);
            assert_eq!(marked(view, cx), "a ([bc]) d");
            assert!(view.cursor_col > view.anchor_col);
            // A pair that does not auto-close still wraps, and a reversed
            // selection stays reversed.
            select(view, 4, 3, cx);
            type_text(view, "'", cx);
            assert_eq!(marked(view, cx), "a ('[b]'c) d");
            assert!(view.cursor_col < view.anchor_col);
            view.execute_editing_command("editor.undo", cx);
            assert_eq!(marked(view, cx), "a ([b]c) d");
        })
    });
}

#[gpui::test]
fn quotes_do_not_close_inside_strings_or_comments(cx: &mut TestAppContext) {
    let text = "fn f() { let s = \"a b\"; } // c";
    let inside_string = text.find("a b").unwrap() + 1;
    let (editor, cx) = pairs_editor(cx, text, inside_string);
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            type_text(view, "\"", cx);
            assert_eq!(view.text(cx), "fn f() { let s = \"a\" b\"; } // c");
            let end = view.text(cx).len();
            move_to(view, end, cx);
            type_text(view, "\"", cx);
            assert!(view.text(cx).ends_with("// c\""));
            move_to(view, 8, cx);
            type_text(view, "\"", cx);
            assert!(view.text(cx).starts_with("fn f() {\"\" "));
        })
    });
}

#[gpui::test]
fn the_settings_turn_closing_and_wrapping_off(cx: &mut TestAppContext) {
    let (editor, cx) = pairs_editor(cx, "ab", 0);
    set(cx, EDITOR_AUTO_CLOSING_BRACKETS, false);
    set(cx, EDITOR_AUTO_SURROUND, false);
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            type_text(view, "(", cx);
            assert_eq!(marked(view, cx), "(|ab");
            select(view, 1, 3, cx);
            type_text(view, "(", cx);
            assert_eq!(marked(view, cx), "((|");
        })
    });
}

#[gpui::test]
fn composed_text_does_not_close(cx: &mut TestAppContext) {
    let (editor, cx) = pairs_editor(cx, "", 0);
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            view.mark_text(None, "\"", None, cx);
            view.type_text(None, "\"", cx);
            assert_eq!(marked(view, cx), "\"|");
        })
    });
}
