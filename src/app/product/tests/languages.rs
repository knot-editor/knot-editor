//! Language registration, language and indentation detection, and explicit
//! languages through the public extension API and the native Open and Save As
//! paths.
use super::*;
use crate::app::workbench::ExtensionView;
use knot_host::lifecycle::ExtensionKey;

const SOURCE: &str = r##"
import { commands, documents, languages, workbench } from "knot";
async function failure(operation) {
  try { await operation(); } catch (error) { return error; }
  throw new Error("expected a failure");
}
const grammars = await languages.grammars();
if (!grammars.includes("rust") || !grammars.includes("python") || !Object.isFrozen(grammars)) {
  throw new Error(`grammars: ${grammars}`);
}
const unknown = await failure(() => languages.register({
  id: "cobol", name: "COBOL", grammar: "cobol", queries: { highlights: "" },
}));
if (unknown.code !== "UnknownGrammar") throw unknown;
const invalid = await failure(() => languages.register({
  id: "rust", name: "Rust", grammar: "rust",
  queries: { highlights: "(identifier) @variable\n(not_a_node) @x" },
}));
if (invalid.code !== "InvalidQuery" || invalid.details.query !== "highlights"
    || invalid.details.row !== 1) {
  throw new Error(JSON.stringify(invalid.details));
}
const invalidIndents = await failure(() => languages.register({
  id: "rust", name: "Rust", grammar: "rust",
  queries: { highlights: "", indents: "(block) @indent.begin\n((block) @x (#lua-match? @x \"a\"))" },
}));
if (invalidIndents.code !== "InvalidQuery" || invalidIndents.details.query !== "indents"
    || invalidIndents.details.row !== 1) {
  throw new Error(JSON.stringify(invalidIndents.details));
}
const invalidFolds = await failure(() => languages.register({
  id: "rust", name: "Rust", grammar: "rust",
  queries: { highlights: "", folds: "(block) @fold\n(not_a_node) @fold" },
}));
if (invalidFolds.code !== "InvalidQuery" || invalidFolds.details.query !== "folds"
    || invalidFolds.details.row !== 1) {
  throw new Error(JSON.stringify(invalidFolds.details));
}
const typeError = await failure(() => languages.register({
  id: "rust", name: "Rust", grammar: "rust", fileExtensions: [".rs"], queries: { highlights: "" },
}));
if (!(typeError instanceof TypeError)) throw typeError;
for (const editing of [
  { comments: { line: ["// "] } },
  { comments: { block: ["<!--"] } },
  { brackets: [{ open: "", close: ")" }] },
  { brackets: [{ open: "(", close: ")", notIn: ["code"] }] },
]) {
  const error = await failure(() => languages.register({
    id: "bad", name: "Bad", grammar: "rust", queries: { highlights: "" }, ...editing,
  }));
  if (!(error instanceof TypeError)) throw error;
}
await languages.register({
  id: "edited", name: "Edited", grammar: "python",
  queries: { highlights: "", indents: "(block) @indent.begin", folds: "" },
  comments: { line: ["#"], block: ['"""', '"""'] },
  brackets: [{ open: "(", close: ")" }, { open: "'", close: "'", notIn: ["string"] }],
  autoCloseBefore: ";)",
});
const edited = await languages.get("edited");
const expected = {
  id: "edited", name: "Edited", grammar: "python", fileExtensions: [], fileNames: [],
  interpreters: [], queries: { highlights: "", injections: "", indents: "(block) @indent.begin", folds: "" },
  comments: { line: ["#"], block: ['"""', '"""'] },
  brackets: [{ open: "(", close: ")" }, { open: "'", close: "'", notIn: ["string"] }],
  autoCloseBefore: ";)",
};
if (JSON.stringify(edited) !== JSON.stringify(expected) || !Object.isFrozen(edited.brackets[1].notIn)) {
  throw new Error(JSON.stringify(edited));
}
// A derived language spreads the definition and replaces one key.
await languages.register({ ...edited, comments: { line: ["//"] } });
const derived = await languages.get("edited");
if (derived.comments.line[0] !== "//" || derived.brackets.length !== 2) {
  throw new Error(JSON.stringify(derived));
}
if (await languages.get("missing") !== null) throw new Error("unregistered language");
await languages.register({
  id: "rust", name: "Rust", grammar: "rust", fileExtensions: ["rs"],
  queries: { highlights: "(identifier) @variable" },
});
await languages.register({
  id: "python", name: "Python", grammar: "python", fileExtensions: ["py"],
  interpreters: ["python"], queries: { highlights: "(identifier) @variable" },
});
const listed = await languages.list();
if (listed.map((language) => language.id).join() !== "edited,python,rust"
    || listed[1].interpreters[0] !== "python" || !Object.isFrozen(listed[1].fileExtensions)) {
  throw new Error(JSON.stringify(listed));
}
await workbench.registerView({
  id: "lang.log", title: "Log", content: { kind: "text" }, language: "rust",
  placement: { location: "bottom" },
});
await commands.register("lang.set", async ({ buffer, arguments: language }) => {
  const document = (await documents.list()).find((candidate) => candidate.buffer === buffer);
  await documents.setLanguage(document.id, language);
  const updated = await documents.get(document.id);
  // An unregistered explicit language reports plain text.
  const registered = (await languages.list()).some((candidate) => candidate.id === language);
  if (language !== null && updated.language !== (registered ? language : "plaintext")) {
    throw new Error(`language ${updated.language}`);
  }
});
"##;

