use gpui::{AppContext, TestAppContext};

use super::*;
use crate::app::{
    documents::{DocumentCollection, DocumentState},
    filesystem::ResourceVersion,
    theme::Highlight,
};
use knot_model::editing::EditingDefinition;

const MARKDOWN_INJECTIONS: &str = r#"
(fenced_code_block
  (info_string (language) @injection.language)
  (code_fence_content) @injection.content)
"#;

fn owner(extension: u64) -> LanguageOwner {
    LanguageOwner {
        extension: ExtensionId::new(extension),
        lifecycle: ExtensionLifecycleId::new(1),
    }
}

fn definition(id: &str, name: &str) -> LanguageDefinition {
    LanguageDefinition {
        info: LanguageInfo {
            id: id.into(),
            name: name.into(),
            grammar: "rust".into(),
            file_extensions: Vec::new(),
            file_names: Vec::new(),
            interpreters: Vec::new(),
        },
        queries: LanguageQueries {
            highlights: "(identifier) @variable".into(),
            injections: String::new(),
            indents: String::new(),
            folds: String::new(),
        },
        editing: EditingDefinition::default(),
    }
}

fn claiming(
    id: &str,
    extensions: &[&str],
    names: &[&str],
    interpreters: &[&str],
) -> LanguageDefinition {
    let mut definition = definition(id, id);
    let owned = |values: &[&str]| values.iter().map(|value| (*value).to_owned()).collect();
    definition.info.file_extensions = owned(extensions);
    definition.info.file_names = owned(names);
    definition.info.interpreters = owned(interpreters);
    definition
}

fn association(language: &str, extensions: &[&str], names: &[&str]) -> LanguageAssociation {
    let owned = |values: &[&str]| values.iter().map(|value| (*value).to_owned()).collect();
    LanguageAssociation {
        language: language.into(),
        file_extensions: owned(extensions),
        file_names: owned(names),
        interpreters: Vec::new(),
    }
}

fn install(cx: &mut TestAppContext) -> Entity<DocumentCollection> {
    let documents = cx.new(|_| DocumentCollection::new());
    cx.set_global(ApplicationDocuments(documents.clone()));
    documents
}

fn register(cx: &mut TestAppContext, owner: LanguageOwner, definition: LanguageDefinition) -> u64 {
    let language = definition.compile().unwrap();
    cx.update(|cx| super::register(owner, language, cx))
}

fn persisted(
    documents: &Entity<DocumentCollection>,
    uri: &str,
    text: &str,
    cx: &mut TestAppContext,
) -> Entity<Buffer> {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(text)));
    let uri = ResourceUri::parse(uri).unwrap();
    documents.update(cx, |documents, cx| {
        documents.create_persisted("file", model.clone(), uri, 0, ResourceVersion::new([1]), cx)
    });
    model
}

fn state(model: &Entity<Buffer>, cx: &TestAppContext) -> (Option<String>, bool, bool) {
    model.read_with(cx, |model, _| {
        (
            model.language().map(str::to_owned),
            model.language_is_explicit(),
            model.syntax_generation().is_some(),
        )
    })
}

