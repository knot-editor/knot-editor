//! The bundled Vim package driven by real keystrokes through a window.
//!
//! The behavior cases live beside the package in `test/cases/<topic>.jsonc`,
//! in the [case](knot::test_support::cases) format, and each topic runs as its own test
//! through the key router of a window without a native window, native
//! commands, and ordered key delivery. A case starts from `before`, types
//! `keys` in the key notation, and expects `after`: the text with the caret or
//! the selection at its markers, or an object with that `text` and the `mode`
//! the editor is in afterwards, "normal" unless given, or "insert", "visual",
//! or "visual-line", and the `top` line in view. A `view` lays the editor out to
//! show exactly `lines` lines and scrolls `top` to its first line before the
//! keys. `language` sets the document's language. Keys are pressed without
//! waiting between them, as fast typing arrives, and the run settles once at
//! the end. One window shows every case in turn, each in a fresh workbench
//! over a fresh document, after Vim starts over with an empty clipboard and
//! no registers, marks, or last find. Pointer input and the work a key costs run
//! in a window with a native window.
use std::path::{Path, PathBuf};

use gpui::{TestAppContext, point, px};
use serde::Deserialize;
use serde_json::{Value, json};

use knot::test_support::cases::{Failures, Text, package_files, read_groups, topics, unmark};
use knot::test_support::{TestEditor, TestProduct, wait_until};

const ENABLE: &str = r#"import { settings } from "knot";
await settings.set("vim.enabled", true);
"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    #[serde(default)]
    language: Option<String>,
    before: Text,
    keys: String,
    after: Expected,
    #[serde(default)]
    view: Option<View>,
    /// The folds collapsed before the keys, as first and last lines.
    #[serde(default)]
    folds: Vec<[usize; 2]>,
}

/// What a case expects: the text, or the text with more.
#[derive(Deserialize)]
#[serde(untagged)]
enum Expected {
    Full(Full),
    Text(Text),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Full {
    text: Text,
    #[serde(default = "normal")]
    mode: String,
    #[serde(default)]
    top: Option<usize>,
    /// The folds collapsed afterwards, as first and last lines.
    #[serde(default)]
    folds: Option<Vec<[usize; 2]>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct View {
    lines: usize,
    #[serde(default)]
    top: usize,
}

fn normal() -> String {
    "normal".into()
}

/// The mode from the key context, and in normal mode's context from the
/// status bar, which names the visual modes.
fn mode(test_editor: &TestEditor, cx: &mut TestAppContext) -> String {
    let mode = match test_editor.contexts(cx).as_slice() {
        [context] => context.trim_start_matches("vim.").to_owned(),
        other => return format!("{other:?}"),
    };
    let (left, _) = test_editor.status(cx);
    if left.iter().any(|text| text == "-- VISUAL --") {
        "visual".into()
    } else if left.iter().any(|text| text == "-- VISUAL LINE --") {
        "visual-line".into()
    } else {
        mode
    }
}

/// Lay the editor out to show exactly `lines` whole lines, then scroll line
/// `top` to the top.
fn show_lines(test_editor: &TestEditor, lines: usize, top: usize, cx: &mut TestAppContext) {
    test_editor.lay_out(lines, 100, cx);
    test_editor.scroll_rows(-1_000_000_000, cx);
    test_editor.scroll_rows(top as i64, cx);
    assert_eq!(
        top_line(test_editor, cx),
        top,
        "the view starts at line {top}"
    );
}

/// Collapse the folds `folds`, as first and last lines.
fn fold_lines(test_editor: &TestEditor, folds: &[[usize; 2]], cx: &mut TestAppContext) {
    for [first, last] in folds {
        assert!(test_editor.fold(*first, *last, cx), "fold {first}..={last}");
    }
    test_editor.settle(cx);
}

/// The collapsed folds as first and last lines.
fn fold_lines_now(test_editor: &TestEditor, cx: &mut TestAppContext) -> Vec<[usize; 2]> {
    test_editor
        .folds(cx)
        .into_iter()
        .map(|lines| [lines.start, lines.end - 1])
        .collect()
}

/// The first line wholly in view.
fn top_line(test_editor: &TestEditor, cx: &mut TestAppContext) -> usize {
    let (first, _) = test_editor.viewport(cx);
    test_editor.line_of_row(first, cx)
}

/// The line the caret is on.
fn caret_line(test_editor: &TestEditor, cx: &mut TestAppContext) -> usize {
    let caret = test_editor.caret(cx);
    test_editor.line_of(caret, cx)
}

/// Whether the status bar's left side shows `text`.
fn shows(test_editor: &TestEditor, text: &str, cx: &mut TestAppContext) -> bool {
    test_editor.status(cx).0.iter().any(|item| item == text)
}

/// Whether the status bar's right side shows `text`.
fn shows_right(test_editor: &TestEditor, text: &str, cx: &mut TestAppContext) -> bool {
    test_editor.status(cx).1.iter().any(|item| item == text)
}

/// Start the bundled packages with Vim enabled.
fn start(cx: &mut TestAppContext) -> TestProduct {
    TestProduct::start_with_post_init(packages(), Some(ENABLE), cx)
}

/// Open `text` with the caret at `caret` in a window Vim follows.
fn open(product: &TestProduct, text: &str, caret: usize, cx: &mut TestAppContext) -> TestEditor {
    let test_editor = product.editor(text, None, (caret, caret), cx);
    assert_eq!(mode(&test_editor, cx), "normal", "Vim follows the editor");
    test_editor
}

/// Start over from a fresh start's package state, so no case sees what an
/// earlier one left: an empty clipboard, and no registers, register record,
/// marks, or last find.
fn start_over(test_editor: &TestEditor, cx: &mut TestAppContext) {
    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new())));
    test_editor.run_command("vim.reset", cx);
}

