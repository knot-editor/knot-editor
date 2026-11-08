//! The bundled Vim package driven by real keystrokes through a product
//! session.
//!
//! The behavior cases live beside the package in `test/cases/<topic>.jsonc`,
//! in the [case](super::cases) format, and each topic runs as its own test
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
use serde::Deserialize;

use super::cases::{Failures, Text, package_files, parse_keys, read_groups, topics, unmark};
use super::editor_api::active_editor_view;
use super::harness::{Session, Windowed, start_bundled_packages_with_post_init, work_per_key};
use super::*;
use crate::app::editor::CaretShape;

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

impl Session {
    /// The mode from the key context, and in normal mode's context from
    /// the status bar, which names the visual modes.
    fn mode(&self, cx: &mut TestAppContext) -> String {
        cx.read(|cx| self.mode_in(cx))
    }

    fn mode_in(&self, cx: &gpui::App) -> String {
        let mode = match self.contexts(cx).as_slice() {
            [context] => context.trim_start_matches("vim.").to_owned(),
            other => return format!("{other:?}"),
        };
        let (left, _) = self.status(cx);
        if left.iter().any(|text| text == "-- VISUAL --") {
            "visual".into()
        } else if left.iter().any(|text| text == "-- VISUAL LINE --") {
            "visual-line".into()
        } else {
            mode
        }
    }

    fn caret_shape(&self, cx: &mut TestAppContext) -> CaretShape {
        cx.read(|cx| self.editor.read(cx).caret_shape())
    }

    fn caret(&self, cx: &mut TestAppContext) -> usize {
        cx.read(|cx| self.editor.read(cx).selection(cx).1)
    }

    /// Lay the editor out to show exactly `lines` whole lines, then scroll
    /// line `top` to the top.
    fn show_lines(&self, lines: usize, top: usize, cx: &mut TestAppContext) {
        self.lay_out(lines, 100, cx);
        self.editor.update(cx, |editor, cx| {
            editor.scroll_by_rows(-1_000_000_000, cx);
            editor.scroll_by_rows(top as i64, cx);
        });
        assert_eq!(self.top_line(cx), top, "the view starts at line {top}");
    }

    /// Collapse the folds `folds`, as first and last lines.
    fn fold_lines(&self, folds: &[[usize; 2]], cx: &mut TestAppContext) {
        self.editor.update(cx, |editor, cx| {
            for [first, last] in folds {
                assert!(editor.fold(*first..*last + 1, cx), "fold {first}..={last}");
            }
        });
        self.settle(cx);
    }

    /// The collapsed folds as first and last lines.
    fn fold_lines_now(&self, cx: &mut TestAppContext) -> Vec<[usize; 2]> {
        cx.read(|cx| {
            self.editor
                .read(cx)
                .folds()
                .into_iter()
                .map(|lines| [lines.start, lines.end - 1])
                .collect()
        })
    }

    /// The first line wholly in view.
    fn top_line(&self, cx: &mut TestAppContext) -> usize {
        cx.read(|cx| {
            let editor = self.editor.read(cx);
            editor.row_start(editor.viewport().0, cx).0
        })
    }
}

/// Open `text` with the caret at `caret` in a session Vim follows.
fn open(text: &str, caret: usize, cx: &mut TestAppContext) -> Session {
    let session = Session::open(text, None, (caret, caret), cx);
    assert_eq!(session.mode(cx), "normal", "Vim follows the editor");
    session
}

/// Start over from a fresh start's package state, so no case sees what an
/// earlier one left: an empty clipboard, and no registers, register record,
/// marks, or last find.
fn start_over(session: &Session, cx: &mut TestAppContext) {
    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new())));
    session.run_command("vim.reset", cx);
}