#[gpui::test]
fn documents_detect_on_open_and_follow_registration_changes(cx: &mut TestAppContext) {
    let documents = install(cx);
    let before = persisted(&documents, "mem://workspace/main.rs", "fn main() {}", cx);
    assert_eq!(state(&before, cx), (None, false, false));

    let rust = register(cx, owner(1), claiming("rust", &["rs"], &[], &[]));
    assert_eq!(state(&before, cx), (Some("rust".into()), false, true));
    let after = persisted(&documents, "mem://workspace/lib.rs", "", cx);
    assert_eq!(state(&after, cx), (Some("rust".into()), false, true));
    let script = persisted(
        &documents,
        "mem://workspace/run",
        "#!/usr/bin/env rust-script\n",
        cx,
    );
    assert_eq!(state(&script, cx).0, None);

    // An explicit language holds through registration changes, painting
    // plain text while nothing registers it.
    cx.update(|cx| {
        super::set_document_language(&before, None, Some("other".into()), cx);
    });
    assert_eq!(state(&before, cx), (Some("other".into()), true, false));
    let other = register(cx, owner(2), claiming("other", &["rs"], &[], &[]));
    assert_eq!(state(&before, cx), (Some("other".into()), true, true));
    assert_eq!(state(&after, cx).0, Some("other".into()));

    // Removal reveals the earlier claim and drops syntax for the now
    // unregistered explicit language.
    cx.update(|cx| {
        assert!(super::unregister(
            ExtensionId::new(2),
            ExtensionLifecycleId::new(1),
            other,
            cx
        ))
    });
    assert_eq!(state(&before, cx), (Some("other".into()), true, false));
    assert_eq!(state(&after, cx), (Some("rust".into()), false, true));

    let uri = ResourceUri::parse("mem://workspace/main.rs").unwrap();
    cx.update(|cx| super::set_document_language(&before, Some(&uri), None, cx));
    assert_eq!(state(&before, cx), (Some("rust".into()), false, true));

    cx.update(|cx| super::remove_lifecycle(ExtensionId::new(1), ExtensionLifecycleId::new(1), cx));
    assert_eq!(state(&before, cx), (None, false, false));
    assert_eq!(state(&after, cx), (None, false, false));
    let _ = rust;
}

#[gpui::test]
fn save_as_redetects_unless_the_language_is_explicit(cx: &mut TestAppContext) {
    let documents = install(cx);
    register(cx, owner(1), claiming("rust", &["rs"], &[], &[]));
    register(cx, owner(1), claiming("python", &["py"], &[], &[]));
    for explicit in [false, true] {
        let model = cx.new(|_| Buffer::new(BufferModel::from_text("x = 1")));
        let id = documents.update(cx, |documents, cx| {
            documents.create_untitled("Untitled", model.clone(), cx)
        });
        assert_eq!(state(&model, cx).0, None);
        if explicit {
            cx.update(|cx| super::set_document_language(&model, None, Some("rust".into()), cx));
        }
        let uri = ResourceUri::parse(&format!("mem://workspace/{explicit}.py")).unwrap();
        documents.update(cx, |documents, cx| {
            let capture = documents.begin_persistence(id, &model).unwrap();
            assert!(documents.finish_persistence(
                id,
                &model,
                &capture,
                ("saved.py", uri),
                (ResourceVersion::new([1]), 0),
                cx,
            ));
            assert!(matches!(
                documents.get(id).unwrap().state(),
                DocumentState::Persisted { .. }
            ));
        });
        let expected = if explicit { "rust" } else { "python" };
        assert_eq!(state(&model, cx), (Some(expected.into()), explicit, true));
    }
}

#[gpui::test]
fn plain_save_keeps_the_detected_language(cx: &mut TestAppContext) {
    let documents = install(cx);
    register(cx, owner(1), claiming("python", &[], &[], &["python3"]));
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("#!/usr/bin/env python3\n")));
    let uri = ResourceUri::parse("mem://workspace/run").unwrap();
    let id = documents.update(cx, |documents, cx| {
        documents.create_persisted(
            "run",
            model.clone(),
            uri.clone(),
            0,
            ResourceVersion::new([1]),
            cx,
        )
    });
    assert_eq!(state(&model, cx).0, Some("python".into()));

    model.update(cx, |model, _| {
        let len = model.len();
        model.replace(0..len, "#!/bin/sh\n").unwrap()
    });
    let revision = model.read_with(cx, |model, _| model.revision());
    documents.update(cx, |documents, cx| {
        let capture = documents.begin_persistence(id, &model).unwrap();
        assert!(documents.finish_persistence(
            id,
            &model,
            &capture,
            ("run", uri),
            (ResourceVersion::new([2]), revision),
            cx,
        ));
    });
    assert_eq!(state(&model, cx).0, Some("python".into()));
}

