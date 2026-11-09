//! The bundled Vim package driven by real keystrokes through a product
//! session.
//!
//! The behavior cases live beside the package in `test/cases/<topic>.jsonc`,
//! in the [case](knot::test_support::cases) format, and each topic runs as its own test
//! through a windowless session's key router, native commands, and ordered
//! key delivery. A case starts from `before`, types `keys` in the key
//! notation, and expects `after`: the text with the caret or the selection at
//! its markers, or an object with that `text` and the `mode` the editor is
//! in afterwards, "normal" unless given, or "insert", "visual", or
//! "visual-line", and the `top` line in view. A `view` lays the editor out to
//! show exactly `lines` lines and scrolls `top` to its first line before the
//! keys. `language` sets the document's language. Keys are pressed without
//! waiting between them, as fast typing arrives, and the run settles once at
//! the end. One session shows every case in turn, each in a fresh workbench
//! over a fresh document, after Vim starts over with an empty clipboard and
//! no registers, marks, or last find. Pointer input and the work a key costs a window run
//! in a product window.
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
fn mode(session: &TestEditor, cx: &mut TestAppContext) -> String {
    let mode = match session.contexts(cx).as_slice() {
        [context] => context.trim_start_matches("vim.").to_owned(),
        other => return format!("{other:?}"),
    };
    let (left, _) = session.status(cx);
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
fn show_lines(session: &TestEditor, lines: usize, top: usize, cx: &mut TestAppContext) {
    session.lay_out(lines, 100, cx);
    session.scroll_rows(-1_000_000_000, cx);
    session.scroll_rows(top as i64, cx);
    assert_eq!(top_line(session, cx), top, "the view starts at line {top}");
}

/// Collapse the folds `folds`, as first and last lines.
fn fold_lines(session: &TestEditor, folds: &[[usize; 2]], cx: &mut TestAppContext) {
    for [first, last] in folds {
        assert!(session.fold(*first, *last, cx), "fold {first}..={last}");
    }
    session.settle(cx);
}

/// The collapsed folds as first and last lines.
fn fold_lines_now(session: &TestEditor, cx: &mut TestAppContext) -> Vec<[usize; 2]> {
    session
        .folds(cx)
        .into_iter()
        .map(|lines| [lines.start, lines.end - 1])
        .collect()
}

/// The first line wholly in view.
fn top_line(session: &TestEditor, cx: &mut TestAppContext) -> usize {
    let (first, _) = session.viewport(cx);
    session.line_of_row(first, cx)
}

/// The line the caret is on.
fn caret_line(session: &TestEditor, cx: &mut TestAppContext) -> usize {
    let caret = session.caret(cx);
    session.line_of(caret, cx)
}

/// Whether the status bar's left side shows `text`.
fn shows(session: &TestEditor, text: &str, cx: &mut TestAppContext) -> bool {
    session.status(cx).0.iter().any(|item| item == text)
}

/// Whether the status bar's right side shows `text`.
fn shows_right(session: &TestEditor, text: &str, cx: &mut TestAppContext) -> bool {
    session.status(cx).1.iter().any(|item| item == text)
}

/// Start the bundled packages with Vim enabled.
fn start(cx: &mut TestAppContext) -> TestProduct {
    TestProduct::start_with_post_init(packages(), Some(ENABLE), cx)
}

/// Open `text` with the caret at `caret` in a session Vim follows.
fn open(product: &TestProduct, text: &str, caret: usize, cx: &mut TestAppContext) -> TestEditor {
    let session = product.editor(text, None, (caret, caret), cx);
    assert_eq!(mode(&session, cx), "normal", "Vim follows the editor");
    session
}

/// Start over from a fresh start's package state, so no case sees what an
/// earlier one left: an empty clipboard, and no registers, register record,
/// marks, or last find.
fn start_over(session: &TestEditor, cx: &mut TestAppContext) {
    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new())));
    session.run_command("vim.reset", cx);
}

