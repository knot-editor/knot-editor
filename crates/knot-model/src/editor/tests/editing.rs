//! Typing, deletion, movement, undo across editors, composition, completion,
//! selection reveals, view state, and contribution projection.

use knot_protocol::{
    CompletionResultItem, Decoration, ExtensionId, ExtensionKey, ExtensionLifecycleId,
};

use super::*;
use crate::completion::CompletionProviderRegistry;

/// The text the last copy or cut put on the clipboard.
fn clipboard(f: &mut Fixture) -> Option<String> {
    f.editor
        .take_effects()
        .into_iter()
        .rev()
        .find_map(|effect| match effect {
            EditorEffect::WriteClipboard(text) => Some(text),
            _ => None,
        })
}

#[test]
fn ordinary_editing_keeps_projection_clipboard_and_composition_synchronous() {
    let mut f = Fixture::new("");
    f.type_text("a");
    f.type_text("👩‍💻");
    f.type_text("é");
    assert_eq!(f.text(), "a👩‍💻é");
    f.run("editor.delete-backward");
    assert_eq!(f.text(), "a👩‍💻");
    f.run("editor.select-all");
    f.run("editor.cut");
    assert_eq!(f.text(), "");
    let cut = clipboard(&mut f);
    assert_eq!(cut.as_deref(), Some("a👩‍💻"));
    f.editor.paste(cut.as_deref(), &mut f.buffer);
    assert_eq!(f.text(), "a👩‍💻");
    f.editor.mark_text(None, "に", Some(0..1), &mut f.buffer);
    assert_eq!(f.editor.marked_range_utf16, Some(6..7));
    f.editor.mark_text(None, "日本", Some(2..2), &mut f.buffer);
    assert_eq!(f.text(), "a👩‍💻日本");
    f.type_text("日本語");
    assert_eq!(f.text(), "a👩‍💻日本語");
    assert_eq!(f.editor.marked_range_utf16, None);
    f.run("editor.undo");
    assert_eq!(f.text(), "a👩‍💻");
    f.run("editor.redo");
    assert_eq!(f.text(), "a👩‍💻日本語");
    f.run("editor.select-all");
    f.type_text("one\r\ntwo\rthree");
    assert_eq!(f.text(), "one\ntwo\nthree");
}

#[test]
fn copy_needs_a_selection_and_writes_the_selected_text() {
    let mut f = Fixture::new("abc");
    assert!(!f.run("copy"));
    assert_eq!(clipboard(&mut f), None);
    f.place((0, 1), (0, 3));
    assert!(f.run("copy"));
    assert_eq!(clipboard(&mut f).as_deref(), Some("bc"));
    assert_eq!(f.text(), "abc");
}

#[test]
fn typing_deletion_and_interaction_boundaries_form_practical_undo_steps() {
    let mut f = Fixture::new("");
    f.type_text("a");
    f.type_text("β");
    f.type_text("c");
    f.run("editor.undo");
    assert_eq!(f.text(), "");
    f.run("editor.redo");
    assert_eq!(f.text(), "aβc");

    f.run("editor.move-document-end");
    f.run("editor.delete-backward");
    f.run("editor.delete-backward");
    assert_eq!(f.text(), "a");
    f.run("editor.undo");
    assert_eq!(f.text(), "aβc");

    f.run("editor.move-document-end");
    f.run("editor.move-left");
    f.type_text("X");
    assert_eq!(f.text(), "aβXc");
    f.run("editor.undo");
    assert_eq!(f.text(), "aβc");

    f.editor.paste(Some(" pasted"), &mut f.buffer);
    assert_eq!(f.text(), "aβ pastedc");
    f.run("editor.undo");
    assert_eq!(f.text(), "aβc");
}

#[test]
fn undo_and_redo_restore_the_originating_views_grouped_selection() {
    let mut f = Fixture::new("abcdef");
    let mut origin = EditorState::new("editor", EditorSettings::default(), &mut f.buffer);
    let other = &mut f.editor;

    (origin.anchor_col, origin.cursor_col) = (4, 1);
    origin.has_selection = true;
    origin.sync_view_position(&mut f.buffer);
    origin.type_text(None, "X", &mut f.buffer);
    origin.type_text(None, "Y", &mut f.buffer);
    other.buffer_changed(&mut f.buffer);
    other.cursor_col = 5;
    other.sync_view_position(&mut f.buffer);
    other.execute_editing_command("editor.undo", &mut f.buffer);
    origin.buffer_changed(&mut f.buffer);

    assert_eq!(f.buffer.text(), "abcdef");
    assert_eq!((origin.anchor_col, origin.cursor_col), (4, 1));
    assert!(origin.has_selection);
    assert!(origin.selection_reversed);
    assert!(!other.has_selection);
    assert_ne!(other.caret(), (0, 1));

    other.execute_editing_command("editor.redo", &mut f.buffer);
    origin.buffer_changed(&mut f.buffer);
    assert_eq!(f.buffer.text(), "aXYef");
    assert_eq!(origin.caret(), (0, 3));
    assert!(!origin.has_selection);
}