#[gpui::test]
fn injecting_languages_reparse_when_registrations_change(cx: &mut TestAppContext) {
    let documents = install(cx);
    let mut markdown = claiming("markdown", &["md"], &[], &[]);
    markdown.info.grammar = "markdown".into();
    markdown.queries.highlights = "(fenced_code_block) @markup.raw.block".into();
    markdown.queries.injections = MARKDOWN_INJECTIONS.into();
    register(cx, owner(1), markdown);
    let text = "```rust\nlet x = 1;\n```\n";
    let model = persisted(&documents, "mem://workspace/README.md", text, cx);
    let unchanged = persisted(&documents, "mem://workspace/main.rs", "", cx);
    cx.run_until_parked();
    let highlights = |cx: &TestAppContext| {
        model.read_with(cx, |model, _| model.syntax_highlights(0..text.len()))
    };
    assert!(
        highlights(cx)
            .iter()
            .all(|(_, highlight)| *highlight == Highlight::resolve("markup.raw.block").unwrap())
    );

    register(cx, owner(1), claiming("rust", &["rs"], &[], &[]));
    let rust_generation = unchanged.read_with(cx, |model, _| model.syntax_generation());
    assert!(rust_generation.is_some());
    cx.run_until_parked();
    let variable = Highlight::resolve("variable").unwrap();
    assert!(
        highlights(cx)
            .iter()
            .any(|(range, highlight)| *highlight == variable && &text[range.clone()] == "x")
    );
    // A registration a language cannot inject keeps its parse.
    register(cx, owner(1), claiming("python", &["py"], &[], &[]));
    assert_eq!(
        unchanged.read_with(cx, |model, _| model.syntax_generation()),
        rust_generation
    );
}

#[gpui::test]
fn buffers_outside_documents_keep_a_fixed_language(cx: &mut TestAppContext) {
    install(cx);
    let model = cx.new(|_| Buffer::new(BufferModel::from_read_only_text("fn main() {}")));
    cx.update(|cx| super::set_language(&model, "rust".into(), cx));
    assert_eq!(state(&model, cx), (Some("rust".into()), true, false));
    register(cx, owner(1), claiming("rust", &["rs"], &[], &[]));
    assert_eq!(state(&model, cx), (Some("rust".into()), true, true));
}

#[gpui::test]
fn associations_redetect_open_documents(cx: &mut TestAppContext) {
    let documents = install(cx);
    register(cx, owner(1), claiming("rust", &["rs"], &[], &[]));
    register(cx, owner(1), claiming("python", &["py"], &[], &[]));
    let main = persisted(&documents, "mem://workspace/main.rs", "", cx);
    let explicit = persisted(&documents, "mem://workspace/lib.rs", "", cx);
    cx.update(|cx| super::set_document_language(&explicit, None, Some("rust".into()), cx));

    let python =
        cx.update(|cx| super::associate(owner(2), association("python", &["rs"], &[]), cx));
    assert_eq!(state(&main, cx), (Some("python".into()), false, true));
    assert_eq!(state(&explicit, cx), (Some("rust".into()), true, true));

    // An unregistered target is kept and paints as plain text.
    let cobol = cx.update(|cx| super::associate(owner(2), association("cobol", &["rs"], &[]), cx));
    assert_eq!(state(&main, cx), (Some("cobol".into()), false, false));
    assert_eq!(
        cx.read(|cx| super::effective_language(main.read(cx), cx)),
        "plaintext"
    );

    for (id, expected) in [(cobol, "python"), (python, "rust")] {
        cx.update(|cx| {
            assert!(super::unregister(
                ExtensionId::new(2),
                ExtensionLifecycleId::new(1),
                id,
                cx
            ))
        });
        assert_eq!(state(&main, cx), (Some(expected.into()), false, true));
    }
}
