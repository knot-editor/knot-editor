//! The bundled Vim package driven by real keystrokes in a product window.
//!
//! The behavior cases live beside the package in `test/cases.jsonc` and run
//! here through the real editor, native commands, and ordered key delivery.
//! A case starts from `before`, where `‸` marks the caret, or both ends of a
//! selection, anchor first, types `keys`, and expects `after` with the caret
//! or the selection at its markers and the editor in `mode`.
//! In `keys`, characters are typed and `<name>` presses a named key:
//! `<esc>`, `<cr>`, `<bs>`, `<del>`, `<tab>`, `<s-tab>`, `<lt>` for `<`, and
//! `<c-x>` for ctrl with a letter. Keys are pressed without waiting between
//! them, as fast typing arrives, and the run settles once at the end.
use serde::Deserialize;

use super::editor_api::active_editor;
use super::language_editing::start_bundled_packages_with_post_init;
use super::language_indents::{CARET, case_files, unmark};
use super::*;
use crate::app::editor::{CaretShape, EditorView};
use crate::app::product::DocumentId;
use crate::app::status_bar::ApplicationStatusBar;

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
}

fn normal() -> String {
    "normal".into()
}

/// One product window over a document whose editor Vim follows.
struct Fixture {
    shell: Entity<ProductShell>,
    window: gpui::AnyWindowHandle,
    editor: Entity<EditorView>,
    model: Entity<BufferModel>,
    document: DocumentId,
}

impl Fixture {
    fn open(text: &str, language: Option<&str>, caret: usize, cx: &mut TestAppContext) -> Self {
        Self::open_selected(text, language, (caret, caret), cx)
    }

    fn open_selected(
        text: &str,
        language: Option<&str>,
        (anchor, caret): (usize, usize),
        cx: &mut TestAppContext,
    ) -> Self {
        let documents = cx.read(|cx| cx.global::<ApplicationDocuments>().0.clone());
        let model = cx.new(|_| BufferModel::from_text(text));
        let document = documents.update(cx, |documents, cx| {
            documents.create_untitled("Vim", model.clone(), cx)
        });
        if let Some(language) = language {
            cx.update(|cx| crate::app::languages::set_language(&model, language.into(), cx));
        }
        let (shell, window) = product_window(document, model.clone(), cx);
        let editor = active_editor(&shell, cx);
        cx.update_window(window, |_, window, cx| {
            window.activate_window();
            editor.update(cx, |editor, cx| {
                editor.set_selection(anchor, caret, crate::app::editor::Reveal::Nearest, cx)
            });
            shell.read(cx).focus_active_editor(window, cx)
        })
        .unwrap();
        cx.refresh().unwrap();
        let fixture = Self {
            shell,
            window,
            editor,
            model,
            document,
        };
        // Entering normal mode sets the context and the caret separately,
        // and a selection then enters visual mode.
        let mode = if anchor == caret { "normal" } else { "visual" };
        wait_until(cx, "Vim to follow the editor", |cx| {
            fixture.mode_in(cx) == mode
                && fixture.editor.read(cx).caret_shape() == CaretShape::Block
        });
        fixture
    }

    /// Close the window and release its document, so later cases do not
    /// pay for them.
    fn close(&self, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, _| window.remove_window())
            .unwrap();
        let documents = cx.read(|cx| cx.global::<ApplicationDocuments>().0.clone());
        documents.update(cx, |documents, cx| documents.remove(self.document, cx));
        cx.update(crate::app::application_observation::changed);
        cx.run_until_parked();
    }

    fn contexts(&self, cx: &gpui::App) -> Vec<String> {
        self.editor
            .read(cx)
            .key_contexts()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    /// Press `keys` in the case notation, then wait until every command
    /// they started has finished.
    fn press(&self, keys: &str, cx: &mut TestAppContext) {
        for key in parse_keys(keys) {
            cx.dispatch_keystroke(self.window, gpui::Keystroke::parse(&key).unwrap());
        }
        wait_until(cx, "the key hold to release", |cx| {
            !self.shell.read(cx).holds_keys()
        });
        cx.run_until_parked();
    }

    /// The text with `‸` at the caret, or at both ends of a selection.
    fn marked(&self, cx: &mut TestAppContext) -> String {
        cx.read(|cx| {
            let mut text = self.model.read(cx).text();
            let (anchor, caret) = self.editor.read(cx).selection(cx);
            text.insert(anchor.max(caret), CARET);
            if anchor != caret {
                text.insert(anchor.min(caret), CARET);
            }
            text
        })
    }

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
        let (left, _) = self.status_in(cx);
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

    /// The window's status bar texts, left side then right side.
    fn status(&self, cx: &mut TestAppContext) -> (Vec<String>, Vec<String>) {
        cx.read(|cx| self.status_in(cx))
    }

    fn status_in(&self, cx: &gpui::App) -> (Vec<String>, Vec<String>) {
        let key = self.window.window_id().as_u64().to_string();
        let items = cx.global::<ApplicationStatusBar>().0.items(&key, &[]);
        let texts = |side: Vec<crate::app::status_bar::BarItem>| {
            side.into_iter().map(|bar| bar.item.text).collect()
        };
        (texts(items.left), texts(items.right))
    }
}

