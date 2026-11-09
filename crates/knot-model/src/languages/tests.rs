use super::*;
use crate::editing::{Bracket, BracketPair, CommentTokens, SyntaxFamily};
use crate::syntax::SyntaxQuery;

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

fn names(registry: &LanguageRegistry) -> Vec<(String, String)> {
    registry
        .list()
        .into_iter()
        .map(|info| (info.id, info.name))
        .collect()
}

#[test]
fn the_latest_registration_defines_a_language_and_removal_reveals_the_previous() {
    let mut registry = LanguageRegistry::default();
    registry.register(owner(1), definition("rust", "First").compile().unwrap());
    let second = registry.register(owner(2), definition("rust", "Second").compile().unwrap());
    registry.register(owner(3), definition("rust", "Third").compile().unwrap());
    registry.register(owner(1), definition("go", "Go").compile().unwrap());
    assert_eq!(
        names(&registry),
        [("go".into(), "Go".into()), ("rust".into(), "Third".into())]
    );
    let third = registry.resolve("rust").unwrap();
    assert!(Arc::ptr_eq(&third, &registry.resolve("rust").unwrap()));

    assert!(registry.remove_lifecycle(ExtensionId::new(3), ExtensionLifecycleId::new(1)));
    assert_eq!(names(&registry)[1].1, "Second");
    assert!(!Arc::ptr_eq(&third, &registry.resolve("rust").unwrap()));
    // Only the owner can remove a registration.
    assert!(!registry.unregister(ExtensionId::new(1), ExtensionLifecycleId::new(1), second));
    assert!(registry.unregister(ExtensionId::new(2), ExtensionLifecycleId::new(1), second));
    assert_eq!(names(&registry)[1].1, "First");
    assert!(!registry.unregister(ExtensionId::new(2), ExtensionLifecycleId::new(1), second));

    // A restarted owner registers again and wins.
    registry.register(owner(4), definition("rust", "Later").compile().unwrap());
    assert_eq!(names(&registry)[1].1, "Later");
}

#[test]
fn compile_errors_name_the_grammar_or_query() {
    let mut unknown = definition("cobol", "COBOL");
    unknown.info.grammar = "cobol".into();
    assert!(matches!(
        unknown.compile(),
        Err(SyntaxLanguageError::UnknownGrammar)
    ));
    let mut invalid = definition("rust", "Rust");
    invalid.queries.injections = "(not_a_node) @injection.content".into();
    assert!(matches!(
        invalid.compile(),
        Err(SyntaxLanguageError::InvalidQuery {
            query: SyntaxQuery::Injections,
            row: 0,
            ..
        })
    ));
    let mut invalid = definition("rust", "Rust");
    invalid.queries.indents = "(block) @indent.begin\n(not_a_node) @indent.end".into();
    assert!(matches!(
        invalid.compile(),
        Err(SyntaxLanguageError::InvalidQuery {
            query: SyntaxQuery::Indents,
            row: 1,
            ..
        })
    ));
}

fn edited(id: &str, name: &str, line_comment: &str) -> LanguageDefinition {
    let mut definition = definition(id, name);
    definition.editing = EditingDefinition {
        comments: Some(CommentTokens {
            line: vec![line_comment.into()],
            block: Some(("/*".into(), "*/".into())),
        }),
        brackets: Some(vec![
            BracketPair {
                open: "{".into(),
                close: "}".into(),
                auto_close: None,
                newline: None,
                not_in: None,
            },
            BracketPair {
                open: "'".into(),
                close: "'".into(),
                auto_close: Some(false),
                newline: None,
                not_in: Some(vec![SyntaxFamily::String]),
            },
        ]),
        auto_close_before: Some(";)".into()),
    };
    definition
}

