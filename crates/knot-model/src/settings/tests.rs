use serde_json::json;

use super::*;

const KEY: &str = "test.width";

fn scope(language: Option<&str>, view: Option<&str>) -> SettingsScope {
    SettingsScope {
        language: language.map(Into::into),
        view: view.map(Into::into),
        document: None,
    }
}

fn rust_editor() -> SettingsScope {
    scope(Some("rust"), Some("editor"))
}

fn extension(lifecycle: u64) -> SettingOwner {
    SettingOwner::Extension {
        extension: ExtensionId::new(1),
        lifecycle: ExtensionLifecycleId::new(lifecycle),
    }
}

fn definition(value_type: SettingType, default: Value) -> SettingDefinition {
    SettingDefinition {
        key: KEY.into(),
        value_type,
        default,
        description: "A test width".into(),
    }
}

fn store_with_integer() -> SettingsStore {
    let mut store = SettingsStore::default();
    store
        .define(extension(1), definition(SettingType::Integer, json!(10)))
        .unwrap();
    store
}

#[test]
fn native_definitions_resolve_their_default() {
    let store = SettingsStore::default();
    assert_eq!(store.get(EDITOR_TAB_SIZE, &rust_editor()), Some(&json!(4)));
    assert_eq!(
        store.definition(EDITOR_TAB_SIZE).unwrap().value_type,
        SettingType::Integer
    );
}

#[test]
fn text_lists_accept_only_strings() {
    let mut store = SettingsStore::default();
    assert_eq!(
        store.get(STATUS_BAR_HIDDEN_ITEMS, &SettingsScope::default()),
        Some(&json!([]))
    );
    store
        .set(
            STATUS_BAR_HIDDEN_ITEMS,
            json!(["position", "language"]),
            SettingsScope::default(),
        )
        .unwrap();
    for invalid in [json!("position"), json!(["position", 3])] {
        assert_eq!(
            store.set(STATUS_BAR_HIDDEN_ITEMS, invalid, SettingsScope::default()),
            Err(SettingsError::TypeMismatch {
                expected: SettingType::Strings
            })
        );
    }
    assert_eq!(
        store.get(STATUS_BAR_HIDDEN_ITEMS, &SettingsScope::default()),
        Some(&json!(["position", "language"]))
    );
}

#[test]
fn unscoped_value_replaces_the_default() {
    let mut store = store_with_integer();
    assert_eq!(store.get(KEY, &rust_editor()), Some(&json!(10)));
    store.set(KEY, json!(2), SettingsScope::default()).unwrap();
    assert_eq!(store.get(KEY, &rust_editor()), Some(&json!(2)));
    assert_eq!(store.get(KEY, &SettingsScope::default()), Some(&json!(2)));
}

#[test]
fn scoped_value_beats_a_later_unscoped_one() {
    let mut store = store_with_integer();
    store.set(KEY, json!(8), scope(Some("rust"), None)).unwrap();
    store.set(KEY, json!(2), SettingsScope::default()).unwrap();
    assert_eq!(store.get(KEY, &rust_editor()), Some(&json!(8)));
    assert_eq!(
        store.get(KEY, &scope(Some("python"), Some("editor"))),
        Some(&json!(2))
    );
}

#[test]
fn more_facts_beat_fewer() {
    let mut store = store_with_integer();
    store.set(KEY, json!(3), rust_editor()).unwrap();
    store.set(KEY, json!(8), scope(Some("rust"), None)).unwrap();
    store
        .set(KEY, json!(5), scope(None, Some("editor")))
        .unwrap();
    assert_eq!(store.get(KEY, &rust_editor()), Some(&json!(3)));
    assert_eq!(
        store.get(KEY, &scope(Some("rust"), Some("search.results"))),
        Some(&json!(8))
    );
}