/// Run the cases of the topic file `topic`.
fn run_topic(topic: &str, cx: &mut TestAppContext) {
    let product = start(cx);
    let mut failures = Failures::default();
    let mut count = 0;
    // One session shows every case in turn.
    let mut session: Option<TestEditor> = None;
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
                match &mut session {
                    Some(session) => session.load(&before, language, selection, cx),
                    None => session = Some(product.editor(&before, language, selection, cx)),
                }
                let session = session.as_ref().unwrap();
                start_over(session, cx);
                if let Some(view) = &value.view {
                    show_lines(session, view.lines, view.top, cx);
                }
                fold_lines(session, &value.folds, cx);
                session.press(&value.keys, cx);
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
                let actual = session.marked(cx);
                let mode = mode(session, cx);
                let top = top_line(session, cx);
                let folds = fold_lines_now(session, cx);
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
    let session = open(&product, "one two", 0, cx);
    assert_eq!(session.caret_shape(cx), "block");

    session.press("2g", cx);
    assert!(
        shows_right(&session, "2g", cx),
        "pending keys show while typed: {:?}",
        session.status(cx).1
    );

    session.press("<esc>i", cx);
    assert!(shows(&session, "-- INSERT --", cx));
    assert!(!shows_right(&session, "2g", cx));
    assert_eq!(session.caret_shape(cx), "bar");

    session.press("<esc>", cx);
    assert!(!shows(&session, "-- INSERT --", cx));
    assert_eq!(session.caret_shape(cx), "block");
}

#[gpui::test]
fn j_moves_by_buffer_line_and_gj_by_row_under_soft_wrap(cx: &mut TestAppContext) {
    let product = start(cx);
    let long = "word ".repeat(400);
    let session = open(&product, &format!("{long}\nnext"), 0, cx);
    session.set_setting("editor.softWrap", true.into(), cx);
    session.lay_out(20, 100, cx);

    session.press("gj", cx);
    let row = session.caret(cx);
    assert!(
        row > 0 && row < long.len(),
        "gj stays on the wrapped line: {row}"
    );
    session.press("ggj", cx);
    assert_eq!(session.caret(cx), long.len() + 1);
}

#[gpui::test]
fn screen_motions_and_scrolls_count_rows_under_soft_wrap(cx: &mut TestAppContext) {
    let product = start(cx);
    let long = "word ".repeat(60);
    let lines = [
        "alpha", &long, &long, "beta", &long, "gamma", &long, "delta", "end",
    ];
    let session = open(&product, &lines.join("\n"), 0, cx);
    session.set_setting("editor.softWrap", true.into(), cx);
    session.lay_out(8, 100, cx);
    assert_eq!(session.viewport(cx), (0, 8));
    let bottom = session.line_of_row(7, cx);
    assert!(
        bottom < lines.len() - 1,
        "the viewport ends inside the text"
    );
    assert!(
        bottom - session.line_of_row(0, cx) < 7,
        "wrapped lines take several rows"
    );

    session.press("L", cx);
    assert_eq!(
        caret_line(&session, cx),
        bottom,
        "L goes to the last row's line"
    );
    session.press("2H", cx);
    assert_eq!(
        caret_line(&session, cx),
        session.line_of_row(1, cx),
        "2H goes to the second row's line"
    );
    session.press("M", cx);
    assert_eq!(
        caret_line(&session, cx),
        session.line_of_row(3, cx),
        "M goes to the middle row's line"
    );

    // A half-page scroll moves the view by half its rows, so the caret
    // moves the same number of rows.
    session.press("gg<c-d>", cx);
    assert_eq!(
        session.viewport(cx).0,
        4,
        "ctrl-d scrolls half of the 8 rows"
    );
    assert_eq!(caret_line(&session, cx), session.line_of_row(4, cx));
    session.press("<c-u>", cx);
    assert_eq!(session.viewport(cx).0, 0);

    // zz centers the caret's row even though it is on screen.
    session.press("5G", cx);
    let row = session.row_of(4, 0, cx).unwrap();
    assert!(row >= 6, "line 5 starts below the first screenful: {row}");
    session.press("zz", cx);
    // The caret's row is centered, leaving half a row cut off at each end.
    assert_eq!(session.viewport(cx), (row - 3, 7));
}