/// Wait for `ready`, naming what never happened when it does not.
fn wait_until(cx: &mut TestAppContext, what: &str, mut ready: impl FnMut(&gpui::App) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if cx.read(&mut ready) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        std::thread::yield_now();
    }
}

/// gpui keystrokes for the case notation.
fn parse_keys(keys: &str) -> Vec<String> {
    let mut parsed = Vec::new();
    let mut rest = keys;
    while let Some(character) = rest.chars().next() {
        if character == '<'
            && let Some(end) = rest.find('>')
        {
            let name = &rest[1..end];
            parsed.push(match name {
                "esc" => "escape".to_owned(),
                "cr" => "enter".to_owned(),
                "bs" => "backspace".to_owned(),
                "del" => "delete".to_owned(),
                "tab" => "tab".to_owned(),
                "s-tab" => "shift-tab".to_owned(),
                "lt" => "<".to_owned(),
                "left" | "right" | "up" | "down" | "home" | "end" => name.to_owned(),
                _ => match name.strip_prefix("c-") {
                    Some(letter) => format!("ctrl-{letter}"),
                    None => panic!("unknown key <{name}>"),
                },
            });
            rest = &rest[end + 1..];
        } else {
            parsed.push(match character {
                ' ' => "space".to_owned(),
                upper if upper.is_ascii_uppercase() => {
                    format!("shift-{}", upper.to_ascii_lowercase())
                }
                other => other.to_string(),
            });
            rest = &rest[character.len_utf8()..];
        }
    }
    parsed
}

#[gpui::test]
fn the_vim_cases_behave_as_listed(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
    let mut failures = Vec::new();
    let mut count = 0;
    for path in case_files("cases.jsonc") {
        let text = std::fs::read_to_string(&path).unwrap();
        let cases: Cases = jsonc_parser::parse_to_serde_value(&text, &Default::default())
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        for case in cases.cases {
            count += 1;
            let (before, selection) = unmark(&case.before);
            let Some(selection) = selection else {
                panic!("{} marks no caret", case.name);
            };
            let fixture = Fixture::open_selected(&before, case.language.as_deref(), selection, cx);
            fixture.press(&case.keys, cx);
            let expected = case.after.join("\n");
            let actual = fixture.marked(cx);
            let mode = fixture.mode(cx);
            fixture.close(cx);
            if actual != expected || mode != case.mode {
                failures.push(format!(
                    "{} (keys {:?})\n  expected ({}):\n{expected}\n  actual ({mode}):\n{actual}",
                    case.name, case.keys, case.mode
                ));
            }
        }
    }
    assert!(count > 0, "no Vim cases found");
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[gpui::test]
fn the_mode_shows_in_the_status_bar_and_the_caret(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
    let fixture = Fixture::open("one two", None, 0, cx);
    let shows = |side: &[String], text: &str| side.iter().any(|item| item == text);
    assert_eq!(fixture.caret_shape(cx), CaretShape::Block);

    fixture.press("2g", cx);
    let (_, right) = fixture.status(cx);
    assert!(
        shows(&right, "2g"),
        "pending keys show while typed: {right:?}"
    );

    fixture.press("<esc>i", cx);
    let (left, right) = fixture.status(cx);
    assert!(shows(&left, "-- INSERT --"), "{left:?}");
    assert!(!shows(&right, "2g"));
    assert_eq!(fixture.caret_shape(cx), CaretShape::Bar);

    fixture.press("<esc>", cx);
    let (left, _) = fixture.status(cx);
    assert!(!shows(&left, "-- INSERT --"));
    assert_eq!(fixture.caret_shape(cx), CaretShape::Block);
}

#[gpui::test]
fn j_moves_by_buffer_line_and_gj_by_row_under_soft_wrap(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
    let long = "word ".repeat(400);
    let fixture = Fixture::open(&format!("{long}\nnext"), None, 0, cx);
    let uuid = cx.read(|cx| {
        cx.global::<ApplicationDocuments>()
            .0
            .read(cx)
            .get(fixture.document)
            .unwrap()
            .identity()
    });
    cx.update(|cx| crate::app::settings::set_document_soft_wrap(uuid, true, cx));
    cx.refresh().unwrap();
    cx.run_until_parked();
    let caret = |cx: &mut TestAppContext| cx.read(|cx| fixture.editor.read(cx).selection(cx).1);

    fixture.press("gj", cx);
    let row = caret(cx);
    assert!(
        row > 0 && row < long.len(),
        "gj stays on the wrapped line: {row}"
    );
    fixture.press("ggj", cx);
    assert_eq!(caret(cx), long.len() + 1);
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
    let fixture = Fixture::open("# notes", None, 0, cx);
    fixture.press("i", cx);
    assert_eq!(fixture.mode(cx), "insert");

    // A language Vim is off for leaves native editing, in insert mode or not.
    cx.update(|cx| {
        crate::app::languages::set_language(&fixture.model, "markdown".into(), cx);
        crate::app::application_observation::changed(cx);
    });
    wait_for_api_condition(cx, |cx| fixture.contexts(cx).is_empty());
    assert_eq!(fixture.caret_shape(cx), CaretShape::Bar);
    fixture.press("x", cx);
    assert_eq!(fixture.marked(cx), "x‸# notes");

    cx.update(|cx| {
        crate::app::languages::set_language(&fixture.model, "plaintext".into(), cx);
        crate::app::application_observation::changed(cx);
    });
    wait_for_api_condition(cx, |cx| fixture.contexts(cx) == ["vim.normal"]);
}

#[gpui::test]
fn registers_persist_across_a_package_restart(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
    let fixture = Fixture::open("one\ntwo", None, 0, cx);
    fixture.press("\"ayy\"byw", cx);

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
    wait_for_api_condition(cx, |_| restarted.lock().unwrap().is_some());
    let entry = restarted.lock().unwrap().take().unwrap();
    assert_eq!(
        entry.result,
        crate::app::extensions::load::LoadResult::Loaded
    );
    wait_until(cx, "the restarted package to follow the editor", |cx| {
        fixture.contexts(cx) == ["vim.normal"]
    });

    // Linewise text stays linewise, and characterwise text characterwise.
    fixture.press("j\"ap\"bP", cx);
    assert_eq!(fixture.marked(cx), "one\ntwo\non‸eone");
}

#[gpui::test]
fn the_clipboard_pastes_linewise_only_while_it_holds_the_yanked_line(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
    let fixture = Fixture::open("one", None, 0, cx);
    fixture.press("yy", cx);
    assert_eq!(
        cx.read(|cx| cx.read_from_clipboard().unwrap().text().unwrap()),
        "one\n"
    );
    fixture.press("p", cx);
    assert_eq!(fixture.marked(cx), "one\n‸one");

    // Text copied elsewhere pastes characterwise, even with a line break.
    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("x\n".into())));
    fixture.press("p", cx);
    assert_eq!(fixture.marked(cx), "one\no‸x\nne");
}