/// Run the cases of the topic file `topic`.
fn run_topic(topic: &str, cx: &mut TestAppContext) {
    let product = start(cx);
    let mut failures = Failures::default();
    let mut count = 0;
    // One window shows every case in turn.
    let mut test_editor: Option<TestEditor> = None;
    for path in package_files(&package("vim"))
        .into_iter()
        .filter(|path| path.file_stem().is_some_and(|stem| stem == topic))
    {
        for group in read_groups(&path, &["cases"]) {
            for case in group.cases::<Case>("cases") {
                count += 1;
                let value = &case.value;
                let (before, selection) = unmark(&value.before.lines());
                let Some(selection) = selection else {
                    panic!("{}: {} marks no caret", case.at, case.name);
                };
                let language = value.language.as_deref();
                match &mut test_editor {
                    Some(test_editor) => test_editor.load(&before, language, selection, cx),
                    None => test_editor = Some(product.editor(&before, language, selection, cx)),
                }
                let test_editor = test_editor.as_ref().unwrap();
                start_over(test_editor, cx);
                if let Some(view) = &value.view {
                    show_lines(test_editor, view.lines, view.top, cx);
                }
                fold_lines(test_editor, &value.folds, cx);
                test_editor.press(&value.keys, cx);
                let (text, expected_mode, expected_top, expected_folds) = match &value.after {
                    Expected::Text(text) => (text, "normal", None, None),
                    Expected::Full(full) => (
                        &full.text,
                        full.mode.as_str(),
                        full.top,
                        full.folds.as_ref(),
                    ),
                };
                let expected = text.lines().join("\n");
                let actual = test_editor.marked(cx);
                let mode = mode(test_editor, cx);
                let top = top_line(test_editor, cx);
                let folds = fold_lines_now(test_editor, cx);
                if actual != expected
                    || mode != expected_mode
                    || expected_top.is_some_and(|line| line != top)
                    || expected_folds.is_some_and(|expected| *expected != folds)
                {
                    let folds = match expected_folds {
                        Some(expected) => {
                            format!("\n  folds: expected {expected:?}, actual {folds:?}")
                        }
                        None => String::new(),
                    };
                    let top = match expected_top {
                        Some(line) => format!("\n  top line: expected {line}, actual {top}"),
                        None => String::new(),
                    };
                    failures.fail(
                        &case,
                        "",
                        format!(
                            "keys {:?}\n  expected ({expected_mode}):\n{expected}\n  actual ({mode}):\n{actual}{top}{folds}",
                            value.keys
                        ),
                    );
                }
            }
        }
    }
    if std::env::var("KNOT_CASES").is_err() {
        assert!(count > 0, "no Vim cases in {topic}");
    }
    failures.assert_none();
}