#[test]
fn ties_go_to_the_last_write_and_a_re_set_moves_later() {
    let mut store = store_with_integer();
    store.set(KEY, json!(8), scope(Some("rust"), None)).unwrap();
    store
        .set(KEY, json!(5), scope(None, Some("editor")))
        .unwrap();
    assert_eq!(store.get(KEY, &rust_editor()), Some(&json!(5)));

    store.set(KEY, json!(9), scope(Some("rust"), None)).unwrap();
    assert_eq!(store.get(KEY, &rust_editor()), Some(&json!(9)));
}

#[test]
fn reset_reveals_the_next_match() {
    let mut store = store_with_integer();
    store.set(KEY, json!(2), SettingsScope::default()).unwrap();
    store.set(KEY, json!(8), scope(Some("rust"), None)).unwrap();
    assert!(store.reset(KEY, &scope(Some("rust"), None)));
    assert_eq!(store.get(KEY, &rust_editor()), Some(&json!(2)));
    assert!(!store.reset(KEY, &scope(Some("rust"), None)));
    assert!(store.reset(KEY, &SettingsScope::default()));
    assert_eq!(store.get(KEY, &rust_editor()), Some(&json!(10)));
}

#[test]
fn mismatched_facts_are_ignored() {
    let mut store = store_with_integer();
    store
        .set(KEY, json!(8), scope(Some("python"), None))
        .unwrap();
    store
        .set(KEY, json!(6), scope(None, Some("terminal")))
        .unwrap();
    store
        .set(KEY, json!(7), scope(Some("rust"), Some("terminal")))
        .unwrap();
    assert_eq!(store.get(KEY, &rust_editor()), Some(&json!(10)));
    // A context without a language matches no language-scoped value.
    assert_eq!(
        store.get(KEY, &scope(None, Some("editor"))),
        Some(&json!(10))
    );
}

#[test]
fn definition_adopts_pending_values() {
    let mut store = SettingsStore::default();
    store.set(KEY, json!(6), scope(Some("rust"), None)).unwrap();
    let dropped = store
        .define(extension(1), definition(SettingType::Integer, json!(10)))
        .unwrap();
    assert!(dropped.is_empty());
    assert_eq!(store.get(KEY, &rust_editor()), Some(&json!(6)));
}

#[test]
fn definition_drops_mismatching_pending_values() {
    let mut store = SettingsStore::default();
    store
        .set(KEY, json!("wide"), scope(Some("rust"), None))
        .unwrap();
    store.set(KEY, json!(6), SettingsScope::default()).unwrap();
    let dropped = store
        .define(extension(1), definition(SettingType::Integer, json!(10)))
        .unwrap();
    assert_eq!(dropped, vec![scope(Some("rust"), None)]);
    assert_eq!(store.get(KEY, &rust_editor()), Some(&json!(6)));
}

#[test]
fn typed_writes_are_checked_after_definition() {
    let mut store = store_with_integer();
    for value in [json!("4"), json!(4.5), json!(true), json!(null)] {
        assert_eq!(
            store.set(KEY, value, SettingsScope::default()),
            Err(SettingsError::TypeMismatch {
                expected: SettingType::Integer
            })
        );
    }
    assert_eq!(store.get(KEY, &rust_editor()), Some(&json!(10)));

    let mut number = SettingsStore::default();
    number
        .define(extension(1), definition(SettingType::Number, json!(1.5)))
        .unwrap();
    number.set(KEY, json!(2), SettingsScope::default()).unwrap();
    number
        .set(KEY, json!(2.5), SettingsScope::default())
        .unwrap();
    assert_eq!(
        number.set(KEY, json!("2"), SettingsScope::default()),
        Err(SettingsError::TypeMismatch {
            expected: SettingType::Number
        })
    );
}

#[test]
fn definition_rejects_a_mismatching_default() {
    let mut store = SettingsStore::default();
    assert_eq!(
        store.define(extension(1), definition(SettingType::Boolean, json!(1))),
        Err(SettingsError::TypeMismatch {
            expected: SettingType::Boolean
        })
    );
    assert_eq!(
        store.define(extension(1), definition(SettingType::String, json!(true))),
        Err(SettingsError::TypeMismatch {
            expected: SettingType::String
        })
    );
    assert!(store.definition(KEY).is_none());
}