/// Run the cases of the topic file `topic`.
fn run_topic(topic: &str, cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let mut failures = Failures::default();
    let mut count = 0;
    // One session shows every case in turn.
    let mut session: Option<Session> = None;
    for path in package_files("vim")
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
                    None => session = Some(Session::open(&before, language, selection, cx)),
                }
                let session = session.as_ref().unwrap();
                start_over(session, cx);
                if let Some(view) = &value.view {
                    session.show_lines(view.lines, view.top, cx);
                }
                session.fold_lines(&value.folds, cx);
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
                let mode = session.mode(cx);
                let top = session.top_line(cx);
                let folds = session.fold_lines_now(cx);
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
    "vim",
    run_topic;
    ex, folding, insert, marks, motions, objects, operators, registers, scrolling, visual
);

#[gpui::test]
fn the_mode_shows_in_the_status_bar_and_the_caret(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let session = open("one two", 0, cx);
    let shows = |side: &[String], text: &str| side.iter().any(|item| item == text);
    assert_eq!(session.caret_shape(cx), CaretShape::Block);

    session.press("2g", cx);
    let (_, right) = cx.read(|cx| session.status(cx));
    assert!(
        shows(&right, "2g"),
        "pending keys show while typed: {right:?}"
    );

    session.press("<esc>i", cx);
    let (left, right) = cx.read(|cx| session.status(cx));
    assert!(shows(&left, "-- INSERT --"), "{left:?}");
    assert!(!shows(&right, "2g"));
    assert_eq!(session.caret_shape(cx), CaretShape::Bar);

    session.press("<esc>", cx);
    let (left, _) = cx.read(|cx| session.status(cx));
    assert!(!shows(&left, "-- INSERT --"));
    assert_eq!(session.caret_shape(cx), CaretShape::Block);
}

#[gpui::test]
fn j_moves_by_buffer_line_and_gj_by_row_under_soft_wrap(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let long = "word ".repeat(400);
    let session = open(&format!("{long}\nnext"), 0, cx);
    let uuid = cx.read(|cx| {
        cx.global::<ApplicationDocuments>()
            .0
            .read(cx)
            .get(session.document)
            .unwrap()
            .identity()
    });
    cx.update(|cx| crate::app::settings::set_document_soft_wrap(uuid, true, cx));
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
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let long = "word ".repeat(60);
    let lines = [
        "alpha", &long, &long, "beta", &long, "gamma", &long, "delta", "end",
    ];
    let session = open(&lines.join("\n"), 0, cx);
    let uuid = cx.read(|cx| {
        cx.global::<ApplicationDocuments>()
            .0
            .read(cx)
            .get(session.document)
            .unwrap()
            .identity()
    });
    cx.update(|cx| crate::app::settings::set_document_soft_wrap(uuid, true, cx));
    session.lay_out(8, 100, cx);
    let (first, rows) = cx.read(|cx| session.editor.read(cx).viewport());
    assert_eq!((first, rows), (0, 8));
    // The line each of the viewport's rows belongs to.
    let line_of = |row: usize, cx: &mut TestAppContext| {
        cx.read(|cx| session.editor.read(cx).row_start(row, cx).0)
    };
    let caret_line = |cx: &mut TestAppContext| {
        let caret = session.caret(cx);
        cx.read(|cx| {
            let model = session.model.read(cx);
            (0..model.line_count())
                .rev()
                .find(|line| model.line_range(*line).start <= caret)
                .unwrap()
        })
    };
    let bottom = line_of(7, cx);
    assert!(
        bottom < lines.len() - 1,
        "the viewport ends inside the text"
    );
    assert!(
        bottom - line_of(0, cx) < 7,
        "wrapped lines take several rows"
    );

    session.press("L", cx);
    assert_eq!(caret_line(cx), bottom, "L goes to the last row's line");
    session.press("2H", cx);
    assert_eq!(
        caret_line(cx),
        line_of(1, cx),
        "2H goes to the second row's line"
    );
    session.press("M", cx);
    assert_eq!(
        caret_line(cx),
        line_of(3, cx),
        "M goes to the middle row's line"
    );

    // A half-page scroll moves the view by half its rows, so the caret
    // moves the same number of rows.
    session.press("gg<c-d>", cx);
    let (first, _) = cx.read(|cx| session.editor.read(cx).viewport());
    assert_eq!(first, 4, "ctrl-d scrolls half of the 8 rows");
    assert_eq!(caret_line(cx), line_of(4, cx));
    session.press("<c-u>", cx);
    assert_eq!(cx.read(|cx| session.editor.read(cx).viewport().0), 0);

    // zz centers the caret's row even though it is on screen.
    session.press("5G", cx);
    let row = cx.read(|cx| session.editor.read(cx).row_of_position(4, 0, cx).unwrap());
    assert!(row >= 6, "line 5 starts below the first screenful: {row}");
    session.press("zz", cx);
    let (first, rows) = cx.read(|cx| session.editor.read(cx).viewport());
    // The caret's row is centered, leaving half a row cut off at each end.
    assert_eq!((first, rows), (row - 3, 7));
}