topics!(
    package("vim"),
    run_topic;
    ex, folding, insert, marks, motions, objects, operators, registers, scrolling, visual
);

#[gpui::test]
fn the_mode_shows_in_the_status_bar_and_the_caret(cx: &mut TestAppContext) {
    let product = start(cx);
    let test_editor = open(&product, "one two", 0, cx);
    assert_eq!(test_editor.caret_shape(cx), "block");

    test_editor.press("2g", cx);
    assert!(
        shows_right(&test_editor, "2g", cx),
        "pending keys show while typed: {:?}",
        test_editor.status(cx).1
    );

    test_editor.press("<esc>i", cx);
    assert!(shows(&test_editor, "-- INSERT --", cx));
    assert!(!shows_right(&test_editor, "2g", cx));
    assert_eq!(test_editor.caret_shape(cx), "bar");

    test_editor.press("<esc>", cx);
    assert!(!shows(&test_editor, "-- INSERT --", cx));
    assert_eq!(test_editor.caret_shape(cx), "block");
}

#[gpui::test]
fn j_moves_by_buffer_line_and_gj_by_row_under_soft_wrap(cx: &mut TestAppContext) {
    let product = start(cx);
    let long = "word ".repeat(400);
    let test_editor = open(&product, &format!("{long}\nnext"), 0, cx);
    test_editor.set_setting("editor.softWrap", true.into(), cx);
    test_editor.lay_out(20, 100, cx);

    test_editor.press("gj", cx);
    let row = test_editor.caret(cx);
    assert!(
        row > 0 && row < long.len(),
        "gj stays on the wrapped line: {row}"
    );
    test_editor.press("ggj", cx);
    assert_eq!(test_editor.caret(cx), long.len() + 1);
}

#[gpui::test]
fn screen_motions_and_scrolls_count_rows_under_soft_wrap(cx: &mut TestAppContext) {
    let product = start(cx);
    let long = "word ".repeat(60);
    let lines = [
        "alpha", &long, &long, "beta", &long, "gamma", &long, "delta", "end",
    ];
    let test_editor = open(&product, &lines.join("\n"), 0, cx);
    test_editor.set_setting("editor.softWrap", true.into(), cx);
    test_editor.lay_out(8, 100, cx);
    assert_eq!(test_editor.viewport(cx), (0, 8));
    let bottom = test_editor.line_of_row(7, cx);
    assert!(
        bottom < lines.len() - 1,
        "the viewport ends inside the text"
    );
    assert!(
        bottom - test_editor.line_of_row(0, cx) < 7,
        "wrapped lines take several rows"
    );

    test_editor.press("L", cx);
    assert_eq!(
        caret_line(&test_editor, cx),
        bottom,
        "L goes to the last row's line"
    );
    test_editor.press("2H", cx);
    assert_eq!(
        caret_line(&test_editor, cx),
        test_editor.line_of_row(1, cx),
        "2H goes to the second row's line"
    );
    test_editor.press("M", cx);
    assert_eq!(
        caret_line(&test_editor, cx),
        test_editor.line_of_row(3, cx),
        "M goes to the middle row's line"
    );

    // A half-page scroll moves the view by half its rows, so the caret
    // moves the same number of rows.
    test_editor.press("gg<c-d>", cx);
    assert_eq!(
        test_editor.viewport(cx).0,
        4,
        "ctrl-d scrolls half of the 8 rows"
    );
    assert_eq!(caret_line(&test_editor, cx), test_editor.line_of_row(4, cx));
    test_editor.press("<c-u>", cx);
    assert_eq!(test_editor.viewport(cx).0, 0);

    // zz centers the caret's row even though it is on screen.
    test_editor.press("5G", cx);
    let row = test_editor.row_of(4, 0, cx).unwrap();
    assert!(row >= 6, "line 5 starts below the first screenful: {row}");
    test_editor.press("zz", cx);
    // The caret's row is centered, leaving half a row cut off at each end.
    assert_eq!(test_editor.viewport(cx), (row - 3, 7));
}