#[test]
fn duplicate_definitions_are_rejected() {
    let mut store = store_with_integer();
    assert_eq!(
        store.define(extension(2), definition(SettingType::Integer, json!(1))),
        Err(SettingsError::DuplicateSetting)
    );
    let native = SettingDefinition {
        key: EDITOR_TAB_SIZE.into(),
        ..definition(SettingType::Integer, json!(2))
    };
    assert_eq!(
        store.define(extension(2), native),
        Err(SettingsError::DuplicateSetting)
    );
}

#[test]
fn teardown_removes_definitions_and_keeps_values() {
    let mut store = store_with_integer();
    store.set(KEY, json!(3), SettingsScope::default()).unwrap();
    assert!(!store.remove_lifecycle(ExtensionId::new(1), ExtensionLifecycleId::new(2)));
    assert!(store.remove_lifecycle(ExtensionId::new(1), ExtensionLifecycleId::new(1)));
    assert!(store.definition(KEY).is_none());
    assert_eq!(store.get(KEY, &rust_editor()), None);
    assert!(store.definition(EDITOR_TAB_SIZE).is_some());

    // The restarted lifecycle defines the key again and finds its value.
    store
        .define(extension(2), definition(SettingType::Integer, json!(10)))
        .unwrap();
    assert_eq!(store.get(KEY, &rust_editor()), Some(&json!(3)));
}

#[test]
fn undefined_keys_are_inert() {
    let mut store = SettingsStore::default();
    store
        .set(
            "test.typo",
            json!({ "any": "json" }),
            SettingsScope::default(),
        )
        .unwrap();
    assert_eq!(store.get("test.typo", &SettingsScope::default()), None);
    assert!(store.reset("test.typo", &SettingsScope::default()));
}

fn document(id: &str) -> SettingsScope {
    SettingsScope {
        document: Some(id.into()),
        ..rust_editor()
    }
}

#[test]
fn a_document_value_beats_language_and_view_values_whenever_written() {
    let mut store = store_with_integer();
    store.set(KEY, json!(4), document("a")).unwrap();
    store.set(KEY, json!(8), rust_editor()).unwrap();
    store
        .set(KEY, json!(6), scope(None, Some("editor")))
        .unwrap();
    assert_eq!(store.get(KEY, &document("a")), Some(&json!(4)));
    assert_eq!(store.get(KEY, &document("b")), Some(&json!(8)));
    assert_eq!(store.get(KEY, &rust_editor()), Some(&json!(8)));
}

#[test]
fn equal_document_scopes_resolve_to_the_latest_write() {
    let mut store = store_with_integer();
    store.set(KEY, json!(4), document("a")).unwrap();
    store.set(KEY, json!(2), document("a")).unwrap();
    assert_eq!(store.get(KEY, &document("a")), Some(&json!(2)));
}

#[test]
fn a_read_without_a_document_ignores_document_values() {
    let mut store = store_with_integer();
    store.set(KEY, json!(4), document("a")).unwrap();
    assert_eq!(store.get(KEY, &rust_editor()), Some(&json!(10)));
}

#[test]
fn releasing_a_document_drops_only_its_values() {
    let mut store = store_with_integer();
    store.set(KEY, json!(4), document("a")).unwrap();
    store.set(KEY, json!(5), document("b")).unwrap();
    store.set(KEY, json!(8), rust_editor()).unwrap();
    store.release_document("a");
    assert_eq!(store.get(KEY, &document("a")), Some(&json!(8)));
    assert_eq!(store.get(KEY, &document("b")), Some(&json!(5)));
}

#[test]
fn terminals_resolve_their_own_font_size() {
    let mut store = SettingsStore::default();
    let terminal = scope(None, Some("terminal"));
    assert_eq!(store.get(FONT_SIZE, &terminal), Some(&json!(14)));
    store
        .set(FONT_SIZE, json!(16.5), SettingsScope::default())
        .unwrap();
    store.set(FONT_SIZE, json!(12), terminal.clone()).unwrap();
    assert_eq!(store.get(FONT_SIZE, &rust_editor()), Some(&json!(16.5)));
    assert_eq!(store.get(FONT_SIZE, &terminal), Some(&json!(12)));
    assert_eq!(
        store.check(FONT_SIZE, &json!("large")),
        Err(SettingsError::TypeMismatch {
            expected: SettingType::Number
        })
    );
}