fn language(model: &Entity<BufferModel>, cx: &TestAppContext) -> (Option<String>, bool) {
    model.read_with(cx, |model, _| {
        (
            model.language().map(str::to_owned),
            model.syntax_generation().is_some(),
        )
    })
}

fn reported(model: &Entity<BufferModel>, cx: &TestAppContext) -> String {
    cx.read(|cx| crate::app::languages::effective_language(model.read(cx), cx))
}

fn active_model(shell: &Entity<ProductShell>, cx: &TestAppContext) -> Entity<BufferModel> {
    cx.read(|cx| {
        let pane = shell.read(cx).workbench.read(cx).focused_pane().unwrap();
        pane.active_tab().editor().unwrap().read(cx).model().clone()
    })
}

#[gpui::test]
fn registered_languages_detect_on_open_and_save_as_and_follow_teardown(cx: &mut TestAppContext) {
    let files = tempfile::tempdir().unwrap();
    let source = files.path().join("main.rs");
    std::fs::write(&source, "fn main() {}\n").unwrap();
    let script = files.path().join("script.py");
    let documents = install_globals(cx);
    cx.set_global(ApplicationOpenDialog(Arc::new(FixedOpenDialog(
        OpenDialogOutcome::Selected(source),
    ))));
    cx.set_global(ApplicationSaveDialog(Arc::new(FixedSaveDialog(
        SaveDialogOutcome::Selected(
            ResourceUri::parse(url::Url::from_file_path(&script).unwrap().as_str()).unwrap(),
        ),
    ))));
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let untitled = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    untitled.update(cx, |model, _| model.replace(0..0, "x = 1\n").unwrap());
    let (shell, window) = product_window(document, untitled.clone(), cx);
    let key = ExtensionKey::new(ExtensionId::new(950), ExtensionLifecycleId::new(1));
    let loaded = Arc::new(Mutex::new(None));
    let result = loaded.clone();
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, SOURCE).unwrap();
        cx.spawn(async move |_, _| {
            *result.lock().unwrap() = Some(execution.await.map(|_| ()));
        })
        .detach();
    });
    wait_for_api_condition(cx, |_| loaded.lock().unwrap().is_some());
    loaded.lock().unwrap().take().unwrap().unwrap();
    assert_eq!(language(&untitled, cx), (None, false));
    let editing = cx
        .read(|cx| crate::app::languages::editing_configuration("edited", cx))
        .unwrap();
    assert_eq!(editing.line_comments, ["//"]);
    assert!(!editing.brackets[1].newline);

    let execution = dispatch_product_command(&shell, window, SAVE_AS_COMMAND, cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    assert_eq!(language(&untitled, cx), (Some("python".into()), true));

    let execution = dispatch_product_command(&shell, window, super::super::OPEN_COMMAND, cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    let opened = active_model(&shell, cx);
    assert_eq!(language(&opened, cx), (Some("rust".into()), true));
    cx.run_until_parked();
    assert!(
        opened
            .read_with(cx, |model, _| model.syntax_highlights(0..model.len()))
            .iter()
            .any(|(range, _)| *range == (3..7))
    );

    // An explicit language publishes documentChanged and holds until the
    // document returns to detection; an unregistered one reports plain text.
    host.update(cx, |host, _| host.take_test_application_events());
    let execution = dispatch_product_command_with(
        &shell,
        window,
        "lang.set",
        CommandArgumentValue::String("cobol".into()),
        cx,
    );
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    assert_eq!(language(&opened, cx), (Some("cobol".into()), false));
    assert_eq!(reported(&opened, cx), "plaintext");
    cx.run_until_parked();
    let events = host.update(cx, |host, _| host.take_test_application_events());
    assert!(events.iter().any(|event| {
        let change = serde_json::to_value(&event.change).unwrap();
        change["type"] == "documentChanged" && change["document"]["language"] == "plaintext"
    }));
    let execution =
        dispatch_product_command_with(&shell, window, "lang.set", CommandArgumentValue::Null, cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    assert_eq!(language(&opened, cx), (Some("rust".into()), true));

    let log = cx.read(
        |cx| match &shell.read(cx).contributed_views["lang.log"].content {
            ExtensionView::Text(view) => view.read(cx).model().clone(),
            ExtensionView::Tree(_) => panic!("log is a text frame"),
        },
    );
    assert_eq!(language(&log, cx), (Some("rust".into()), true));

    // Teardown removes the registrations; detected documents lose their
    // language while the frame keeps its fixed, now unregistered, one.
    host.update(cx, |host, cx| host.unload(key, cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(language(&opened, cx), (None, false));
    assert_eq!(language(&untitled, cx), (None, false));
    assert_eq!(language(&log, cx), (Some("rust".into()), false));
    for model in [&opened, &untitled, &log] {
        assert_eq!(reported(model, cx), "plaintext");
    }
}

#[gpui::test]
fn set_language_mode_picks_an_explicit_language_or_returns_to_detection(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    model.update(cx, |model, _| model.replace(0..0, "x = 1\n").unwrap());
    let (shell, window) = product_window(document, model.clone(), cx);
    let key = ExtensionKey::new(ExtensionId::new(951), ExtensionLifecycleId::new(1));
    let loaded = Arc::new(Mutex::new(None));
    let result = loaded.clone();
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, SOURCE).unwrap();
        cx.spawn(async move |_, _| {
            *result.lock().unwrap() = Some(execution.await.map(|_| ()));
        })
        .detach();
    });
    wait_for_api_condition(cx, |_| loaded.lock().unwrap().is_some());
    loaded.lock().unwrap().take().unwrap().unwrap();

    let pick = |label: &str, cx: &mut TestAppContext| {
        let execution = dispatch_product_command(
            &shell,
            window,
            super::super::commands::SET_LANGUAGE_MODE_COMMAND,
            cx,
        );
        assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
        wait_for_api_condition(cx, |cx| shell.read(cx).transient.is_some());
        cx.simulate_input(window, label);
        cx.simulate_keystrokes(window, "enter");
        cx.run_until_parked();
    };
    pick("Python", cx);
    assert_eq!(language(&model, cx), (Some("python".into()), true));
    assert!(model.read_with(cx, |model, _| model.language_is_explicit()));
    pick("Auto Detect", cx);
    assert_eq!(language(&model, cx), (None, false));
    assert!(!model.read_with(cx, |model, _| model.language_is_explicit()));
}

#[gpui::test]
fn opened_documents_detect_their_indentation_and_untitled_ones_do_not(cx: &mut TestAppContext) {
    let files = tempfile::tempdir().unwrap();
    let source = files.path().join("main.c");
    std::fs::write(&source, "int main() {\n\tif (1) {\n\t\treturn 0;\n\t}\n}\n").unwrap();
    let documents = install_globals(cx);
    cx.set_global(ApplicationOpenDialog(Arc::new(FixedOpenDialog(
        OpenDialogOutcome::Selected(source),
    ))));
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let untitled = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (shell, window) = product_window(document, untitled.clone(), cx);

    let execution = dispatch_product_command(&shell, window, super::super::OPEN_COMMAND, cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    let opened = active_model(&shell, cx);
    cx.read(|cx| {
        let spaces = |model: &Entity<BufferModel>| {
            crate::app::settings::editor_insert_spaces(
                &crate::app::settings::SettingsScope {
                    document: model.read(cx).document().map(|id| id.to_string()),
                    ..Default::default()
                },
                cx,
            )
        };
        assert!(!spaces(&opened));
        assert!(spaces(&untitled));
    });
}