#[gpui::test]
fn scrolls_move_through_the_rows_of_one_wrapped_line(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let long = "word ".repeat(400);
    let session = open(&format!("{long}\nend"), 0, cx);
    let uuid = cx.read(|cx| {
        cx.global::<ApplicationDocuments>()
            .0
            .read(cx)
            .get(session.document)
            .unwrap()
            .identity()
    });
    cx.update(|cx| crate::app::settings::set_document_soft_wrap(uuid, true, cx));
    session.lay_out(8, 100, cx);
    // The row the caret shows on, and the view's first row.
    let rows = |cx: &mut TestAppContext| {
        let caret = session.caret(cx);
        cx.read(|cx| {
            let editor = session.editor.read(cx);
            (
                editor.row_of_position(0, caret, cx).unwrap(),
                editor.viewport().0,
            )
        })
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
    start_bundled_packages_with_post_init(
        knot_bundled::PACKAGES,
        Some(
            r#"import { settings } from "knot";
await settings.set("vim.enabled", true);
await settings.set("vim.enabled", false, { language: "markdown" });
"#,
        ),
        cx,
    );
    let session = open("# notes", 0, cx);
    session.press("i", cx);
    assert_eq!(session.mode(cx), "insert");

    // A language Vim is off for leaves native editing, in insert mode or not.
    cx.update(|cx| {
        crate::app::languages::set_language(&session.model, "markdown".into(), cx);
        crate::app::application_observation::changed(cx);
    });
    session.settle(cx);
    assert!(cx.read(|cx| session.contexts(cx)).is_empty());
    assert_eq!(session.caret_shape(cx), CaretShape::Bar);
    session.press("x", cx);
    assert_eq!(session.marked(cx), "x‸# notes");

    cx.update(|cx| {
        crate::app::languages::set_language(&session.model, "plaintext".into(), cx);
        crate::app::application_observation::changed(cx);
    });
    session.settle(cx);
    assert_eq!(cx.read(|cx| session.contexts(cx)), ["vim.normal"]);
}

/// Restart the Vim package and wait until it follows the editor again.
fn restart_vim(session: &Session, cx: &mut TestAppContext) {
    let restarted = std::sync::Arc::new(std::sync::Mutex::new(None));
    let result = restarted.clone();
    cx.update(|cx| {
        let host = crate::app::extensions::host::entity(cx).unwrap();
        host.update(cx, |host, cx| {
            let task = host.restart_package("@knot/vim".into(), cx);
            cx.spawn(async move |_, _| *result.lock().unwrap() = Some(task.await))
                .detach();
        })
    });
    session.settle(cx);
    let entry = restarted.lock().unwrap().take().unwrap();
    assert_eq!(
        entry.result,
        crate::app::extensions::load::LoadResult::Loaded
    );
    assert_eq!(cx.read(|cx| session.contexts(cx)), ["vim.normal"]);
}

#[gpui::test]
fn registers_persist_across_a_package_restart(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let session = open("one\ntwo", 0, cx);
    session.press("\"ayy\"byw", cx);

    restart_vim(&session, cx);

    // Linewise text stays linewise, and characterwise text characterwise.
    session.press("j\"ap\"bP", cx);
    assert_eq!(session.marked(cx), "one\ntwo\non‸eone");
}

#[gpui::test]
fn the_clipboard_pastes_linewise_only_while_it_holds_the_yanked_line(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let session = open("one", 0, cx);
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
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let fixture = Windowed::open("one\ntwo\nthree", cx);
    assert_eq!(fixture.session.mode(cx), "normal", "Vim follows the editor");
    let session = &fixture.session;
    let (bounds, text_bounds) = cx.read(|cx| {
        let view = fixture.view.read(cx);
        (view.interaction_bounds(), view.text_bounds())
    });
    let start = point(text_bounds.origin.x + px(1.), text_bounds.origin.y + px(5.));
    let outside = point(
        bounds.origin.x + bounds.size.width + px(20.),
        bounds.origin.y + bounds.size.height + px(20.),
    );
    let drag = |cx: &mut TestAppContext| {
        let mut window_cx = VisualTestContext::from_window(fixture.window, cx);
        window_cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        window_cx.simulate_mouse_move(outside, MouseButton::Left, Modifiers::default());
        window_cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::default());
        session.settle(cx);
    };
    let shows = |cx: &mut TestAppContext, text: &str| {
        cx.read(|cx| session.status(cx))
            .0
            .iter()
            .any(|item| item == text)
    };

    drag(cx);
    assert_eq!(session.mode(cx), "visual");
    assert!(shows(cx, "-- VISUAL --"));
    assert_eq!(session.marked(cx), "‸one\ntwo\nthree‸");

    // A click leaves visual mode with the caret where it landed.
    let mut window_cx = VisualTestContext::from_window(fixture.window, cx);
    window_cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    window_cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::default());
    session.settle(cx);
    assert_eq!(session.mode(cx), "normal");
    assert!(!shows(cx, "-- VISUAL --"));
    assert_eq!(session.marked(cx), "‸one\ntwo\nthree");

    // The dragged selection is what an operator takes.
    drag(cx);
    assert_eq!(session.mode(cx), "visual");
    fixture.press("d", cx);
    assert_eq!(session.marked(cx), "‸");
    assert_eq!(session.mode(cx), "normal");
}

