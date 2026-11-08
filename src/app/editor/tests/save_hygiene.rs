//! Trimming trailing whitespace and the final newline.

use gpui::{AppContext, TestAppContext};

use super::{BufferModel, Editor};
use crate::app::settings::ApplicationSettings;

const TRIM: &str = "editor.trim-trailing-whitespace";

/// Run `action` on an editor over `model` with the caret at `caret`.
fn run(
    cx: &mut TestAppContext,
    model: BufferModel,
    caret: (usize, usize),
    action: impl FnOnce(&mut Editor, &mut gpui::Context<Editor>),
) -> (String, (usize, usize), String) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let model = cx.new(|_| model);
    let editor = cx.new(|cx| Editor::new(model, crate::app::EDITOR_KEY_CONTEXT, cx));
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            (view.cursor_line, view.cursor_col) = caret;
            (view.anchor_line, view.anchor_col) = caret;
            view.has_selection = false;
            view.sync_view_position(cx);
            action(view, cx);
            let after = view.text(cx);
            let position = (view.cursor_line, view.cursor_col);
            view.execute_editing_command("editor.undo", cx);
            (after, position, view.text(cx))
        })
    })
}

fn clean(cx: &mut TestAppContext, text: &str, caret: (usize, usize)) -> (String, (usize, usize)) {
    let (after, position, undone) = run(cx, BufferModel::from_text(text), caret, |view, cx| {
        view.clean_up_for_save(cx);
    });
    if after != text {
        assert_eq!(undone, text, "one undo restores {text:?}");
    }
    (after, position)
}

#[gpui::test]
fn save_trims_spaces_and_tabs_but_not_the_cursor_line(cx: &mut TestAppContext) {
    assert_eq!(
        clean(cx, "a  \nb\t \n  c  \nd\n", (2, 5)),
        ("a\nb\n  c  \nd\n".into(), (2, 5))
    );
}

#[gpui::test]
fn save_keeps_crlf_terminators(cx: &mut TestAppContext) {
    assert_eq!(
        clean(cx, "a  \r\nb \r\nc", (2, 0)),
        ("a\r\nb\r\nc\r\n".into(), (2, 0))
    );
}

#[gpui::test]
fn save_appends_the_buffers_terminator_to_a_text_without_one(cx: &mut TestAppContext) {
    assert_eq!(clean(cx, "a\nb", (0, 0)), ("a\nb\n".into(), (0, 0)));
    assert_eq!(clean(cx, "a\r\nb", (0, 0)), ("a\r\nb\r\n".into(), (0, 0)));
    // A caret at the end stays before the new terminator.
    assert_eq!(clean(cx, "a\nb", (1, 1)), ("a\nb\n".into(), (1, 1)));
}

#[gpui::test]
fn save_leaves_final_newlines_and_empty_buffers_alone(cx: &mut TestAppContext) {
    for text in ["", "a\n", "a\r\n", "a\n\n\n", "\n"] {
        assert_eq!(clean(cx, text, (0, 0)).0, text);
    }
}

#[gpui::test]
fn save_trims_the_last_line_and_ends_it_in_one_edit(cx: &mut TestAppContext) {
    assert_eq!(clean(cx, "a\nb  ", (0, 0)), ("a\nb\n".into(), (0, 0)));
    assert_eq!(clean(cx, "a\n  ", (0, 0)), ("a\n".into(), (0, 0)));
}

#[gpui::test]
fn save_cleanup_follows_the_settings(cx: &mut TestAppContext) {
    use crate::app::settings::{
        FILES_INSERT_FINAL_NEWLINE, FILES_TRIM_TRAILING_WHITESPACE, SettingsScope,
    };
    cx.update(|cx| {
        let mut settings = ApplicationSettings::default();
        let global = SettingsScope::default();
        settings
            .0
            .set(FILES_TRIM_TRAILING_WHITESPACE, "off".into(), global.clone())
            .unwrap();
        settings
            .0
            .set(FILES_INSERT_FINAL_NEWLINE, false.into(), global)
            .unwrap();
        cx.set_global(settings);
    });
    let model = cx.new(|_| BufferModel::from_text("a  \nb"));
    let editor = cx.new(|cx| Editor::new(model, crate::app::EDITOR_KEY_CONTEXT, cx));
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            view.clean_up_for_save(cx);
            assert_eq!(view.text(cx), "a  \nb");
        });
    });
}

#[gpui::test]
fn trim_command_trims_every_line_including_the_cursors_as_one_undo_step(cx: &mut TestAppContext) {
    let (after, position, undone) = run(
        cx,
        BufferModel::from_text("a  \nb \nc"),
        (1, 2),
        |view, cx| {
            assert!(view.execute_editing_command(TRIM, cx));
        },
    );
    assert_eq!(after, "a\nb\nc");
    assert_eq!(position, (1, 1));
    assert_eq!(undone, "a  \nb \nc");
}

#[gpui::test]
fn cleanup_does_nothing_to_a_read_only_model(cx: &mut TestAppContext) {
    let (after, _, _) = run(
        cx,
        BufferModel::from_read_only_text("a  \nb"),
        (0, 0),
        |view, cx| {
            view.clean_up_for_save(cx);
            view.execute_editing_command(TRIM, cx);
        },
    );
    assert_eq!(after, "a  \nb");
}
