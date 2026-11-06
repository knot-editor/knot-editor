//! The bundled Vim package driven by real keystrokes through a product
//! session.
//!
//! The behavior cases live beside the package in `test/cases.jsonc` and run
//! here through a windowless session's key router, native commands, and
//! ordered key delivery. A case starts from `before`, where `‸` marks the
//! caret, or both ends of a selection, anchor first, types `keys` in the
//! [harness](super::harness) key notation, and expects `after` with the
//! caret or the selection at its markers and the editor in `mode`. A `view`
//! lays the editor out to show exactly `lines` lines, scrolls `top` to its
//! first line before the keys, and expects `afterTop` there afterwards.
//! Keys are pressed without waiting between them, as fast typing arrives,
//! and the run settles once at the end. Pointer input and the work a key
//! costs a window run in a product window.
use serde::Deserialize;

use super::editor_api::active_editor_view;
use super::harness::{Session, start_bundled_packages_with_post_init};
use super::language_indents::{case_files, unmark};
use super::*;
use crate::app::editor::{CaretShape, Reveal};

const ENABLE: &str = r#"import { settings } from "knot";
await settings.set("vim.enabled", true);
"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cases {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    #[serde(default)]
    language: Option<String>,
    before: Vec<String>,
    keys: String,
    after: Vec<String>,
    #[serde(default = "normal")]
    mode: String,
    #[serde(default)]
    view: Option<View>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct View {
    lines: usize,
    #[serde(default)]
    top: usize,
    #[serde(default)]
    after_top: Option<usize>,
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

/// A product window over a document Vim follows, for pointer input and the
/// work a key costs a window.
struct Windowed {
    session: Session,
    window: gpui::AnyWindowHandle,
    view: Entity<crate::app::editor::EditorView>,
}

impl Windowed {
    fn open(text: &str, cx: &mut TestAppContext) -> Self {
        let documents = cx.read(|cx| cx.global::<ApplicationDocuments>().0.clone());
        let model = cx.new(|_| BufferModel::from_text(text));
        let document = documents.update(cx, |documents, cx| {
            documents.create_untitled("Vim", model.clone(), cx)
        });
        let (shell, window) = product_window(document, model.clone(), cx);
        let view = active_editor_view(&shell, cx);
        let editor = cx.read(|cx| view.read(cx).editor().clone());
        cx.update_window(window, |_, window, cx| {
            window.activate_window();
            editor.update(cx, |editor, cx| {
                editor.set_selection(0, 0, Reveal::Nearest, cx)
            });
            shell.update(cx, |shell, cx| shell.focus_active_editor(cx))
        })
        .unwrap();
        cx.refresh().unwrap();
        let session = Session {
            shell,
            editor,
            model,
            document,
        };
        session.settle(cx);
        assert_eq!(session.mode(cx), "normal", "Vim follows the editor");
        Self {
            session,
            window,
            view,
        }
    }

    /// Press `keys` through the window, then settle.
    fn press(&self, keys: &str, cx: &mut TestAppContext) {
        for key in super::harness::parse_keys(keys) {
            cx.dispatch_keystroke(self.window, gpui::Keystroke::parse(&key).unwrap());
        }
        self.session.settle(cx);
    }
}

#[gpui::test]
fn the_vim_cases_behave_as_listed(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
    let mut failures = Vec::new();
    let mut count = 0;
    // KNOT_CASES runs only the cases whose names contain it.
    let filter = std::env::var("KNOT_CASES").ok();
    let started = std::time::Instant::now();
    for path in case_files("cases.jsonc") {
        let text = std::fs::read_to_string(&path).unwrap();
        let cases: Cases = jsonc_parser::parse_to_serde_value(&text, &Default::default())
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        for case in cases.cases {
            if filter
                .as_ref()
                .is_some_and(|filter| !case.name.contains(filter.as_str()))
            {
                continue;
            }
            count += 1;
            let (before, selection) = unmark(&case.before);
            let Some(selection) = selection else {
                panic!("{} marks no caret", case.name);
            };
            let session = Session::open(&before, case.language.as_deref(), selection, cx);
            if let Some(view) = &case.view {
                session.show_lines(view.lines, view.top, cx);
            }
            session.press(&case.keys, cx);
            let expected = case.after.join("\n");
            let actual = session.marked(cx);
            let mode = session.mode(cx);
            let top = session.top_line(cx);
            session.close(cx);
            let expected_top = case.view.as_ref().and_then(|view| view.after_top);
            if actual != expected
                || mode != case.mode
                || expected_top.is_some_and(|line| line != top)
            {
                let top = match expected_top {
                    Some(line) => format!("\n  top line: expected {line}, actual {top}"),
                    None => String::new(),
                };
                failures.push(format!(
                    "{} (keys {:?})\n  expected ({}):\n{expected}\n  actual ({mode}):\n{actual}{top}",
                    case.name, case.keys, case.mode
                ));
            }
        }
    }
    assert!(count > 0, "no Vim cases found");
    eprintln!(
        "{count} Vim cases in {:?}, {:?} each",
        started.elapsed(),
        started.elapsed() / count
    );
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

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

#[gpui::test]
fn registers_persist_across_a_package_restart(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
    let session = open("one\ntwo", 0, cx);
    session.press("\"ayy\"byw", cx);

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

/// The draws and host requests each key causes, pressed one at a time
/// through the window in the key notation and settled before the next.
fn work_per_key(
    fixture: &Windowed,
    keys: &[&str],
    cx: &mut TestAppContext,
) -> Vec<crate::app::work_counts::WorkCounts> {
    crate::app::work_counts::take();
    keys.iter()
        .map(|key| {
            fixture.press(key, cx);
            crate::app::work_counts::take()
        })
        .collect()
}

/// A key costs a few draws and the host requests its behavior needs: the
/// package's reads and writes, the selection notifications, and the status
/// bar package's position. Unchanged status items send nothing.
#[gpui::test]
fn a_key_costs_few_draws_and_requests(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
    let fixture = Windowed::open("one two three\nfour five six\n", cx);
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
