//! Trimming trailing whitespace and the final newline.

use super::*;

const TRIM: &str = "editor.trim-trailing-whitespace";

/// Run `action` on an editor over `model` with the caret at `caret`; returns
/// the text after it, the caret then, and the text after one undo.
fn run(
    model: BufferModel,
    caret: Position,
    action: impl FnOnce(&mut Fixture),
) -> (String, Position, String) {
    let mut fixture = Fixture::over(model, EditorSettings::default());
    fixture.place(caret, caret);
    action(&mut fixture);
    let after = fixture.text();
    let position = fixture.editor.caret();
    fixture.run("editor.undo");
    (after, position, fixture.text())
}

fn clean(text: &str, caret: Position) -> (String, Position) {
    let (after, position, undone) = run(BufferModel::from_text(text), caret, |fixture| {
        fixture.editor.clean_up_for_save(&mut fixture.buffer);
    });
    if after != text {
        assert_eq!(undone, text, "one undo restores {text:?}");
    }
    (after, position)
}

#[test]
fn save_trims_spaces_and_tabs_but_not_the_cursor_line() {
    assert_eq!(
        clean("a  \nb\t \n  c  \nd\n", (2, 5)),
        ("a\nb\n  c  \nd\n".into(), (2, 5))
    );
}

#[test]
fn save_keeps_crlf_terminators() {
    assert_eq!(
        clean("a  \r\nb \r\nc", (2, 0)),
        ("a\r\nb\r\nc\r\n".into(), (2, 0))
    );
}

#[test]
fn save_appends_the_buffers_terminator_to_a_text_without_one() {
    assert_eq!(clean("a\nb", (0, 0)), ("a\nb\n".into(), (0, 0)));
    assert_eq!(clean("a\r\nb", (0, 0)), ("a\r\nb\r\n".into(), (0, 0)));
    // A caret at the end stays before the new terminator.
    assert_eq!(clean("a\nb", (1, 1)), ("a\nb\n".into(), (1, 1)));
}

#[test]
fn save_leaves_final_newlines_and_empty_buffers_alone() {
    for text in ["", "a\n", "a\r\n", "a\n\n\n", "\n"] {
        assert_eq!(clean(text, (0, 0)).0, text);
    }
}

#[test]
fn save_trims_the_last_line_and_ends_it_in_one_edit() {
    assert_eq!(clean("a\nb  ", (0, 0)), ("a\nb\n".into(), (0, 0)));
    assert_eq!(clean("a\n  ", (0, 0)), ("a\n".into(), (0, 0)));
}

#[test]
fn save_cleanup_follows_the_settings() {
    let settings = EditorSettings {
        trim_trailing_whitespace: false,
        insert_final_newline: false,
        ..EditorSettings::default()
    };
    let mut fixture = Fixture::over(BufferModel::from_text("a  \nb"), settings);
    fixture.editor.clean_up_for_save(&mut fixture.buffer);
    assert_eq!(fixture.text(), "a  \nb");
}

#[test]
fn trim_command_trims_every_line_including_the_cursors_as_one_undo_step() {
    let (after, position, undone) = run(BufferModel::from_text("a  \nb \nc"), (1, 2), |fixture| {
        assert!(fixture.run(TRIM));
    });
    assert_eq!(after, "a\nb\nc");
    assert_eq!(position, (1, 1));
    assert_eq!(undone, "a  \nb \nc");
}

#[test]
fn cleanup_does_nothing_to_a_read_only_model() {
    let (after, _, _) = run(
        BufferModel::from_read_only_text("a  \nb"),
        (0, 0),
        |fixture| {
            fixture.editor.clean_up_for_save(&mut fixture.buffer);
            fixture.run(TRIM);
        },
    );
    assert_eq!(after, "a  \nb");
}