#[test]
fn replay_ignores_view_state_for_an_originating_view_that_was_dropped() {
    let mut f = Fixture::new("text");
    let mut origin = EditorState::new("editor", EditorSettings::default(), &mut f.buffer);
    origin.execute_editing_command("editor.move-document-end", &mut f.buffer);
    origin.type_text(None, "!", &mut f.buffer);
    origin.release(&mut f.buffer);
    drop(origin);
    f.editor.buffer_changed(&mut f.buffer);

    f.run("editor.undo");
    assert_eq!(f.text(), "text");
    assert_eq!(f.editor.caret(), (0, 0));
}

#[test]
fn a_newer_edit_invalidates_an_unobserved_history_view_restore() {
    let mut f = Fixture::new("abcdef");
    let mut origin = EditorState::new("editor", EditorSettings::default(), &mut f.buffer);
    (origin.anchor_col, origin.cursor_col) = (4, 1);
    origin.has_selection = true;
    origin.sync_view_position(&mut f.buffer);
    origin.type_text(None, "X", &mut f.buffer);
    f.editor.buffer_changed(&mut f.buffer);
    f.run("editor.undo");
    // The origin sees the undo only after a later edit.
    assert!(f.buffer.replace(0..0, "z").unwrap());
    origin.buffer_changed(&mut f.buffer);

    assert_eq!(f.text(), "zabcdef");
    assert_eq!(origin.caret(), (0, 5));
    assert!(!origin.has_selection);
}

#[test]
fn movement_uses_visual_columns_graphemes_and_pane_height() {
    let mut f = Fixture::new("éééx\na\nabcde\nword next\nlast");
    f.editor.cursor_col = "ééé".len();
    f.run("editor.move-down");
    assert_eq!(f.editor.caret(), (1, 1));
    f.run("editor.move-down");
    assert_eq!(f.editor.caret(), (2, 3));
    f.run("editor.move-document-start");
    f.editor.viewport_rows = 2.;
    f.run("editor.select-page-down");
    assert_eq!(f.editor.selected_byte_range(&f.buffer), Some(0..10));
    f.run("editor.move-document-end");
    assert_eq!(f.editor.caret(), (4, 4));
    f.run("editor.move-word-left");
    assert_eq!(f.editor.caret(), (4, 0));
    f.run("editor.select-word-left");
    assert_eq!(f.editor.caret(), (3, 5));
}

#[test]
fn vertical_movement_keeps_the_display_cell_across_tabs_and_wide_characters() {
    // Cells: a tab reaches the next stop of 4, CJK and emoji take two.
    let mut f = Fixture::new("abcdefgh\n\tx\n中文字\n😀😀z\nab\nabcdefgh");
    let down = |f: &mut Fixture| {
        f.run("editor.move-down");
        f.editor.caret()
    };
    f.editor.cursor_col = 5;
    // Cell 5 is after the tab and `x`.
    assert_eq!(down(&mut f), (1, 2));
    // The wanted cell stays 5, between 文 (ending at 4) and 字 (ending at
    // 6): the earlier boundary wins.
    assert_eq!(down(&mut f), (2, "中文".len()));
    // After two emoji and `z`, the line ends at cell 5.
    assert_eq!(down(&mut f), (3, "😀😀z".len()));
    // A short line ends at its end, and the cell survives it.
    assert_eq!(down(&mut f), (4, 2));
    assert_eq!(down(&mut f), (5, 5));
}

#[test]
fn crlf_is_one_navigation_and_deletion_boundary() {
    let mut f = Fixture::new("a\r\nb");
    f.run("editor.move-line-end");
    assert_eq!(f.editor.caret(), (0, 1));
    f.run("editor.move-right");
    assert_eq!(f.editor.caret(), (1, 0));
    f.run("editor.delete-backward");
    assert_eq!(f.text(), "ab");
}