#[test]
fn editing_configuration_follows_the_defining_registration() {
    let mut registry = LanguageRegistry::default();
    assert!(registry.editing("rust").is_none());
    assert!(registry.get("rust").is_none());
    registry.register(owner(1), edited("rust", "First", "//").compile().unwrap());
    let second = registry.register(owner(2), edited("rust", "Second", "#").compile().unwrap());
    // An association neither defines a language nor hides its definition.
    registry.associate(
        owner(3),
        LanguageAssociation {
            language: "rust".into(),
            file_extensions: vec!["rsx".into()],
            file_names: Vec::new(),
            interpreters: Vec::new(),
        },
    );

    let editing = registry.editing("rust").unwrap();
    assert_eq!(editing.line_comments, ["#"]);
    assert_eq!(editing.block_comment, Some(("/*".into(), "*/".into())));
    // Compiling resolves bracket defaults; quotes open no middle line.
    assert_eq!(
        editing.brackets,
        [
            Bracket {
                open: "{".into(),
                close: "}".into(),
                auto_close: true,
                newline: true,
                not_in: Vec::new(),
            },
            Bracket {
                open: "'".into(),
                close: "'".into(),
                auto_close: false,
                newline: false,
                not_in: vec![SyntaxFamily::String],
            },
        ]
    );
    assert_eq!(editing.auto_close_before, ";)");
    assert_eq!(
        registry.get("rust").unwrap(),
        &edited("rust", "Second", "#")
    );

    assert!(registry.unregister(ExtensionId::new(2), ExtensionLifecycleId::new(1), second));
    assert_eq!(registry.editing("rust").unwrap().line_comments, ["//"]);
    assert_eq!(registry.get("rust").unwrap().info.name, "First");
    assert!(registry.remove_lifecycle(ExtensionId::new(1), ExtensionLifecycleId::new(1)));
    assert!(registry.editing("rust").is_none());
    assert!(registry.get("rust").is_none());

    // A language without editing data has an empty configuration.
    registry.register(owner(4), definition("go", "Go").compile().unwrap());
    let editing = registry.editing("go").unwrap();
    assert!(editing.line_comments.is_empty() && editing.brackets.is_empty());
    assert!(editing.block_comment.is_none());
}

#[test]
fn detection_prefers_names_then_longest_extensions_then_interpreters() {
    let mut registry = LanguageRegistry::default();
    for definition in [
        claiming("typescript", &["ts"], &[], &[]),
        claiming("typescript-declarations", &["d.ts"], &[], &[]),
        claiming("makefile", &["mk"], &["Makefile"], &[]),
        claiming("python", &["py"], &[], &["python"]),
        claiming("shellscript", &["sh", "bashrc"], &[], &["bash", "sh"]),
    ] {
        registry.register(owner(1), definition.compile().unwrap());
    }
    let detect = |name: Option<&str>, first_line: &str| registry.detect(name, first_line);
    assert_eq!(detect(Some("main.TS"), ""), Some("typescript".into()));
    assert_eq!(
        detect(Some("index.d.ts"), ""),
        Some("typescript-declarations".into())
    );
    assert_eq!(detect(Some("Makefile"), ""), Some("makefile".into()));
    assert_eq!(detect(Some("makefile"), ""), None);
    assert_eq!(detect(Some(".bashrc"), ""), Some("shellscript".into()));
    assert_eq!(detect(Some("ts"), ""), None);
    // A matching file name wins over the shebang.
    assert_eq!(
        detect(Some("tool.py"), "#!/bin/bash"),
        Some("python".into())
    );
    assert_eq!(
        detect(Some("tool"), "#!/usr/bin/env -S python3.12 -u"),
        Some("python".into())
    );
    assert_eq!(detect(None, "#!/bin/sh -e"), Some("shellscript".into()));
    assert_eq!(
        detect(None, "#!/usr/bin/env FOO=1 bash"),
        Some("shellscript".into())
    );
    assert_eq!(detect(None, "# not a shebang"), None);
    assert_eq!(detect(Some("notes.txt"), ""), None);

    // A longer extension beats a later registration of a shorter one.
    registry.register(
        owner(3),
        claiming("later-ts", &["ts"], &[], &[]).compile().unwrap(),
    );
    assert_eq!(
        registry.detect(Some("index.d.ts"), ""),
        Some("typescript-declarations".into())
    );
    assert_eq!(
        registry.detect(Some("index.ts"), ""),
        Some("later-ts".into())
    );
}

