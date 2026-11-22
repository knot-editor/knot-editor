//! The contracts Knot defines and asks itself: every package serving
//! `completion` contributes, and the latest one serving a language's paste
//! contract answers a paste.

use super::extension_restart::{Fixture, start_allowing_failures};
use super::*;

const COMPLETION_FIRST: &str = r#"
import { completionContract } from "knot";
await completionContract.serve({
  provideCompletions() { return [{ label: "first", insertText: "first" }]; },
});
"#;

const COMPLETION_SECOND: &str = r#"
import { completionContract } from "knot";
await completionContract.serve({
  provideCompletions() { return [{ label: "second", insertText: "second" }]; },
});
"#;

const COMPLETION_MALFORMED: &str = r#"
import { completionContract } from "knot";
await completionContract.serve({
  provideCompletions() { return [{ label: "fine", insertText: "fine" }, { label: 7, insertText: "bad" }]; },
});
"#;

#[gpui::test]
fn every_completion_provider_contributes_not_only_the_latest(cx: &mut TestAppContext) {
    let fixture = start_allowing_failures(
        &[
            ("@example/first", &[], COMPLETION_FIRST),
            ("@example/second", &["@example/first"], COMPLETION_SECOND),
        ],
        cx,
    );
    let execution = fixture.command(
        super::super::super::commands::dispatch::SHOW_COMPLETIONS_COMMAND,
        cx,
    );
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    let editor = fixture.editor(cx);
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| {
            editor
                .read(cx)
                .completion_state()
                .is_some_and(|(_, pending, _, _)| pending == 0)
        })
    });
    let items = cx.read(|cx| {
        editor
            .read(cx)
            .completion_surface()
            .unwrap()
            .snapshot()
            .items
            .iter()
            .map(|item| (item.label.clone(), item.provider_label.clone()))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        items,
        vec![
            ("first".to_owned(), "@example/first".to_owned()),
            ("second".to_owned(), "@example/second".to_owned()),
        ]
    );
}

#[gpui::test]
fn a_malformed_completion_fails_only_its_own_provider(cx: &mut TestAppContext) {
    let fixture = start_allowing_failures(
        &[
            ("@example/first", &[], COMPLETION_FIRST),
            (
                "@example/malformed",
                &["@example/first"],
                COMPLETION_MALFORMED,
            ),
        ],
        cx,
    );
    let execution = fixture.command(
        super::super::super::commands::dispatch::SHOW_COMPLETIONS_COMMAND,
        cx,
    );
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    let editor = fixture.editor(cx);
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| {
            editor
                .read(cx)
                .completion_state()
                .is_some_and(|(_, pending, _, _)| pending == 0)
        })
    });
    let (items, failures) = cx.read(|cx| {
        let snapshot = editor.read(cx).completion_surface().unwrap().snapshot();
        (
            snapshot
                .items
                .iter()
                .map(|item| item.label.clone())
                .collect::<Vec<_>>(),
            snapshot
                .failures
                .iter()
                .map(|failure| (failure.provider_label.clone(), failure.message.clone()))
                .collect::<Vec<_>>(),
        )
    });
    assert_eq!(items, vec!["first".to_owned()]);
    assert_eq!(
        failures,
        vec![(
            "@example/malformed".to_owned(),
            "knot completion: provideCompletions: result[1].label: expected string, found number 7 (the provider's schema)".to_owned()
        )]
    );
}

const PASTE_PROVIDER: &str = r#"
import { languages, pasteContract } from "knot";
await languages.register({
  id: "rust",
  name: "Rust",
  grammar: "rust",
  queries: { highlights: "(string_literal) @string" },
});
await pasteContract("rust").serve({
  providePaste({ text, selectedText }) {
    if (text === "boom") throw new Error("paste provider broke");
    if (text === "number") return 7;
    return text === "keep" ? null : `<${selectedText}|${text}>`;
  },
});
"#;