#[test]
fn delete_to_line_end_removes_the_rest_of_the_line_then_its_break() {
    let mut f = Fixture::new("hello world\r\nnext");
    f.run("editor.move-word-right");
    f.run("editor.delete-to-line-end");
    assert_eq!(f.text(), "hello\r\nnext");
    f.run("editor.delete-to-line-end");
    assert_eq!(f.text(), "hellonext");
    f.run("editor.undo");
    assert_eq!(f.text(), "hello\r\nnext");

    f.run("editor.select-left");
    f.run("editor.delete-to-line-end");
    assert_eq!(f.text(), "hell\r\nnext");

    f.run("editor.move-document-end");
    assert!(f.run("editor.delete-to-line-end"));
    assert_eq!(f.text(), "hell\r\nnext");
}

#[test]
fn inserted_newlines_follow_a_crlf_buffer() {
    let mut f = Fixture::new("a\r\nb");
    f.run("editor.move-line-end");
    f.run("editor.insert-newline");
    assert_eq!(f.text(), "a\r\n\r\nb");
    assert_eq!(f.editor.caret(), (1, 0));

    f.editor.paste(Some("1\n2\r\n3"), &mut f.buffer);
    assert_eq!(f.text(), "a\r\n1\r\n2\r\n3\r\nb");
    assert_eq!(f.editor.caret(), (3, 1));

    f.type_text("\n");
    assert_eq!(f.text(), "a\r\n1\r\n2\r\n3\r\n\r\nb");

    f.run("editor.delete-backward");
    f.run("editor.delete-forward");
    assert_eq!(f.text(), "a\r\n1\r\n2\r\n3b");
}

#[test]
fn composition_is_cleared_by_navigation_and_external_edits() {
    let mut f = Fixture::new("abc");
    f.editor.mark_text(None, "に", Some(1..1), &mut f.buffer);
    f.run("editor.move-right");
    assert_eq!(f.editor.marked_range_utf16, None);
    f.editor.mark_text(None, "本", Some(1..1), &mut f.buffer);
    f.run("editor.select-all");
    assert_eq!(f.editor.marked_range_utf16, None);
    f.editor.mark_text(None, "日", Some(1..1), &mut f.buffer);
    f.editor
        .set_selection(0, "日".len(), Reveal::Nearest, &mut f.buffer);
    assert_eq!(f.editor.marked_range_utf16, None);
    f.editor.mark_text(None, "語", Some(1..1), &mut f.buffer);
    assert!(f.editor.marked_range_utf16.is_some());

    f.buffer.replace(0..0, "prefix").unwrap();
    f.editor.buffer_changed(&mut f.buffer);
    assert_eq!(f.editor.marked_range_utf16, None);
}

#[test]
fn constructs_from_arbitrary_buffer_text() {
    let f = Fixture::new("generated\ntext");
    assert_eq!(f.editor.line_count, 2);
}

#[test]
fn completion_acceptance_rechecks_the_captured_revision() {
    let mut registry = CompletionProviderRegistry::new();
    registry.register(
        ExtensionId::new(1),
        ExtensionLifecycleId::new(1),
        "fixture".into(),
    );
    let open = |f: &mut Fixture, handle: u64| {
        f.editor.cursor_col = 2;
        let request = f
            .editor
            .start_completion(BufferHandle::new(handle), registry.snapshot(), &f.buffer)
            .remove(0)
            .1;
        assert!(f.editor.apply_completion_response(
            request.buffer,
            CompletionResponse {
                registration: request.registration,
                revision: request.revision,
                generation: request.generation,
                result: Ok(vec![CompletionResultItem {
                    label: "foo".into(),
                    insert_text: "foo".into(),
                }]),
            },
            &f.buffer,
        ));
    };

    let mut f = Fixture::new("fo");
    open(&mut f, 1);
    assert!(f.buffer.replace(0..0, "x").unwrap());
    f.editor.accept_selected_completion(&mut f.buffer);
    assert_eq!(f.text(), "xfo");

    let mut f = Fixture::new("fo");
    open(&mut f, 2);
    f.editor.accept_selected_completion(&mut f.buffer);
    assert_eq!(f.text(), "foo");
}

fn hundred_lines() -> Fixture {
    let text = (0..100)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    Fixture::new(&text)
}