#[gpui::test]
fn scrolls_move_through_the_rows_of_one_wrapped_line(cx: &mut TestAppContext) {
    let product = start(cx);
    let long = "word ".repeat(400);
    let session = open(&product, &format!("{long}\nend"), 0, cx);
    session.set_setting("editor.softWrap", true.into(), cx);
    session.lay_out(8, 100, cx);
    // The row the caret shows on, and the view's first row.
    let rows = |cx: &mut TestAppContext| {
        let caret = session.caret(cx);
        (
            session.row_of(0, caret, cx).unwrap(),
            session.viewport(cx).0,
        )
    };
    assert_eq!(rows(cx), (0, 0));

    // Half a view of 8 rows, though the whole view is one line.
    session.press("<c-d>", cx);
    assert_eq!(rows(cx), (4, 4));
    session.press("<c-d>", cx);
    assert_eq!(rows(cx), (8, 8));
    session.press("<c-u>", cx);
    assert_eq!(rows(cx), (4, 4));

    // A page keeps two rows; the caret moves only to stay on screen.
    session.press("<c-f>", cx);
    assert_eq!(rows(cx), (10, 10));
    session.press("<c-b>", cx);
    assert_eq!(rows(cx), (10, 4));
    session.press("<c-u>", cx);
    assert_eq!(rows(cx), (6, 0));

    // A line scroll moves the caret only when it would leave the view.
    session.press("<c-e>", cx);
    assert_eq!(rows(cx), (6, 1));
    session.press("6<c-e>", cx);
    assert_eq!(rows(cx), (7, 7));
    session.press("<c-y>", cx);
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
    let session = open(&product, "# notes", 0, cx);
    session.press("i", cx);
    assert_eq!(mode(&session, cx), "insert");

    // A language Vim is off for leaves native editing, in insert mode or not.
    session.set_language("markdown", cx);
    assert!(session.contexts(cx).is_empty());
    assert_eq!(session.caret_shape(cx), "bar");
    session.press("x", cx);
    assert_eq!(session.marked(cx), "x‸# notes");

    session.set_language("plaintext", cx);
    assert_eq!(session.contexts(cx), ["vim.normal"]);
}

/// Restart the Vim package and wait until it follows the editor again.
fn restart_vim(product: &TestProduct, session: &TestEditor, cx: &mut TestAppContext) {
    product.restart_package("@knot/vim", cx);
    session.settle(cx);
    assert_eq!(session.contexts(cx), ["vim.normal"]);
}

#[gpui::test]
fn registers_persist_across_a_package_restart(cx: &mut TestAppContext) {
    let product = start(cx);
    let session = open(&product, "one\ntwo", 0, cx);
    session.press("\"ayy\"byw", cx);

    restart_vim(&product, &session, cx);

    // Linewise text stays linewise, and characterwise text characterwise.
    session.press("j\"ap\"bP", cx);
    assert_eq!(session.marked(cx), "one\ntwo\non‸eone");
}

#[gpui::test]
fn the_clipboard_pastes_linewise_only_while_it_holds_the_yanked_line(cx: &mut TestAppContext) {
    let product = start(cx);
    let session = open(&product, "one", 0, cx);
    session.press("yy", cx);
    assert_eq!(
        cx.read(|cx| cx.read_from_clipboard().unwrap().text().unwrap()),
        "one\n"
    );
    session.press("p", cx);
    assert_eq!(session.marked(cx), "one\n‸one");

    // Text copied elsewhere pastes characterwise, even with a line break.
    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("x\n".into())));
    session.press("p", cx);
    assert_eq!(session.marked(cx), "one\no‸x\nne");
}