/// Paste `clipboard` over the middle two characters of `xaby` in `language`,
/// returning the text it leaves.
fn paste_over_selection(
    fixture: &Fixture,
    language: Option<&str>,
    clipboard: &str,
    cx: &mut TestAppContext,
) -> String {
    let editor = fixture.editor(cx);
    let model = cx.read(|cx| editor.read(cx).model().clone());
    cx.update(|cx| {
        model.update(cx, |model, _| model.reset_to_text("xaby"));
        crate::app::languages::set_document_language(&model, None, language.map(str::to_owned), cx);
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(clipboard.into()));
    });
    cx.run_until_parked();
    editor.update(cx, |editor, cx| {
        editor.set_selection(1, 3, crate::app::editor::Reveal::Nearest, cx)
    });
    let execution = fixture.command(super::super::PASTE_COMMAND, cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    cx.read(|cx| model.read(cx).text())
}

#[gpui::test]
fn paste_uses_the_answer_of_the_languages_provider(cx: &mut TestAppContext) {
    let fixture = start_allowing_failures(&[("@example/paste", &[], PASTE_PROVIDER)], cx);
    let rust = Some("rust");
    assert_eq!(
        paste_over_selection(&fixture, rust, "url", cx),
        "x<ab|url>y"
    );
    // A null answer or a throw pastes the clipboard's text.
    assert_eq!(paste_over_selection(&fixture, rust, "keep", cx), "xkeepy");
    assert_eq!(paste_over_selection(&fixture, rust, "boom", cx), "xboomy");
    // An answer the contract does not allow pastes the clipboard's text too.
    assert_eq!(
        paste_over_selection(&fixture, rust, "number", cx),
        "xnumbery"
    );
    // A language without a provider pastes the text as is.
    assert_eq!(paste_over_selection(&fixture, None, "url", cx), "xurly");
}

const PREPARE_PASTE: &str = r#"
import { commands, workbench } from "knot";
await commands.register("prepare.paste", async ({ editor }) => {
  const result = {
    selection: await editor.preparePaste("url"),
    keep: await editor.preparePaste("keep"),
    boom: await editor.preparePaste("boom"),
    range: await editor.preparePaste("url", [0, 1]),
  };
  try { await editor.preparePaste("url", [0, 9]); } catch (error) { result.invalid = error.code; }
  await workbench.writeClipboardText(JSON.stringify(result));
});
"#;

#[gpui::test]
fn prepare_paste_answers_without_editing(cx: &mut TestAppContext) {
    let fixture = start_allowing_failures(
        &[
            ("@example/paste", &[], PASTE_PROVIDER),
            ("@example/prepare", &[], PREPARE_PASTE),
        ],
        cx,
    );
    let editor = fixture.editor(cx);
    let model = cx.read(|cx| editor.read(cx).model().clone());
    cx.update(|cx| {
        model.update(cx, |model, _| model.reset_to_text("xaby"));
        crate::app::languages::set_document_language(&model, None, Some("rust".into()), cx);
    });
    cx.run_until_parked();
    editor.update(cx, |editor, cx| {
        editor.set_selection(1, 3, crate::app::editor::Reveal::Nearest, cx)
    });
    let answer: serde_json::Value =
        serde_json::from_str(&fixture.clipboard_after("prepare.paste", cx)).unwrap();
    assert_eq!(
        answer,
        serde_json::json!({
            "selection": "<ab|url>",
            "keep": "keep",
            "boom": "boom",
            "range": "<x|url>",
            "invalid": "InvalidRange",
        })
    );
    assert_eq!(cx.read(|cx| model.read(cx).text()), "xaby");

    // Without a provider for the language, the text comes back as is.
    cx.update(|cx| crate::app::languages::set_document_language(&model, None, None, cx));
    cx.run_until_parked();
    let answer: serde_json::Value =
        serde_json::from_str(&fixture.clipboard_after("prepare.paste", cx)).unwrap();
    assert_eq!(answer["selection"], "url");
}