#[test]
fn set_selection_reveals_nearest_or_centers_an_offscreen_caret() {
    let mut f = hundred_lines();
    let line_start = |f: &Fixture, line: usize| f.buffer.line_range(line).start;
    let (line_10, line_15, line_50, line_90) = (
        line_start(&f, 10),
        line_start(&f, 15),
        line_start(&f, 50),
        line_start(&f, 90),
    );
    let (editor, buffer) = (&mut f.editor, &mut f.buffer);
    editor.viewport_rows = 10.;

    // Nearest scrolls just enough to show the caret's line at the bottom.
    editor.set_selection(line_50, line_50 + 2, Reveal::Nearest, buffer);
    assert_eq!(editor.scroll_top, 41.);
    assert_eq!(editor.selection(buffer), (line_50, line_50 + 2));
    assert_eq!(
        editor.selected_byte_range(buffer),
        Some(line_50..line_50 + 2)
    );

    // Center keeps the scroll while the caret's line is visible...
    editor.set_selection(line_10, line_10, Reveal::Nearest, buffer);
    assert_eq!(editor.scroll_top, 10.);
    editor.set_selection(line_15, line_15, Reveal::Center, buffer);
    assert_eq!(editor.scroll_top, 10.);

    // ...and centers it when the line is outside the viewport.
    editor.set_selection(line_90, line_90, Reveal::Center, buffer);
    assert_eq!(editor.scroll_top, 85.5);
    assert_eq!(editor.selected_byte_range(buffer), None);
}

#[test]
fn top_and_bottom_reveals_scroll_and_scrolling_leaves_the_caret() {
    let mut f = hundred_lines();
    let line_start = |f: &Fixture, line: usize| f.buffer.line_range(line).start;
    let (line_5, line_50, line_100) = (line_start(&f, 5), line_start(&f, 50), line_start(&f, 100));
    let (editor, buffer) = (&mut f.editor, &mut f.buffer);
    // Before layout there is a single visible line.
    assert_eq!(editor.viewport(), (0, 1));
    editor.viewport_rows = 10.;
    assert_eq!(editor.viewport(), (0, 10));

    editor.set_selection(line_50, line_50, Reveal::Top, buffer);
    assert_eq!(editor.scroll_top, 50.);
    assert_eq!(editor.viewport(), (50, 10));
    editor.set_selection(line_50, line_50, Reveal::Bottom, buffer);
    assert_eq!(editor.scroll_top, 41.);
    assert_eq!(editor.viewport(), (41, 10));

    // Both stop at the ends of the text.
    editor.set_selection(line_5, line_5, Reveal::Bottom, buffer);
    assert_eq!(editor.scroll_top, 0.);
    editor.set_selection(line_100, line_100, Reveal::Top, buffer);
    assert_eq!(editor.scroll_top, 91.);
    assert_eq!(editor.viewport(), (91, 10));

    // Center keeps a caret that is on screen where it is; always-center
    // moves the view to it.
    editor.scroll_top = 91.;
    editor.set_selection(line_100, line_100, Reveal::Center, buffer);
    assert_eq!(editor.scroll_top, 91.);
    editor.set_selection(line_100, line_100, Reveal::AlwaysCenter, buffer);
    assert_eq!(editor.scroll_top, 91., "clamped to the end of the text");
    editor.set_selection(line_50, line_50, Reveal::AlwaysCenter, buffer);
    assert_eq!(editor.scroll_top, 45.5);
    editor.set_selection(line_100, line_100, Reveal::Top, buffer);

    // Scrolling moves neither the caret nor the reveal-on-paint flag.
    editor.scroll_by_rows(-30);
    assert_eq!(editor.scroll_top, 61.);
    assert_eq!(editor.selection(buffer), (line_100, line_100));
    assert!(!editor.reveal_caret);
    editor.scroll_by_rows(-1000);
    assert_eq!(editor.scroll_top, 0.);
    editor.scroll_by_rows(1000);
    assert_eq!(editor.scroll_top, 91.);
}

#[test]
fn view_state_round_trips_and_clamps_to_shorter_multibyte_text() {
    let mut f = Fixture::new("one\ntwo\nthree\nfour");
    let state = EditorViewState {
        anchor: 4,
        caret: 9,
        top: 8,
        scroll_x: 14.,
        folds: Vec::new(),
    };
    f.editor.restore_view_state(state.clone(), &mut f.buffer);
    assert_eq!(f.editor.view_state(&f.buffer), state);
    assert_eq!(f.editor.selected_byte_range(&f.buffer), Some(4..9));
    assert_eq!(f.editor.scroll_top, 2.);

    // Offsets past the end clamp, and offsets inside a scalar snap back.
    let mut f = Fixture::new("é😀\nb");
    f.editor.restore_view_state(
        EditorViewState {
            anchor: 3,
            caret: 400,
            top: 400,
            scroll_x: -5.,
            folds: Vec::new(),
        },
        &mut f.buffer,
    );
    assert_eq!(
        f.editor.view_state(&f.buffer),
        EditorViewState {
            anchor: 2,
            caret: "é😀\nb".len(),
            top: "é😀\n".len(),
            scroll_x: 0.,
            folds: Vec::new(),
        }
    );
}

