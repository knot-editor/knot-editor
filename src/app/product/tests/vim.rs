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

use super::cases::{Failures, Text, package_files, read_groups, topics, unmark};
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
            editor.scroll_by_lines(-1e9, cx);
            editor.scroll_by_lines(top as f64, cx);
        });
        assert_eq!(self.top_line(cx), top, "the view starts at line {top}");
    }

    /// The first line wholly in view.
    fn top_line(&self, cx: &mut TestAppContext) -> usize {
        cx.read(|cx| self.editor.read(cx).fully_visible_lines().0)
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
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
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
                session.press(&value.keys, cx);
                let (text, expected_mode, expected_top) = match &value.after {
                    Expected::Text(text) => (text, "normal", None),
                    Expected::Full(full) => (&full.text, full.mode.as_str(), full.top),
                };
                let expected = text.lines().join("\n");
                let actual = session.marked(cx);
                let mode = session.mode(cx);
                let top = session.top_line(cx);
                if actual != expected
                    || mode != expected_mode
                    || expected_top.is_some_and(|line| line != top)
                {
                    let top = match expected_top {
                        Some(line) => format!("\n  top line: expected {line}, actual {top}"),
                        None => String::new(),
                    };
                    failures.fail(
                        &case,
                        "",
                        format!(
                            "keys {:?}\n  expected ({expected_mode}):\n{expected}\n  actual ({mode}):\n{actual}{top}",
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
    insert, marks, motions, objects, operators, registers, scrolling, visual
);

#[gpui::test]
fn the_mode_shows_in_the_status_bar_and_the_caret(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
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
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
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
fn vim_follows_the_setting_and_a_language_scoped_value(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(
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
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
    let session = open("one\ntwo", 0, cx);
    session.press("\"ayy\"byw", cx);

    restart_vim(&session, cx);

    // Linewise text stays linewise, and characterwise text characterwise.
    session.press("j\"ap\"bP", cx);
    assert_eq!(session.marked(cx), "one\ntwo\non‸eone");
}

#[gpui::test]
fn the_clipboard_pastes_linewise_only_while_it_holds_the_yanked_line(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
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
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
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
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
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
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
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
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
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