#[gpui::test]
fn a_mouse_drag_enters_visual_mode_and_a_click_leaves_it(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
    let fixture = Fixture::open("one\ntwo\nthree", None, 0, cx);
    let (bounds, text_bounds) = cx.read(|cx| {
        let editor = fixture.editor.read(cx);
        (editor.interaction_bounds(), editor.text_bounds())
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
    };
    let shows = |fixture: &Fixture, cx: &mut TestAppContext, text: &str| {
        fixture.status(cx).0.iter().any(|item| item == text)
    };

    drag(cx);
    wait_until(cx, "the drag to enter visual mode", |cx| {
        fixture.mode_in(cx) == "visual"
    });
    assert!(shows(&fixture, cx, "-- VISUAL --"));
    assert_eq!(fixture.marked(cx), "‸one\ntwo\nthree‸");

    // A click leaves visual mode with the caret where it landed.
    let mut window_cx = VisualTestContext::from_window(fixture.window, cx);
    window_cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    window_cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::default());
    wait_until(cx, "the click to leave visual mode", |cx| {
        fixture.mode_in(cx) == "normal"
    });
    assert!(!shows(&fixture, cx, "-- VISUAL --"));
    assert_eq!(fixture.marked(cx), "‸one\ntwo\nthree");

    // The dragged selection is what an operator takes.
    drag(cx);
    wait_until(cx, "the drag to enter visual mode", |cx| {
        fixture.mode_in(cx) == "visual"
    });
    fixture.press("d", cx);
    assert_eq!(fixture.marked(cx), "‸");
    assert_eq!(fixture.mode(cx), "normal");
}

#[gpui::test]
fn a_global_mark_jumps_to_another_open_document(cx: &mut TestAppContext) {
    start_bundled_packages_with_post_init(Some(ENABLE), cx);
    let fixture = Fixture::open("one\ntwo", None, 0, cx);
    fixture.press("jmA", cx);
    cx.update_window(fixture.window, |_, window, cx| {
        fixture
            .shell
            .update(cx, |shell, cx| shell.new_tab(window, cx))
    })
    .unwrap();
    let other = active_editor(&fixture.shell, cx);
    assert_ne!(other.entity_id(), fixture.editor.entity_id());
    wait_until(cx, "Vim to follow the new tab", |cx| {
        other.read(cx).key_contexts() == ["vim.normal"]
    });

    fixture.press("'A", cx);
    wait_until(cx, "the jump to show the marked document", |cx| {
        let active = fixture
            .shell
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .active_tab()
            .editor()
            .map(|editor| editor.entity_id());
        active == Some(fixture.editor.entity_id())
    });
    assert_eq!(fixture.marked(cx), "one\n‸two");
    // The marked document's editor has the keys now.
    fixture.press("k", cx);
    assert_eq!(fixture.marked(cx), "‸one\ntwo");
}