#[gpui::test]
fn scrolls_move_through_the_rows_of_one_wrapped_line(cx: &mut TestAppContext) {
    let product = start(cx);
    let long = "word ".repeat(400);
    let test_editor = open(&product, &format!("{long}\nend"), 0, cx);
    test_editor.set_setting("editor.softWrap", true.into(), cx);
    test_editor.lay_out(8, 100, cx);
    // The row the caret shows on, and the view's first row.
    let rows = |cx: &mut TestAppContext| {
        let caret = test_editor.caret(cx);
        (
            test_editor.row_of(0, caret, cx).unwrap(),
            test_editor.viewport(cx).0,
        )
    };
    assert_eq!(rows(cx), (0, 0));

    // Half a view of 8 rows, though the whole view is one line.
    test_editor.press("<c-d>", cx);
    assert_eq!(rows(cx), (4, 4));
    test_editor.press("<c-d>", cx);
    assert_eq!(rows(cx), (8, 8));
    test_editor.press("<c-u>", cx);
    assert_eq!(rows(cx), (4, 4));

    // A page keeps two rows; the caret moves only to stay on screen.
    test_editor.press("<c-f>", cx);
    assert_eq!(rows(cx), (10, 10));
    test_editor.press("<c-b>", cx);
    assert_eq!(rows(cx), (10, 4));
    test_editor.press("<c-u>", cx);
    assert_eq!(rows(cx), (6, 0));

    // A line scroll moves the caret only when it would leave the view.
    test_editor.press("<c-e>", cx);
    assert_eq!(rows(cx), (6, 1));
    test_editor.press("6<c-e>", cx);
    assert_eq!(rows(cx), (7, 7));
    test_editor.press("<c-y>", cx);
    assert_eq!(rows(cx), (7, 6));
}

#[gpui::test]
fn vim_follows_the_setting_and_a_language_scoped_value(cx: &mut TestAppContext) {
    let product = TestProduct::start_with_post_init(
        packages(),
        Some(
            r#"import { settings } from "knot";
await settings.set("vim.enabled", true);
await settings.set("vim.enabled", false, { language: "markdown" });
"#,
        ),
        cx,
    );
    let test_editor = open(&product, "# notes", 0, cx);
    test_editor.press("i", cx);
    assert_eq!(mode(&test_editor, cx), "insert");

    // A language Vim is off for leaves native editing, in insert mode or not.
    test_editor.set_language("markdown", cx);
    assert!(test_editor.contexts(cx).is_empty());
    assert_eq!(test_editor.caret_shape(cx), "bar");
    test_editor.press("x", cx);
    assert_eq!(test_editor.marked(cx), "x‸# notes");

    test_editor.set_language("plaintext", cx);
    assert_eq!(test_editor.contexts(cx), ["vim.normal"]);
}

/// Restart the Vim package and wait until it follows the editor again.
fn restart_vim(product: &TestProduct, test_editor: &TestEditor, cx: &mut TestAppContext) {
    product.restart_package("@knot/vim", cx);
    test_editor.settle(cx);
    assert_eq!(test_editor.contexts(cx), ["vim.normal"]);
}

#[gpui::test]
fn registers_persist_across_a_package_restart(cx: &mut TestAppContext) {
    let product = start(cx);
    let test_editor = open(&product, "one\ntwo", 0, cx);
    test_editor.press("\"ayy\"byw", cx);

    restart_vim(&product, &test_editor, cx);

    // Linewise text stays linewise, and characterwise text characterwise.
    test_editor.press("j\"ap\"bP", cx);
    assert_eq!(test_editor.marked(cx), "one\ntwo\non‸eone");
}

#[gpui::test]
fn the_clipboard_pastes_linewise_only_while_it_holds_the_yanked_line(cx: &mut TestAppContext) {
    let product = start(cx);
    let test_editor = open(&product, "one", 0, cx);
    test_editor.press("yy", cx);
    assert_eq!(
        cx.read(|cx| cx.read_from_clipboard().unwrap().text().unwrap()),
        "one\n"
    );
    test_editor.press("p", cx);
    assert_eq!(test_editor.marked(cx), "one\n‸one");

    // Text copied elsewhere pastes characterwise, even with a line break.
    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("x\n".into())));
    test_editor.press("p", cx);
    assert_eq!(test_editor.marked(cx), "one\no‸x\nne");
}