#[gpui::test]
fn a_mouse_drag_enters_visual_mode_and_a_click_leaves_it(cx: &mut TestAppContext) {
    let product = start(cx);
    let session = product.windowed_editor("one\ntwo\nthree", cx);
    assert_eq!(mode(&session, cx), "normal", "Vim follows the editor");
    let (bounds, text_bounds) = session.bounds(cx);
    let start = point(text_bounds.origin.x + px(1.), text_bounds.origin.y + px(5.));
    let outside = point(
        bounds.origin.x + bounds.size.width + px(20.),
        bounds.origin.y + bounds.size.height + px(20.),
    );

    session.drag(start, outside, cx);
    assert_eq!(mode(&session, cx), "visual");
    assert!(shows(&session, "-- VISUAL --", cx));
    assert_eq!(session.marked(cx), "‸one\ntwo\nthree‸");

    // A click leaves visual mode with the caret where it landed.
    session.drag(start, start, cx);
    assert_eq!(mode(&session, cx), "normal");
    assert!(!shows(&session, "-- VISUAL --", cx));
    assert_eq!(session.marked(cx), "‸one\ntwo\nthree");

    // The dragged selection is what an operator takes.
    session.drag(start, outside, cx);
    assert_eq!(mode(&session, cx), "visual");
    session.press("d", cx);
    assert_eq!(session.marked(cx), "‸");
    assert_eq!(mode(&session, cx), "normal");
}

#[gpui::test]
fn a_global_mark_jumps_to_another_open_document(cx: &mut TestAppContext) {
    let product = start(cx);
    let session = open(&product, "one\ntwo", 0, cx);
    session.press("jmA", cx);
    session.new_tab(cx);
    assert!(!session.is_active(cx));
    assert_eq!(session.active_contexts(cx), ["vim.normal"]);

    session.press("'A", cx);
    assert!(session.is_active(cx), "the jump shows the marked document");
    assert_eq!(session.marked(cx), "one\n‸two");
    // The marked document's editor has the keys now.
    session.press("k", cx);
    assert_eq!(session.marked(cx), "‸one\ntwo");
}

/// A key costs a few draws and the host requests its behavior needs: the
/// package's reads and writes, the selection notifications, and the status
/// bar package's position. Unchanged status items send nothing.
#[gpui::test]
fn a_key_costs_few_draws_and_requests(cx: &mut TestAppContext) {
    let product = start(cx);
    let session = product.windowed_editor("one two three\nfour five six\n", cx);
    assert_eq!(mode(&session, cx), "normal", "Vim follows the editor");
    // The first key also pays for following the editor.
    session.work_per_key(&["l"], cx);
    let keys = ["w", "d", "w", "i", "a", "<esc>"];
    // At most (draws, requests) per key.
    let bounds = [(3, 16), (3, 3), (7, 26), (3, 10), (3, 7), (6, 20)];
    let counts = session.work_per_key(&keys, cx);
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
    assert_eq!(session.marked(cx), "one ‸athree\nfour five six\n");
}

/// What one case leaves in the package, the next case starts without:
/// registers, the clipboard, the last find, marks, and the persisted
/// register record a restart would read back.
#[gpui::test]
fn a_case_starts_without_what_an_earlier_one_left(cx: &mut TestAppContext) {
    let product = start(cx);
    let mut session = open(&product, "one\ntwo", 0, cx);
    session.press("\"ayyyyfe", cx);
    session.load("xey", None, (2, 2), cx);
    session.press("mA0", cx);
    let left = "\"ap;`A";
    session.press(left, cx);
    assert_eq!(
        session.marked(cx),
        "xe‸y\none",
        "what the earlier case left"
    );

    session.load("xey", None, (2, 2), cx);
    session.press("mA0", cx);
    start_over(&session, cx);
    session.press(left, cx);
    session.press("p", cx);
    assert_eq!(session.marked(cx), "‸xey");

    restart_vim(&product, &session, cx);
    session.press("\"ap", cx);
    assert_eq!(session.marked(cx), "‸xey", "the register record");
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
    let session = product.open_file(&files, "mem://product/dir/a.txt", cx);
    assert_eq!(mode(&session, cx), "normal", "Vim follows the editor");
    session.press("x:w<cr>", cx);
    assert_eq!(files.read("mem://product/dir/a.txt"), "ne\n");
    assert!(!session.is_dirty(cx));

    session.press(":e b.txt<cr>", cx);
    assert_eq!(session.active_text(cx), "two");
    // Vim follows the opened editor.
    session.press("x", cx);
    assert_eq!(session.active_text(cx), "wo");
}