#[gpui::test]
fn a_global_mark_jumps_to_another_open_document(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let session = open("one\ntwo", 0, cx);
    session.press("jmA", cx);
    session.shell.update(cx, |shell, cx| shell.new_tab(cx));
    session.settle(cx);
    let other = active_editor_view(&session.shell, cx);
    let other_editor = cx.read(|cx| other.read(cx).editor().clone());
    assert_ne!(other_editor, session.editor);
    assert!(cx.read(|cx| other_editor.read(cx).key_contexts() == ["vim.normal"]));

    session.press("'A", cx);
    let active = active_editor_view(&session.shell, cx);
    assert_eq!(
        cx.read(|cx| active.read(cx).editor().clone()),
        session.editor,
        "the jump shows the marked document"
    );
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
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let fixture = Windowed::open("one two three\nfour five six\n", cx);
    assert_eq!(fixture.session.mode(cx), "normal", "Vim follows the editor");
    // The first key also pays for following the editor.
    work_per_key(&fixture, &["l"], cx);
    let keys = ["w", "d", "w", "i", "a", "<esc>"];
    // At most (draws, requests) per key.
    let bounds = [(3, 16), (3, 3), (7, 26), (3, 10), (3, 7), (6, 20)];
    let counts = work_per_key(&fixture, &keys, cx);
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
    assert_eq!(fixture.session.marked(cx), "one ‸athree\nfour five six\n");
}

/// What one case leaves in the package, the next case starts without:
/// registers, the clipboard, the last find, marks, and the persisted
/// register record a restart would read back.
#[gpui::test]
fn a_case_starts_without_what_an_earlier_one_left(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let mut session = open("one\ntwo", 0, cx);
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

    restart_vim(&session, cx);
    session.press("\"ap", cx);
    assert_eq!(session.marked(cx), "‸xey", "the register record");
}

