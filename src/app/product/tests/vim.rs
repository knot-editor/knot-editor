//! The bundled Vim package driven by real keystrokes in a product window.
//!
//! The behavior cases live beside the package in `test/cases.jsonc` and run
//! here through the real editor, native commands, and ordered key delivery.
//! A case starts from `before`, where `‸` marks the caret, types `keys`, and
//! expects `after` with the caret at its marker and the editor in `mode`.
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
                editor.set_selection(caret, caret, crate::app::editor::Reveal::Nearest, cx)
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
        // Entering normal mode sets the context and the caret separately.
        wait_until(cx, "Vim to follow the editor", |cx| {
            fixture.contexts(cx) == ["vim.normal"]
                && fixture.editor.read(cx).caret_shape() == CaretShape::Block
        });
        fixture
    }

    /// Close the window, so later cases do not pay for it.
    fn close(&self, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, _| window.remove_window())
            .unwrap();
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

    fn mode(&self, cx: &mut TestAppContext) -> String {
        match cx.read(|cx| self.contexts(cx)).as_slice() {
            [context] => context.trim_start_matches("vim.").to_owned(),
            other => format!("{other:?}"),
        }
    }

    fn caret_shape(&self, cx: &mut TestAppContext) -> CaretShape {
        cx.read(|cx| self.editor.read(cx).caret_shape())
    }

    /// The window's status bar texts, left side then right side.
    fn status(&self, cx: &mut TestAppContext) -> (Vec<String>, Vec<String>) {
        cx.read(|cx| {
            let key = self.window.window_id().as_u64().to_string();
            let items = cx.global::<ApplicationStatusBar>().0.items(&key, &[]);
            let texts = |side: Vec<crate::app::status_bar::BarItem>| {
                side.into_iter().map(|bar| bar.item.text).collect()
            };
            (texts(items.left), texts(items.right))
        })
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
            let Some((_, caret)) = selection else {
                panic!("{} marks no caret", case.name);
            };
            let fixture = Fixture::open(&before, case.language.as_deref(), caret, cx);
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