#[gpui::test]
fn a_mouse_drag_enters_visual_mode_and_a_click_leaves_it(cx: &mut TestAppContext) {
    let product = start(cx);
    let test_editor = product.windowed_editor("one\ntwo\nthree", cx);
    assert_eq!(mode(&test_editor, cx), "normal", "Vim follows the editor");
    let (bounds, text_bounds) = test_editor.bounds(cx);
    let start = point(text_bounds.origin.x + px(1.), text_bounds.origin.y + px(5.));
    let outside = point(
        bounds.origin.x + bounds.size.width + px(20.),
        bounds.origin.y + bounds.size.height + px(20.),
    );

    test_editor.drag(start, outside, cx);
    assert_eq!(mode(&test_editor, cx), "visual");
    assert!(shows(&test_editor, "-- VISUAL --", cx));
    assert_eq!(test_editor.marked(cx), "‸one\ntwo\nthree‸");

    // A click leaves visual mode with the caret where it landed.
    test_editor.drag(start, start, cx);
    assert_eq!(mode(&test_editor, cx), "normal");
    assert!(!shows(&test_editor, "-- VISUAL --", cx));
    assert_eq!(test_editor.marked(cx), "‸one\ntwo\nthree");

    // The dragged selection is what an operator takes.
    test_editor.drag(start, outside, cx);
    assert_eq!(mode(&test_editor, cx), "visual");
    test_editor.press("d", cx);
    assert_eq!(test_editor.marked(cx), "‸");
    assert_eq!(mode(&test_editor, cx), "normal");
}

#[gpui::test]
fn a_global_mark_jumps_to_another_open_document(cx: &mut TestAppContext) {
    let product = start(cx);
    let test_editor = open(&product, "one\ntwo", 0, cx);
    test_editor.press("jmA", cx);
    test_editor.new_tab(cx);
    assert!(!test_editor.is_active(cx));
    assert_eq!(test_editor.active_contexts(cx), ["vim.normal"]);

    test_editor.press("'A", cx);
    assert!(
        test_editor.is_active(cx),
        "the jump shows the marked document"
    );
    assert_eq!(test_editor.marked(cx), "one\n‸two");
    // The marked document's editor has the keys now.
    test_editor.press("k", cx);
    assert_eq!(test_editor.marked(cx), "‸one\ntwo");
}

/// A key costs a few draws and the host requests its behavior needs: the
/// package's reads and writes, the selection notifications, and the status
/// bar package's position. Unchanged status items send nothing.
#[gpui::test]
fn a_key_costs_few_draws_and_requests(cx: &mut TestAppContext) {
    let product = start(cx);
    let test_editor = product.windowed_editor("one two three\nfour five six\n", cx);
    assert_eq!(mode(&test_editor, cx), "normal", "Vim follows the editor");
    // The first key also pays for following the editor.
    test_editor.work_per_key(&["l"], cx);
    let keys = ["w", "d", "w", "i", "a", "<esc>"];
    // At most (draws, requests) per key.
    let bounds = [(3, 16), (3, 3), (7, 26), (3, 10), (3, 7), (6, 20)];
    let counts = test_editor.work_per_key(&keys, cx);
    let report = keys
        .iter()
        .zip(&counts)
        .map(|(key, counts)| format!("{key}: {} draws, {:?}", counts.draws, counts.requests))
        .collect::<Vec<_>>()
        .join("\n");
    for (counts, (draws, requests)) in counts.iter().zip(bounds) {
        assert!(
            counts.draws <= draws && counts.requests.len() <= requests,
            "\n{report}"
        );
    }
    assert_eq!(test_editor.marked(cx), "one ‸athree\nfour five six\n");
}