/// Serve `a.txt` holding "one" and `b.txt` holding "two" from
/// `mem://product/dir/`.
fn serve_files(cx: &mut TestAppContext) -> Arc<MemoryFileSystemProvider> {
    let (filesystems, provider) = memory_filesystems_with_provider();
    provider
        .seed_directory(ResourceUri::parse("mem://product/dir").unwrap())
        .unwrap();
    for (name, text) in [("a.txt", "one"), ("b.txt", "two")] {
        let uri = ResourceUri::parse(&format!("mem://product/dir/{name}")).unwrap();
        provider.seed_file(uri, text.as_bytes()).unwrap();
    }
    cx.set_global(ApplicationFileSystems(filesystems));
    provider
}

/// A session over the served file `uri`.
fn open_file(provider: &MemoryFileSystemProvider, uri: &str, cx: &mut TestAppContext) -> Session {
    let uri = ResourceUri::parse(uri).unwrap();
    let opened = pollster::block_on(provider.read(uri.clone())).unwrap();
    let model = cx.new(|_| BufferModel::from_text(String::from_utf8(opened.bytes).unwrap()));
    let documents = cx.read(|cx| cx.global::<ApplicationDocuments>().0.clone());
    let document = documents.update(cx, |documents, cx| {
        documents.create_persisted("a.txt", model.clone(), uri, 0, opened.version, cx)
    });
    let session = Session::over(document, model, (0, 0), cx);
    assert_eq!(session.mode(cx), "normal", "Vim follows the editor");
    session
}

/// The text of the session's active editor.
fn active_text(session: &Session, cx: &mut TestAppContext) -> String {
    let view = active_editor_view(&session.shell, cx);
    cx.read(|cx| view.read(cx).model().read(cx).text())
}

fn served(provider: &MemoryFileSystemProvider, uri: &str) -> String {
    let file = pollster::block_on(provider.read(ResourceUri::parse(uri).unwrap())).unwrap();
    String::from_utf8(file.bytes).unwrap()
}

#[gpui::test]
fn ex_writes_and_opens_files_beside_the_document(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let provider = serve_files(cx);
    let session = open_file(&provider, "mem://product/dir/a.txt", cx);
    session.press("x:w<cr>", cx);
    assert_eq!(served(&provider, "mem://product/dir/a.txt"), "ne\n");
    let dirty = |cx: &mut TestAppContext| {
        cx.read(|cx| {
            let documents = cx.global::<ApplicationDocuments>().0.read(cx);
            documents.get(session.document).unwrap().is_dirty(cx)
        })
    };
    assert!(!dirty(cx));

    session.press(":e b.txt<cr>", cx);
    assert_eq!(active_text(&session, cx), "two");
    // Vim follows the opened editor.
    session.press("x", cx);
    assert_eq!(active_text(&session, cx), "wo");
}

#[gpui::test]
fn ex_edit_from_an_untitled_document_opens_in_the_first_root(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    serve_files(cx);
    let session = open("", 0, cx);
    session.shell.update(cx, |shell, cx| {
        shell.replace_workspace_roots(vec![ResourceUri::parse("mem://product/dir/").unwrap()], cx);
    });
    session.press(":e b.txt<cr>", cx);
    assert_eq!(active_text(&session, cx), "two");
}