#[test]
fn the_latest_registration_claiming_a_pattern_decides_detection() {
    let mut registry = LanguageRegistry::default();
    registry.register(
        owner(1),
        claiming("cpp", &["h", "hpp"], &[], &[]).compile().unwrap(),
    );
    registry.register(owner(2), claiming("c", &["h"], &[], &[]).compile().unwrap());
    assert_eq!(registry.detect(Some("a.h"), ""), Some("c".into()));
    assert_eq!(registry.detect(Some("a.hpp"), ""), Some("cpp".into()));
    registry.remove_lifecycle(ExtensionId::new(2), ExtensionLifecycleId::new(1));
    assert_eq!(registry.detect(Some("a.h"), ""), Some("cpp".into()));
}

#[test]
fn injections_resolve_by_id_case_insensitively_or_by_extension() {
    let mut registry = LanguageRegistry::default();
    registry.register(
        owner(1),
        claiming("rust", &["rs"], &[], &[]).compile().unwrap(),
    );
    let resolver = registry.resolver();
    let rust = registry.resolve("rust").unwrap();
    for name in ["rust", "Rust", "rs", "RS"] {
        assert!(Arc::ptr_eq(&resolver(name).unwrap(), &rust), "{name}");
    }
    assert!(resolver("python").is_none());
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

#[test]
fn associations_claim_patterns_in_registration_order_without_defining_languages() {
    let mut registry = LanguageRegistry::default();
    registry.register(
        owner(1),
        claiming("markdown", &["md"], &[], &[]).compile().unwrap(),
    );
    registry.register(
        owner(1),
        claiming("rust", &["rs"], &[], &[]).compile().unwrap(),
    );
    let mdx = registry.associate(owner(2), association("markdown", &["mdx"], &["NOTES"]));
    let rs = registry.associate(owner(2), association("python", &["rs"], &[]));
    assert_eq!(
        registry.detect(Some("page.mdx"), ""),
        Some("markdown".into())
    );
    assert_eq!(registry.detect(Some("NOTES"), ""), Some("markdown".into()));
    assert_eq!(registry.detect(Some("main.rs"), ""), Some("python".into()));

    // An association neither defines nor lists a language, even one that is
    // registered nowhere.
    assert!(registry.resolve("python").is_none());
    let listed = registry
        .list()
        .into_iter()
        .map(|info| info.id)
        .collect::<Vec<_>>();
    assert_eq!(listed, ["markdown", "rust"]);

    // A later registration claiming the pattern wins it back, and removing
    // claims reveals the previous one.
    let later = registry.register(
        owner(3),
        claiming("rust", &["rs"], &[], &[]).compile().unwrap(),
    );
    assert_eq!(registry.detect(Some("main.rs"), ""), Some("rust".into()));
    assert!(registry.unregister(ExtensionId::new(3), ExtensionLifecycleId::new(1), later));
    assert_eq!(registry.detect(Some("main.rs"), ""), Some("python".into()));
    assert!(registry.unregister(ExtensionId::new(2), ExtensionLifecycleId::new(1), rs));
    assert_eq!(registry.detect(Some("main.rs"), ""), Some("rust".into()));
    registry.remove_lifecycle(ExtensionId::new(2), ExtensionLifecycleId::new(1));
    assert_eq!(registry.detect(Some("page.mdx"), ""), None);
    assert!(!registry.unregister(ExtensionId::new(2), ExtensionLifecycleId::new(1), mdx));
}