/// What one case leaves in the package, the next case starts without:
/// registers, the clipboard, the last find, marks, and the persisted
/// register record a restart would read back.
#[gpui::test]
fn a_case_starts_without_what_an_earlier_one_left(cx: &mut TestAppContext) {
    let product = start(cx);
    let mut test_editor = open(&product, "one\ntwo", 0, cx);
    test_editor.press("\"ayyyyfe", cx);
    test_editor.load("xey", None, (2, 2), cx);
    test_editor.press("mA0", cx);
    let left = "\"ap;`A";
    test_editor.press(left, cx);
    assert_eq!(
        test_editor.marked(cx),
        "xe‸y\none",
        "what the earlier case left"
    );

    test_editor.load("xey", None, (2, 2), cx);
    test_editor.press("mA0", cx);
    start_over(&test_editor, cx);
    test_editor.press(left, cx);
    test_editor.press("p", cx);
    assert_eq!(test_editor.marked(cx), "‸xey");

    restart_vim(&product, &test_editor, cx);
    test_editor.press("\"ap", cx);
    assert_eq!(test_editor.marked(cx), "‸xey", "the register record");
}

/// Serve `a.txt` holding "one" and `b.txt` holding "two" from
/// `mem://product/dir/`.
fn serve_files(product: &TestProduct, cx: &mut TestAppContext) -> knot::test_support::ServedFiles {
    product.serve_files(
        "mem://product/dir",
        &[("a.txt", "one"), ("b.txt", "two")],
        cx,
    )
}

#[gpui::test]
fn ex_writes_and_opens_files_beside_the_document(cx: &mut TestAppContext) {
    let product = start(cx);
    let files = serve_files(&product, cx);
    let test_editor = product.open_file(&files, "mem://product/dir/a.txt", cx);
    assert_eq!(mode(&test_editor, cx), "normal", "Vim follows the editor");
    test_editor.press("x:w<cr>", cx);
    assert_eq!(files.read("mem://product/dir/a.txt"), "ne\n");
    assert!(!test_editor.is_dirty(cx));

    test_editor.press(":e b.txt<cr>", cx);
    assert_eq!(test_editor.active_text(cx), "two");
    // Vim follows the opened editor.
    test_editor.press("x", cx);
    assert_eq!(test_editor.active_text(cx), "wo");
}

#[gpui::test]
fn ex_edit_completes_paths_beside_the_document(cx: &mut TestAppContext) {
    let product = start(cx);
    let files = serve_files(&product, cx);
    let test_editor = product.open_file(&files, "mem://product/dir/a.txt", cx);
    test_editor.press(":e b<tab><cr>", cx);
    assert_eq!(test_editor.active_text(cx), "two");
}

#[gpui::test]
fn ex_edit_from_an_untitled_document_opens_in_the_first_root(cx: &mut TestAppContext) {
    let product = start(cx);
    serve_files(&product, cx);
    let test_editor = open(&product, "", 0, cx);
    test_editor.set_roots(&["mem://product/dir/"], cx);
    test_editor.press(":e b.txt<cr>", cx);
    assert_eq!(test_editor.active_text(cx), "two");
}