#[test]
fn file_cleanup_settings_default_on_and_read_any_other_text_as_all() {
    let mut store = SettingsStore::default();
    let context = rust_editor();
    assert_eq!(
        store.get(FILES_TRIM_TRAILING_WHITESPACE, &context),
        Some(&json!("all"))
    );
    assert_eq!(
        store.get(FILES_INSERT_FINAL_NEWLINE, &context),
        Some(&json!(true))
    );
    store
        .set(
            FILES_TRIM_TRAILING_WHITESPACE,
            json!("off"),
            scope(Some("markdown"), None),
        )
        .unwrap();
    assert_eq!(
        store.get(
            FILES_TRIM_TRAILING_WHITESPACE,
            &scope(Some("markdown"), Some("editor"))
        ),
        Some(&json!("off"))
    );
    assert_eq!(
        store.get(FILES_TRIM_TRAILING_WHITESPACE, &context),
        Some(&json!("all"))
    );
}

#[test]
fn file_ignore_settings_default_to_the_shared_policy() {
    let store = SettingsStore::default();
    let context = rust_editor();
    assert_eq!(
        store.get(FILES_HIDDEN, &context),
        Some(&json!([
            ".git",
            ".hg",
            ".svn",
            ".jj",
            "CVS",
            ".DS_Store",
            "Thumbs.db"
        ]))
    );
    assert_eq!(store.get(FILES_IGNORED, &context), Some(&json!([])));
    assert_eq!(
        store.get(FILES_RESPECT_GITIGNORE, &context),
        Some(&json!(true))
    );
}

#[test]
fn doubles_that_hold_integers_become_integers() {
    let restored = restore_integers(json!({"a": [1.0, 2.5, "x"], "b": 9.0e18, "c": null}));
    assert_eq!(restored["a"][0].as_i64(), Some(1));
    assert!(restored["a"][1].as_i64().is_none());
    assert!(restored["b"].as_i64().is_none());
    assert!(restored["c"].is_null());
}

#[test]
fn a_scope_names_its_facts() {
    assert_eq!(SettingsScope::default().describe(), "every context");
    assert_eq!(rust_editor().describe(), "language rust, view editor");
}

#[test]
fn a_batch_of_settings_writes_every_value_or_none() {
    let mut store = SettingsStore::default();
    store
        .define(extension(1), definition(SettingType::Integer, json!(4)))
        .unwrap();
    let scope = rust_editor();
    let rejected = store.set_all(
        vec![
            ("other.key".into(), json!(true)),
            (KEY.into(), json!("wide")),
        ],
        &scope,
    );
    assert!(matches!(
        rejected,
        Err((key, SettingsError::TypeMismatch { .. })) if key == KEY
    ));
    assert_eq!(store.get("other.key", &scope), None);
    let written = store.set_all(vec![(KEY.into(), json!(8))], &scope).unwrap();
    assert_eq!(written, [KEY]);
    assert_eq!(store.get(KEY, &scope), Some(&json!(8)));
}

#[test]
fn every_native_key_is_declared_for_extensions() {
    let mut native = native_definitions()
        .into_iter()
        .map(|definition| definition.key)
        .collect::<Vec<_>>();
    native.sort_unstable();
    let mut declared = include_str!("../../../knot-host/src/js/knot.d.ts")
        .split("export interface Settings {")
        .nth(1)
        .unwrap()
        .split("\n  }")
        .next()
        .unwrap()
        .lines()
        .filter_map(|line| line.trim().strip_prefix('"'))
        .filter_map(|line| line.split('"').next())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    declared.sort_unstable();
    assert_eq!(
        declared, native,
        "declare every native setting in knot.d.ts Settings"
    );
}