#[test]
fn caret_shape_is_the_latest_lifecycles_and_reverts_when_cleared() {
    let first = ExtensionKey::new(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    let second = ExtensionKey::new(ExtensionId::new(2), ExtensionLifecycleId::new(1));
    let mut f = Fixture::new("abc");
    let (editor, buffer) = (&mut f.editor, &mut f.buffer);
    assert_eq!(editor.caret_shape(), CaretShape::Bar);
    editor.set_caret_shape(first, Some(CaretShape::Block));
    editor.set_caret_shape(second, Some(CaretShape::Underline));
    assert_eq!(editor.caret_shape(), CaretShape::Underline);
    editor.set_caret_shape(second, None);
    assert_eq!(editor.caret_shape(), CaretShape::Block);
    editor.set_caret_shape(second, Some(CaretShape::Bar));
    editor.remove_owner_state(second, buffer);
    assert_eq!(editor.caret_shape(), CaretShape::Block);
    editor.remove_owner_state(first, buffer);
    assert_eq!(editor.caret_shape(), CaretShape::Bar);
}

#[test]
fn flat_utf16_offsets_round_trip_across_emoji_lines() {
    let family = "👨‍👩‍👧‍👦";
    let first_line = "a😀b";
    let second_line = format!("{family}z");
    let model = BufferModel::from_text(format!("{first_line}\n{second_line}"));
    let second_line_start = 5;
    let family_utf16_len = family.chars().map(char::len_utf16).sum::<usize>();

    assert_eq!(flat_utf16_to_position(&model, 1), (0, 1));
    assert_eq!(flat_utf16_to_position(&model, 3), (0, "a😀".len()));
    assert_eq!(flat_utf16_to_position(&model, second_line_start), (1, 0));
    assert_eq!(
        flat_utf16_to_position(&model, second_line_start + family_utf16_len),
        (1, family.len())
    );

    for (line, byte_col) in [
        (0, 0),
        (0, 1),
        (0, "a😀".len()),
        (0, first_line.len()),
        (1, 0),
        (1, family.len()),
        (1, second_line.len()),
    ] {
        let utf16 = to_flat_utf16(&model, line, byte_col);
        assert_eq!(flat_utf16_to_position(&model, utf16), (line, byte_col));
    }
}

#[test]
fn words_split_at_vs_code_separators_and_keep_unicode_segments() {
    fn segments(text: &str) -> Vec<&str> {
        word_segments(text)
            .into_iter()
            .map(|(start, end, _)| &text[start..end])
            .collect()
    }
    assert_eq!(segments("some.thing"), ["some", ".", "thing"]);
    assert_eq!(segments("foo::bar()"), ["foo", "::", "bar", "()"]);
    assert_eq!(
        segments("don't 3.14"),
        ["don", "'", "t", " ", "3", ".", "14"]
    );
    assert_eq!(
        segments("snake_case  x-y"),
        ["snake_case", "  ", "x", "-", "y"]
    );
    assert_eq!(segments("日本語 👩‍💻é"), ["日", "本", "語", " ", "👩‍💻", "é"]);
    assert!(segments("").is_empty());
}

#[test]
fn word_at_prefers_the_word_ending_at_the_offset() {
    let model = BufferModel::from_text("foo.bar  é日\n\nlast");
    let at = |offset| word_at(&model, offset).map(|(range, class)| (model.read(range), class));
    let word = |text: &str| Some((text.to_string(), WordClass::Word));
    assert_eq!(at(0), word("foo"));
    assert_eq!(at(2), word("foo"));
    assert_eq!(at(3), word("foo"));
    assert_eq!(at(4), word("bar"));
    assert_eq!(at(7), word("bar"));
    assert_eq!(at(8), Some(("  ".to_string(), WordClass::Whitespace)));
    assert_eq!(at(9), word("é"));
    assert_eq!(at(11), word("é"));
    assert_eq!(at(14), word("日"));
    // The end of a line with a separator last, an empty line, and the last line's end.
    assert_eq!(at(model.len()), word("last"));
    assert_eq!(at(15), None);
    let separators = BufferModel::from_text("a ::");
    assert_eq!(
        word_at(&separators, 4).map(|(range, class)| (range, class)),
        Some((2..4, WordClass::Separator))
    );
    assert_eq!(word_at(&BufferModel::from_text(""), 0), None);
}

#[test]
fn word_movement_stops_at_separators() {
    let mut f = Fixture::new("a.some_thing(x)  end");
    let mut run = |command| {
        f.run(command);
        f.editor.cursor_col
    };
    run("editor.move-document-start");
    let stops = (0..7)
        .map(|_| run("editor.move-word-right"))
        .collect::<Vec<_>>();
    assert_eq!(stops, [1, 2, 12, 13, 14, 15, 20]);
    let stops = (0..7)
        .map(|_| run("editor.move-word-left"))
        .collect::<Vec<_>>();
    assert_eq!(stops, [17, 14, 13, 12, 2, 1, 0]);
}

#[test]
fn soft_wrap_and_indentation_commands_leave_their_settings_to_the_owner() {
    let document = uuid::Uuid::new_v4();
    let mut buffer = BufferModel::from_text("a\n\tb");
    buffer.set_document(document);
    let mut f = Fixture::over(buffer, EditorSettings::default());
    assert!(f.run("editor.toggle-soft-wrap"));
    assert!(f.run("editor.convert-indentation-to-tabs"));
    assert!(f.run("editor.detect-indentation"));
    assert_eq!(
        f.editor.take_effects(),
        [
            EditorEffect::SetDocumentSoftWrap {
                document,
                wrap: true
            },
            EditorEffect::SetDocumentInsertSpaces {
                document,
                spaces: false
            },
            EditorEffect::DetectIndentation { document },
        ]
    );
    // Without a document, there is nothing to write.
    let mut f = Fixture::new("a");
    assert!(!f.run("editor.toggle-soft-wrap"));
    assert!(f.editor.take_effects().is_empty());
}

#[test]
fn contribution_projection_indexes_multiline_ranges() {
    let model = BufferModel::from_text("aa\nbbbb\ncc");
    let (decorations, gutters, actions) = project_contributions(
        &model,
        vec![ResolvedEditorContribution {
            range: ByteRange {
                start_byte_offset: 1,
                end_byte_offset: 9,
            },
            source: ContributionSource::fixture(),
            decoration: Some(Decoration::new("warning", DecorationStyle::Wavy)),
            gutter: Some("warning".to_owned()),
            command: Some("fixture.action".into()),
        }],
    );

    assert_eq!(
        decorations
            .iter()
            .map(|decoration| (decoration.line, decoration.start, decoration.end))
            .collect::<Vec<_>>(),
        [(0, 1, 2), (1, 0, 4), (2, 0, 1)]
    );
    assert_eq!(gutters.len(), 1);
    assert_eq!(gutters[0].line, 0);
    assert_eq!(gutters[0].command.as_deref(), Some("fixture.action"));
    assert!(actions.iter().all(|action| {
        action.range
            == ByteRange {
                start_byte_offset: 1,
                end_byte_offset: 9,
            }
    }));
    assert_eq!(
        actions
            .iter()
            .map(|action| (action.line, action.start, action.end))
            .collect::<Vec<_>>(),
        [(0, 1, 2), (1, 0, 4), (2, 0, 1)]
    );
}

#[test]
fn each_line_keeps_its_most_severe_marker() {
    let model = BufferModel::from_text("aa\nbb");
    let marker = |start, key: &str, command: &str| ResolvedEditorContribution {
        range: ByteRange {
            start_byte_offset: start,
            end_byte_offset: start + 1,
        },
        source: ContributionSource::fixture(),
        decoration: None,
        gutter: Some(key.to_owned()),
        command: Some(command.into()),
    };
    let (_, gutters, _) = project_contributions(
        &model,
        vec![
            marker(0, "error", "first.error"),
            marker(1, "info", "info"),
            marker(0, "error", "second.error"),
            marker(3, "diff.add", "diff"),
            marker(4, "warning", "warning"),
            marker(3, "made.up", "later"),
        ],
    );
    assert_eq!(
        gutters
            .iter()
            .map(|marker| (marker.line, marker.key.as_str(), marker.command.as_deref()))
            .collect::<Vec<_>>(),
        [
            (0, "error", Some("second.error")),
            (1, "warning", Some("warning"))
        ]
    );
}