#[gpui::test]
fn ex_quit_asks_before_losing_changes_and_quit_bang_does_not(cx: &mut TestAppContext) {
    let product = start(cx);
    let test_editor = product.windowed_editor("one", cx);
    assert_eq!(mode(&test_editor, cx), "normal", "Vim follows the editor");
    test_editor.press("x", cx);
    test_editor.press_unsettled(":q<cr>", cx);
    wait_until(cx, |cx| cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    test_editor.settle(cx);
    assert!(test_editor.is_open(cx));
    assert_eq!(test_editor.marked(cx), "‸ne");

    test_editor.press(":q!<cr>", cx);
    assert!(!cx.has_pending_prompt());
    assert!(!test_editor.is_open(cx));
}

#[gpui::test]
fn ex_set_writes_document_scoped_settings(cx: &mut TestAppContext) {
    let product = start(cx);
    let test_editor = open(&product, "one", 0, cx);
    let here = |key: &str, cx: &mut TestAppContext| test_editor.setting(key, cx);
    let elsewhere = |key: &str, cx: &mut TestAppContext| product.setting(key, Some("editor"), cx);
    test_editor.press(":set ts=2 noet wrap rnu<cr>", cx);
    assert_eq!(here("editor.tabSize", cx), Some(json!(2)));
    assert_eq!(here("editor.insertSpaces", cx), Some(Value::Bool(false)));
    assert_eq!(here("editor.softWrap", cx), Some(Value::Bool(true)));
    assert_eq!(here("editor.lineNumbers", cx), Some(json!("relative")));
    assert_eq!(elsewhere("editor.tabSize", cx), Some(json!(4)));
    assert_eq!(elsewhere("editor.lineNumbers", cx), Some(json!("on")));
    test_editor.press(":set nornu nonu<cr>", cx);
    assert_eq!(here("editor.lineNumbers", cx), Some(json!("off")));

    // An error shows in place of the mode until the next key.
    test_editor.press(":set hls<cr>", cx);
    assert!(shows(&test_editor, "E518: Unknown option: hls", cx));
    test_editor.press("l", cx);
    assert!(!shows(&test_editor, "E518: Unknown option: hls", cx));
}

/// `ctrl-w` runs the workbench's pane commands, with or without ctrl on the
/// second key, and Vim follows the editor each leaves focused. Panes are
/// named by their index in layout order.
#[gpui::test]
fn ctrl_w_splits_moves_between_and_closes_panes(cx: &mut TestAppContext) {
    let product = start(cx);
    let test_editor = open(&product, "one\ntwo", 0, cx);
    let (left, right) = (0, 1);

    test_editor.press("<c-w>v", cx);
    assert_eq!(test_editor.pane_count(cx), 2);
    assert_eq!(
        test_editor.focused_pane(cx),
        right,
        "a split focuses the new pane"
    );
    assert!(!shows_right(&test_editor, "^W", cx));

    test_editor.press("<c-w>h", cx);
    assert_eq!(test_editor.focused_pane(cx), left);
    test_editor.press("<c-w><c-l>", cx);
    assert_eq!(test_editor.focused_pane(cx), right);
    // Vim follows the pane's editor, which takes the keys.
    test_editor.press("<c-w>h<c-w><bs>", cx);
    assert_eq!(test_editor.focused_pane(cx), left);

    // w and W cycle the panes, p goes back, t and b go to the ends.
    test_editor.press("<c-w>w", cx);
    assert_eq!(test_editor.focused_pane(cx), right);
    test_editor.press("<c-w>w", cx);
    assert_eq!(test_editor.focused_pane(cx), left, "w wraps");
    test_editor.press("<c-w>W", cx);
    assert_eq!(test_editor.focused_pane(cx), right, "W wraps back");
    test_editor.press("<c-w>p", cx);
    assert_eq!(test_editor.focused_pane(cx), left);
    test_editor.press("<c-w>p", cx);
    assert_eq!(test_editor.focused_pane(cx), right, "p alternates");
    test_editor.press("<c-w>t", cx);
    assert_eq!(test_editor.focused_pane(cx), left);
    test_editor.press("<c-w>b", cx);
    assert_eq!(test_editor.focused_pane(cx), right);
    test_editor.press("<c-w>h", cx);

    test_editor.press("<c-w>s", cx);
    assert_eq!(test_editor.pane_count(cx), 3);
    test_editor.press("<c-w>k", cx);
    // Closing closes the tab, and its pane only with the last tab.
    test_editor.new_tab(cx);
    test_editor.press("<c-w>c", cx);
    assert_eq!(
        test_editor.pane_count(cx),
        3,
        "the pane keeps its other tab"
    );
    test_editor.press("<c-w>c", cx);
    assert_eq!(test_editor.pane_count(cx), 2);
    test_editor.press("<c-w>v", cx);
    assert_eq!(test_editor.pane_count(cx), 3);
    for key in ["=", "+", "-", "<lt>", ">"] {
        test_editor.press(&format!("<c-w>{key}"), cx);
        assert_eq!(test_editor.pane_count(cx), 3, "{key}");
    }
    test_editor.press("<c-w>q", cx);
    assert_eq!(
        test_editor.pane_count(cx),
        2,
        "q closes the pane resizing left"
    );
    test_editor.press("<c-w>o", cx);
    assert_eq!(test_editor.pane_count(cx), 1, "only closes the other panes");
    assert_eq!(test_editor.focused_pane(cx), 0);
}

/// `ctrl-w` moves between the center's editors and never into a dock's
/// tool, where Vim's keys do not apply; Knot's own directional focus still
/// reaches the dock.
#[gpui::test]
fn ctrl_w_directions_stay_in_the_center(cx: &mut TestAppContext) {
    let product = start(cx);
    let test_editor = open(&product, "one", 0, cx);
    let (editor, explorer) = (0, 1);
    test_editor.toggle_dock_tool(cx);
    assert_eq!(test_editor.pane_count(cx), 2);
    assert_eq!(test_editor.focused_pane(cx), explorer);
    test_editor.run_command("workbench.focus-pane-right", cx);
    assert_eq!(test_editor.focused_pane(cx), editor);
    test_editor.press("<c-w>h", cx);
    assert_eq!(test_editor.focused_pane(cx), editor);
    test_editor.run_command("workbench.focus-pane-left", cx);
    assert_eq!(test_editor.focused_pane(cx), explorer);
}

/// A `ctrl-w` left pending shows `^W`, and the next key either completes it
/// or cancels it without acting as its own command.
#[gpui::test]
fn a_pending_ctrl_w_does_not_swallow_the_next_keys_meaning(cx: &mut TestAppContext) {
    let product = start(cx);
    let test_editor = open(&product, "one\ntwo", 0, cx);
    test_editor.press("<c-w>", cx);
    assert!(shows_right(&test_editor, "^W", cx));
    test_editor.press("<esc>j", cx);
    assert_eq!(test_editor.marked(cx), "one\n‸two");
    assert!(!shows_right(&test_editor, "^W", cx));
    // An unknown key cancels, and the key after it is a plain command.
    test_editor.press("<c-w>xk", cx);
    assert_eq!(test_editor.marked(cx), "‸one\ntwo");
    assert_eq!(test_editor.pane_count(cx), 1);
}

/// `ctrl-w` with a capital moves the tab toward that side, splitting at the
/// edge, and a pane left without tabs goes away.
#[gpui::test]
fn ctrl_w_capital_moves_the_tab_between_panes(cx: &mut TestAppContext) {
    let product = start(cx);
    let test_editor = open(&product, "one\ntwo", 0, cx);
    test_editor.new_tab(cx);
    test_editor.press("<c-w>L", cx);
    assert_eq!(test_editor.pane_count(cx), 2, "the edge splits");
    test_editor.press("<c-w>H", cx);
    assert_eq!(test_editor.pane_count(cx), 1, "the emptied pane collapses");
}

/// A count before or after `ctrl-w` repeats resizing by a step.
#[gpui::test]
fn ctrl_w_counts_repeat_resizing(cx: &mut TestAppContext) {
    let product = start(cx);
    let test_editor = open(&product, "one\ntwo", 0, cx);
    test_editor.press("<c-w>v", cx);
    let split = test_editor.focused_pane(cx);
    test_editor.press("<c-w>h", cx);
    let ratio = |cx: &mut TestAppContext| test_editor.split_ratio(split, cx).unwrap();
    test_editor.press("3<c-w>>", cx);
    assert!((ratio(cx) - 0.65).abs() < 1e-4, "{}", ratio(cx));
    test_editor.press("<c-w>2<lt>", cx);
    assert!((ratio(cx) - 0.55).abs() < 1e-4, "{}", ratio(cx));
    test_editor.press("<c-w>1", cx);
    assert!(shows_right(&test_editor, "^W1", cx));
    test_editor.press("<esc>", cx);
}

/// Vim and the language packages its cases edit in.
fn packages() -> &'static [knot_bundled::BundledSource] {
    static PACKAGES: std::sync::OnceLock<Vec<knot_bundled::BundledSource>> =
        std::sync::OnceLock::new();
    PACKAGES.get_or_init(|| [knot_ext_vim::PACKAGES, knot_ext_languages::PACKAGES].concat())
}

/// The directory of the bundled package `@knot/<name>`.
fn package(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../extensions/@knot")
        .join(name)
}