#[gpui::test]
fn ex_quit_asks_before_losing_changes_and_quit_bang_does_not(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let fixture = Windowed::open("one", cx);
    assert_eq!(fixture.session.mode(cx), "normal", "Vim follows the editor");
    let document = fixture.session.document;
    let open = |cx: &mut TestAppContext| {
        cx.read(|cx| {
            cx.global::<ApplicationDocuments>()
                .0
                .read(cx)
                .get(document)
                .is_some()
        })
    };
    fixture.press("x", cx);
    for key in parse_keys(":q<cr>") {
        cx.dispatch_keystroke(fixture.window, gpui::Keystroke::parse(&key).unwrap());
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !cx.has_pending_prompt() {
        assert!(std::time::Instant::now() < deadline, ":q did not ask");
        cx.run_until_parked();
        std::thread::yield_now();
    }
    cx.simulate_prompt_answer("Cancel");
    fixture.session.settle(cx);
    assert!(open(cx));
    assert_eq!(fixture.session.marked(cx), "‸ne");

    fixture.press(":q!<cr>", cx);
    assert!(!cx.has_pending_prompt());
    assert!(!open(cx));
}

#[gpui::test]
fn ex_set_writes_document_scoped_settings(cx: &mut TestAppContext) {
    use crate::app::settings::{LineNumbers, SettingsScope};
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let session = open("one", 0, cx);
    let scope = SettingsScope {
        language: None,
        view: Some("editor".into()),
        document: Some(cx.read(|cx| {
            cx.global::<ApplicationDocuments>()
                .0
                .read(cx)
                .get(session.document)
                .unwrap()
                .identity()
                .to_string()
        })),
    };
    let elsewhere = SettingsScope {
        document: None,
        ..scope.clone()
    };
    session.press(":set ts=2 noet wrap rnu<cr>", cx);
    cx.read(|cx| {
        use crate::app::settings::*;
        assert_eq!(editor_tab_size(&scope, cx), 2);
        assert!(!editor_insert_spaces(&scope, cx));
        assert!(editor_soft_wrap(&scope, cx));
        assert_eq!(editor_line_numbers(&scope, cx), LineNumbers::Relative);
        assert_eq!(editor_tab_size(&elsewhere, cx), 4);
        assert_eq!(editor_line_numbers(&elsewhere, cx), LineNumbers::On);
    });
    session.press(":set nornu nonu<cr>", cx);
    assert_eq!(
        cx.read(|cx| crate::app::settings::editor_line_numbers(&scope, cx)),
        LineNumbers::Off
    );

    // An error shows in place of the mode until the next key.
    let shows = |cx: &mut TestAppContext, text: &str| {
        cx.read(|cx| session.status(cx))
            .0
            .iter()
            .any(|item| item == text)
    };
    session.press(":set hls<cr>", cx);
    assert!(shows(cx, "E518: Unknown option: hls"));
    session.press("l", cx);
    assert!(!shows(cx, "E518: Unknown option: hls"));
}

fn pane_count(session: &Session, cx: &mut TestAppContext) -> usize {
    cx.read(|cx| session.shell.read(cx).workbench.read(cx).panes().len())
}

fn only_remaining(session: &Session, cx: &mut TestAppContext) -> crate::app::workbench::PaneId {
    cx.read(|cx| session.shell.read(cx).workbench.read(cx).panes()[0].id())
}

fn focused_pane(session: &Session, cx: &mut TestAppContext) -> crate::app::workbench::PaneId {
    cx.read(|cx| {
        session
            .shell
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane_id()
            .unwrap()
    })
}

/// `ctrl-w` runs the workbench's pane commands, with or without ctrl on the
/// second key, and Vim follows the editor each leaves focused.
#[gpui::test]
fn ctrl_w_splits_moves_between_and_closes_panes(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let session = open("one\ntwo", 0, cx);
    let first = focused_pane(&session, cx);

    session.press("<c-w>v", cx);
    assert_eq!(pane_count(&session, cx), 2);
    let right = focused_pane(&session, cx);
    assert_ne!(right, first, "a split focuses the new pane");
    assert!(cx.read(|cx| session.status(cx).1.iter().all(|text| text != "^W")));

    session.press("<c-w>h", cx);
    assert_eq!(focused_pane(&session, cx), first);
    session.press("<c-w><c-l>", cx);
    assert_eq!(focused_pane(&session, cx), right);
    // Vim follows the pane's editor, which takes the keys.
    session.press("<c-w>h<c-w><bs>", cx);
    assert_eq!(focused_pane(&session, cx), first);

    // w and W cycle the panes, p goes back, t and b go to the ends.
    session.press("<c-w>w", cx);
    assert_eq!(focused_pane(&session, cx), right);
    session.press("<c-w>w", cx);
    assert_eq!(focused_pane(&session, cx), first, "w wraps");
    session.press("<c-w>W", cx);
    assert_eq!(focused_pane(&session, cx), right, "W wraps back");
    session.press("<c-w>p", cx);
    assert_eq!(focused_pane(&session, cx), first);
    session.press("<c-w>p", cx);
    assert_eq!(focused_pane(&session, cx), right, "p alternates");
    session.press("<c-w>t", cx);
    assert_eq!(focused_pane(&session, cx), first);
    session.press("<c-w>b", cx);
    assert_eq!(focused_pane(&session, cx), right);
    session.press("<c-w>h", cx);

    session.press("<c-w>s", cx);
    assert_eq!(pane_count(&session, cx), 3);
    session.press("<c-w>k", cx);
    // Closing closes the tab, and its pane only with the last tab.
    session.shell.update(cx, |shell, cx| shell.new_tab(cx));
    session.settle(cx);
    session.press("<c-w>c", cx);
    assert_eq!(pane_count(&session, cx), 3, "the pane keeps its other tab");
    session.press("<c-w>c", cx);
    assert_eq!(pane_count(&session, cx), 2);
    session.press("<c-w>v", cx);
    assert_eq!(pane_count(&session, cx), 3);
    for key in ["=", "+", "-", "<lt>", ">"] {
        session.press(&format!("<c-w>{key}"), cx);
        assert_eq!(pane_count(&session, cx), 3, "{key}");
    }
    session.press("<c-w>q", cx);
    assert_eq!(
        pane_count(&session, cx),
        2,
        "q closes the pane resizing left"
    );
    session.press("<c-w>o", cx);
    assert_eq!(pane_count(&session, cx), 1, "only closes the other panes");
    assert_eq!(focused_pane(&session, cx), only_remaining(&session, cx));
}

/// A `ctrl-w` left pending shows `^W`, and the next key either completes it
/// or cancels it without acting as its own command.
#[gpui::test]
fn a_pending_ctrl_w_does_not_swallow_the_next_keys_meaning(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let session = open("one\ntwo", 0, cx);
    session.press("<c-w>", cx);
    assert!(cx.read(|cx| session.status(cx).1.iter().any(|text| text == "^W")));
    session.press("<esc>j", cx);
    assert_eq!(session.marked(cx), "one\n‸two");
    assert!(cx.read(|cx| session.status(cx).1.iter().all(|text| text != "^W")));
    // An unknown key cancels, and the key after it is a plain command.
    session.press("<c-w>xk", cx);
    assert_eq!(session.marked(cx), "‸one\ntwo");
    assert_eq!(pane_count(&session, cx), 1);
}

/// `ctrl-w` with a capital moves the tab toward that side, splitting at the
/// edge, and a pane left without tabs goes away.
#[gpui::test]
fn ctrl_w_capital_moves_the_tab_between_panes(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let session = open("one\ntwo", 0, cx);
    session.shell.update(cx, |shell, cx| shell.new_tab(cx));
    session.settle(cx);
    session.press("<c-w>L", cx);
    assert_eq!(pane_count(&session, cx), 2, "the edge splits");
    session.press("<c-w>H", cx);
    assert_eq!(pane_count(&session, cx), 1, "the emptied pane collapses");
}

/// A count before or after `ctrl-w` repeats resizing by a step.
#[gpui::test]
fn ctrl_w_counts_repeat_resizing(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(knot_bundled::PACKAGES, Some(ENABLE), cx);
    let session = open("one\ntwo", 0, cx);
    session.press("<c-w>v", cx);
    let split = focused_pane(&session, cx);
    session.press("<c-w>h", cx);
    let ratio = |cx: &mut TestAppContext| {
        cx.read(|cx| {
            session
                .shell
                .read(cx)
                .workbench
                .read(cx)
                .split_ratio(split)
                .unwrap()
        })
    };
    session.press("3<c-w>>", cx);
    assert!((ratio(cx) - 0.65).abs() < 1e-4, "{}", ratio(cx));
    session.press("<c-w>2<lt>", cx);
    assert!((ratio(cx) - 0.55).abs() < 1e-4, "{}", ratio(cx));
    session.press("<c-w>1", cx);
    assert!(cx.read(|cx| session.status(cx).1.iter().any(|text| text == "^W1")));
    session.press("<esc>", cx);
}
