//! The matched-bracket highlight, Jump to Matching Bracket, and typing
//! bracket pairs.

use super::indentation::rust_editor;
use super::*;
use crate::editing::{BracketPair, EditingDefinition, SyntaxFamily};

const JUMP: &str = "editor.jump-to-matching-bracket";

//                  0123456789012345
const SOURCE: &str = "fn f(a) { g(); }";

#[test]
fn the_bracket_at_the_caret_and_its_partner_are_highlighted() {
    let mut f = rust_editor("", SOURCE);
    let mut at = |col| {
        f.editor.cursor_col = col;
        f.editor.matched_brackets(&f.buffer)
    };
    assert_eq!(at(5), Some([4..5, 6..7]));
    assert_eq!(at(4), Some([4..5, 6..7]));
    assert_eq!(at(7), Some([6..7, 4..5]));
    // The bracket before the caret wins.
    assert_eq!(at(12), Some([11..12, 12..13]));
    assert_eq!(at(10), None);
    assert_eq!(at(16), Some([15..16, 8..9]));
}

#[test]
fn the_highlight_follows_edits() {
    let mut f = rust_editor("", SOURCE);
    f.editor.cursor_col = 5;
    assert_eq!(f.editor.matched_brackets(&f.buffer), Some([4..5, 6..7]));
    f.editor.cursor_col = 0;
    f.editor
        .replace_text_with_grouping(None, "x", None, &mut f.buffer);
    f.editor.cursor_col = 6;
    assert_eq!(f.editor.matched_brackets(&f.buffer), Some([5..6, 7..8]));
}

#[test]
fn jump_moves_before_the_partner_and_back() {
    let mut f = rust_editor("", SOURCE);
    let jump = |f: &mut Fixture| {
        f.run(JUMP);
        (
            f.editor.cursor_line,
            f.editor.cursor_col,
            f.editor.has_selection,
        )
    };
    f.editor.cursor_col = 5;
    assert_eq!(jump(&mut f), (0, 6, false));
    assert_eq!(jump(&mut f), (0, 4, false));
    assert_eq!(jump(&mut f), (0, 6, false));
    // Between an adjacent pair, before the opener.
    f.editor.cursor_col = 12;
    assert_eq!(jump(&mut f), (0, 11, false));
    // Away from brackets, before the enclosing closer.
    f.editor.cursor_col = 10;
    assert_eq!(jump(&mut f), (0, 15, false));
    // With a selection, from the caret, clearing the selection.
    f.editor.anchor_col = 0;
    f.editor.cursor_col = 5;
    f.editor.has_selection = true;
    assert_eq!(jump(&mut f), (0, 6, false));
}

#[test]
fn a_document_without_a_language_matches_nothing() {
    let mut f = Fixture::new(SOURCE);
    f.editor.cursor_col = 5;
    assert_eq!(f.editor.matched_brackets(&f.buffer), None);
    f.run(JUMP);
    assert_eq!((f.editor.cursor_line, f.editor.cursor_col), (0, 5));
}

/// An editor over `text` in Rust with string and comment highlights,
/// bracket pairs, a quote that does not close inside strings or comments,
/// and a single quote that only surrounds, with the caret at `caret`.
fn pairs_editor(text: &str, caret: usize) -> Fixture {
    pairs_editor_with(text, caret, EditorSettings::default())
}

fn pairs_editor_with(text: &str, caret: usize, settings: EditorSettings) -> Fixture {
    let pair = |open: &str, close: &str| BracketPair {
        open: open.into(),
        close: close.into(),
        auto_close: None,
        newline: None,
        not_in: None,
    };
    let rust = Language {
        highlights: "(string_literal) @string\n(line_comment) @comment\n(block_comment) @comment.documentation",
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
        ..Default::default()
    }
    .compile("rust");
    let mut buffer = BufferModel::from_text(text);
    set_syntax(&mut buffer, rust, Arc::new(|_| None));
    let mut f = Fixture::over(buffer, settings);
    move_to(&mut f, caret);
    f
}

fn move_to(f: &mut Fixture, offset: usize) {
    (f.editor.cursor_line, f.editor.cursor_col) = (0, offset);
    f.editor.has_selection = false;
    f.editor.finish_position_change(&mut f.buffer);
}

fn select(f: &mut Fixture, anchor: usize, caret: usize) {
    (f.editor.anchor_line, f.editor.anchor_col) = (0, anchor);
    (f.editor.cursor_line, f.editor.cursor_col) = (0, caret);
    f.editor.has_selection = anchor != caret;
    f.editor.finish_position_change(&mut f.buffer);
}

/// Type `text` one character at a time.
fn type_chars(f: &mut Fixture, text: &str) {
    for character in text.chars() {
        f.type_text(&character.to_string());
    }
}

/// The text with `|` at the caret, or `[` and `]` around the selection.
fn marked(f: &Fixture) -> String {
    let mut text = f.text();
    let caret = f.editor.cursor_col;
    if f.editor.has_selection {
        let anchor = f.editor.anchor_col;
        let (start, end) = (anchor.min(caret), anchor.max(caret));
        text.insert(end, ']');
        text.insert(start, '[');
    } else {
        text.insert(caret, '|');
    }
    text
}