#[gpui::test]
fn ex_edit_from_an_untitled_document_opens_in_the_first_root(cx: &mut TestAppContext) {
    let product = start(cx);
    serve_files(&product, cx);
    let session = open(&product, "", 0, cx);
    session.set_roots(&["mem://product/dir/"], cx);
    session.press(":e b.txt<cr>", cx);
    assert_eq!(session.active_text(cx), "two");
}

#[gpui::test]
fn ex_quit_asks_before_losing_changes_and_quit_bang_does_not(cx: &mut TestAppContext) {
    let product = start(cx);
    let session = product.windowed_editor("one", cx);
    assert_eq!(mode(&session, cx), "normal", "Vim follows the editor");
    session.press("x", cx);
    session.press_unsettled(":q<cr>", cx);
    wait_until(cx, |cx| cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    session.settle(cx);
    assert!(session.is_open(cx));
    assert_eq!(session.marked(cx), "‸ne");

    session.press(":q!<cr>", cx);
    assert!(!cx.has_pending_prompt());
    assert!(!session.is_open(cx));
}

#[gpui::test]
fn ex_set_writes_document_scoped_settings(cx: &mut TestAppContext) {
    let product = start(cx);
    let session = open(&product, "one", 0, cx);
    let here = |key: &str, cx: &mut TestAppContext| session.setting(key, cx);
    let elsewhere = |key: &str, cx: &mut TestAppContext| product.setting(key, Some("editor"), cx);
    session.press(":set ts=2 noet wrap rnu<cr>", cx);
    assert_eq!(here("editor.tabSize", cx), Some(json!(2)));
    assert_eq!(here("editor.insertSpaces", cx), Some(Value::Bool(false)));
    assert_eq!(here("editor.softWrap", cx), Some(Value::Bool(true)));
    assert_eq!(here("editor.lineNumbers", cx), Some(json!("relative")));
    assert_eq!(elsewhere("editor.tabSize", cx), Some(json!(4)));
    assert_eq!(elsewhere("editor.lineNumbers", cx), Some(json!("on")));
    session.press(":set nornu nonu<cr>", cx);
    assert_eq!(here("editor.lineNumbers", cx), Some(json!("off")));

    // An error shows in place of the mode until the next key.
    session.press(":set hls<cr>", cx);
    assert!(shows(&session, "E518: Unknown option: hls", cx));
    session.press("l", cx);
    assert!(!shows(&session, "E518: Unknown option: hls", cx));
}

/// `ctrl-w` runs the workbench's pane commands, with or without ctrl on the
/// second key, and Vim follows the editor each leaves focused. Panes are
/// named by their index in layout order.
#[gpui::test]
fn ctrl_w_splits_moves_between_and_closes_panes(cx: &mut TestAppContext) {
    let product = start(cx);
    let session = open(&product, "one\ntwo", 0, cx);
    let (left, right) = (0, 1);

    session.press("<c-w>v", cx);
    assert_eq!(session.pane_count(cx), 2);
    assert_eq!(
        session.focused_pane(cx),
        right,
        "a split focuses the new pane"
    );
    assert!(!shows_right(&session, "^W", cx));

    session.press("<c-w>h", cx);
    assert_eq!(session.focused_pane(cx), left);
    session.press("<c-w><c-l>", cx);
    assert_eq!(session.focused_pane(cx), right);
    // Vim follows the pane's editor, which takes the keys.
    session.press("<c-w>h<c-w><bs>", cx);
    assert_eq!(session.focused_pane(cx), left);

    // w and W cycle the panes, p goes back, t and b go to the ends.
    session.press("<c-w>w", cx);
    assert_eq!(session.focused_pane(cx), right);
    session.press("<c-w>w", cx);
    assert_eq!(session.focused_pane(cx), left, "w wraps");
    session.press("<c-w>W", cx);
    assert_eq!(session.focused_pane(cx), right, "W wraps back");
    session.press("<c-w>p", cx);
    assert_eq!(session.focused_pane(cx), left);
    session.press("<c-w>p", cx);
    assert_eq!(session.focused_pane(cx), right, "p alternates");
    session.press("<c-w>t", cx);
    assert_eq!(session.focused_pane(cx), left);
    session.press("<c-w>b", cx);
    assert_eq!(session.focused_pane(cx), right);
    session.press("<c-w>h", cx);

    session.press("<c-w>s", cx);
    assert_eq!(session.pane_count(cx), 3);
    session.press("<c-w>k", cx);
    // Closing closes the tab, and its pane only with the last tab.
    session.new_tab(cx);
    session.press("<c-w>c", cx);
    assert_eq!(session.pane_count(cx), 3, "the pane keeps its other tab");
    session.press("<c-w>c", cx);
    assert_eq!(session.pane_count(cx), 2);
    session.press("<c-w>v", cx);
    assert_eq!(session.pane_count(cx), 3);
    for key in ["=", "+", "-", "<lt>", ">"] {
        session.press(&format!("<c-w>{key}"), cx);
        assert_eq!(session.pane_count(cx), 3, "{key}");
    }
    session.press("<c-w>q", cx);
    assert_eq!(session.pane_count(cx), 2, "q closes the pane resizing left");
    session.press("<c-w>o", cx);
    assert_eq!(session.pane_count(cx), 1, "only closes the other panes");
    assert_eq!(session.focused_pane(cx), 0);
}

/// `ctrl-w` moves between the center's editors and never into a dock's
/// tool, where Vim's keys do not apply; Knot's own directional focus still
/// reaches the dock.
#[gpui::test]
fn ctrl_w_directions_stay_in_the_center(cx: &mut TestAppContext) {
    let product = start(cx);
    let session = open(&product, "one", 0, cx);
    let (editor, explorer) = (0, 1);
    session.run_command("workbench.toggle-explorer", cx);
    assert_eq!(session.pane_count(cx), 2);
    assert_eq!(session.focused_pane(cx), editor);
    session.press("<c-w>h", cx);
    assert_eq!(session.focused_pane(cx), editor);
    session.run_command("workbench.focus-pane-left", cx);
    assert_eq!(session.focused_pane(cx), explorer);
}

/// A `ctrl-w` left pending shows `^W`, and the next key either completes it
/// or cancels it without acting as its own command.
#[gpui::test]
fn a_pending_ctrl_w_does_not_swallow_the_next_keys_meaning(cx: &mut TestAppContext) {
    let product = start(cx);
    let session = open(&product, "one\ntwo", 0, cx);
    session.press("<c-w>", cx);
    assert!(shows_right(&session, "^W", cx));
    session.press("<esc>j", cx);
    assert_eq!(session.marked(cx), "one\n‸two");
    assert!(!shows_right(&session, "^W", cx));
    // An unknown key cancels, and the key after it is a plain command.
    session.press("<c-w>xk", cx);
    assert_eq!(session.marked(cx), "‸one\ntwo");
    assert_eq!(session.pane_count(cx), 1);
}

/// `ctrl-w` with a capital moves the tab toward that side, splitting at the
/// edge, and a pane left without tabs goes away.
#[gpui::test]
fn ctrl_w_capital_moves_the_tab_between_panes(cx: &mut TestAppContext) {
    let product = start(cx);
    let session = open(&product, "one\ntwo", 0, cx);
    session.new_tab(cx);
    session.press("<c-w>L", cx);
    assert_eq!(session.pane_count(cx), 2, "the edge splits");
    session.press("<c-w>H", cx);
    assert_eq!(session.pane_count(cx), 1, "the emptied pane collapses");
}

/// A count before or after `ctrl-w` repeats resizing by a step.
#[gpui::test]
fn ctrl_w_counts_repeat_resizing(cx: &mut TestAppContext) {
    let product = start(cx);
    let session = open(&product, "one\ntwo", 0, cx);
    session.press("<c-w>v", cx);
    let split = session.focused_pane(cx);
    session.press("<c-w>h", cx);
    let ratio = |cx: &mut TestAppContext| session.split_ratio(split, cx).unwrap();
    session.press("3<c-w>>", cx);
    assert!((ratio(cx) - 0.65).abs() < 1e-4, "{}", ratio(cx));
    session.press("<c-w>2<lt>", cx);
    assert!((ratio(cx) - 0.55).abs() < 1e-4, "{}", ratio(cx));
    session.press("<c-w>1", cx);
    assert!(shows_right(&session, "^W1", cx));
    session.press("<esc>", cx);
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