#[test]
fn an_opener_inserts_its_closer_as_one_undo_step() {
    let mut f = pairs_editor("", 0);
    type_chars(&mut f, "f(");
    assert_eq!(marked(&f), "f(|)");
    type_chars(&mut f, "{");
    assert_eq!(marked(&f), "f({|})");
    f.run("editor.undo");
    assert_eq!(marked(&f), "f(|)");
    f.run("editor.undo");
    assert_eq!(f.text(), "");
}

#[test]
fn an_opener_before_a_word_does_not_close() {
    let mut f = pairs_editor("x", 0);
    type_chars(&mut f, "(");
    assert_eq!(marked(&f), "(|x");
}

#[test]
fn typing_a_tracked_closer_steps_over_it() {
    let mut f = pairs_editor(";", 0);
    type_chars(&mut f, "f((a");
    assert_eq!(marked(&f), "f((a|));");
    type_chars(&mut f, ")");
    assert_eq!(marked(&f), "f((a)|);");
    type_chars(&mut f, ")");
    assert_eq!(marked(&f), "f((a))|;");
    type_chars(&mut f, ")");
    assert_eq!(marked(&f), "f((a)))|;");
}

#[test]
fn an_untracked_closer_is_typed() {
    let mut f = pairs_editor("f()", 2);
    type_chars(&mut f, ")");
    assert_eq!(marked(&f), "f()|)");
}

#[test]
fn leaving_the_pair_stops_tracking_it() {
    let mut f = pairs_editor("", 0);
    type_chars(&mut f, "(");
    move_to(&mut f, 0);
    move_to(&mut f, 1);
    type_chars(&mut f, ")");
    assert_eq!(marked(&f), "()|)");
    // A selection stops tracking too.
    move_to(&mut f, 3);
    type_chars(&mut f, " (");
    select(&mut f, 0, 4);
    move_to(&mut f, 5);
    type_chars(&mut f, ")");
    assert_eq!(marked(&f), "()) ()|)");
}

#[test]
fn backspace_between_a_tracked_pair_deletes_both() {
    let mut f = pairs_editor("", 0);
    type_chars(&mut f, "f(");
    f.run("editor.delete-backward");
    assert_eq!(marked(&f), "f|");
    // Once text was typed inside, the pair is no longer empty.
    type_chars(&mut f, "(a");
    f.run("editor.delete-backward");
    f.run("editor.delete-backward");
    assert_eq!(marked(&f), "f|");
}

#[test]
fn backspace_between_an_untracked_pair_deletes_one_character() {
    let mut f = pairs_editor("()", 1);
    f.run("editor.delete-backward");
    assert_eq!(marked(&f), "|)");
}

#[test]
fn an_opener_wraps_the_selection_and_keeps_it_selected() {
    let mut f = pairs_editor("a bc d", 0);
    select(&mut f, 2, 4);
    type_chars(&mut f, "(");
    assert_eq!(marked(&f), "a ([bc]) d");
    assert!(f.editor.cursor_col > f.editor.anchor_col);
    // A pair that does not auto-close still wraps, and a reversed
    // selection stays reversed.
    select(&mut f, 4, 3);
    type_chars(&mut f, "'");
    assert_eq!(marked(&f), "a ('[b]'c) d");
    assert!(f.editor.cursor_col < f.editor.anchor_col);
    f.run("editor.undo");
    assert_eq!(marked(&f), "a ([b]c) d");
}

#[test]
fn quotes_do_not_close_inside_strings_or_comments() {
    let text = "fn f() { let s = \"a b\"; } // c";
    let inside_string = text.find("a b").unwrap() + 1;
    let mut f = pairs_editor(text, inside_string);
    type_chars(&mut f, "\"");
    assert_eq!(f.text(), "fn f() { let s = \"a\" b\"; } // c");
    let end = f.text().len();
    move_to(&mut f, end);
    type_chars(&mut f, "\"");
    assert!(f.text().ends_with("// c\""));
    move_to(&mut f, 8);
    type_chars(&mut f, "\"");
    assert!(f.text().starts_with("fn f() {\"\" "));
}

#[test]
fn the_settings_turn_closing_and_wrapping_off() {
    let settings = EditorSettings {
        auto_closing_brackets: false,
        auto_surround: false,
        ..EditorSettings::default()
    };
    let mut f = pairs_editor_with("ab", 0, settings);
    type_chars(&mut f, "(");
    assert_eq!(marked(&f), "(|ab");
    select(&mut f, 1, 3);
    type_chars(&mut f, "(");
    assert_eq!(marked(&f), "((|");
}

#[test]
fn composed_text_does_not_close() {
    let mut f = pairs_editor("", 0);
    f.editor.mark_text(None, "\"", None, &mut f.buffer);
    f.type_text("\"");
    assert_eq!(marked(&f), "\"|");
}
