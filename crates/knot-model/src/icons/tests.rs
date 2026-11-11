use super::*;

fn owner(lifecycle: u64) -> IconOwner {
    IconOwner {
        extension: ExtensionId::new(1),
        lifecycle: ExtensionLifecycleId::new(lifecycle),
    }
}

fn svg(text: &str) -> IconDefinition {
    IconDefinition {
        source: IconSource::Svg(text.into()),
        color: IconColor::Surface,
    }
}

fn pack(icons: &[(&str, &str)]) -> BTreeMap<String, IconDefinition> {
    icons
        .iter()
        .map(|(icon, text)| ((*icon).to_owned(), svg(text)))
        .collect()
}

#[test]
fn native_names_resolve_to_lucide_and_unknown_names_to_the_fallback() {
    let registry = IconRegistry::default();
    assert_eq!(
        registry.icon("folder.open"),
        IconDefinition::lucide("folder-open")
    );
    assert_eq!(registry.icon("no.such"), registry.icon(FALLBACK));
    assert_eq!(registry.lookup("no.such"), None);
}

#[test]
fn levels_win_in_order_and_removal_reveals_the_previous_level() {
    let mut registry = IconRegistry::default();
    registry.define(owner(1), "x.icon", svg("default")).unwrap();
    let pack_id = registry.register_pack(owner(2), "Mine", pack(&[("x.icon", "pack")]));
    assert_eq!(registry.icon("x.icon"), svg("default"));
    registry.select_pack("Mine");
    assert_eq!(registry.icon("x.icon"), svg("pack"));
    let first = registry.register_override(owner(3), "x.icon", svg("first"));
    let second = registry.register_override(owner(3), "x.icon", svg("second"));
    assert_eq!(registry.icon("x.icon"), svg("second"));
    assert!(registry.unregister_override(owner(3), second));
    assert_eq!(registry.icon("x.icon"), svg("first"));
    assert!(registry.unregister_override(owner(3), first));
    assert_eq!(registry.icon("x.icon"), svg("pack"));
    assert!(registry.unregister_pack(owner(2), pack_id));
    assert_eq!(registry.icon("x.icon"), svg("default"));
}

#[test]
fn a_pack_falls_back_per_name_and_only_the_active_pack_applies() {
    let mut registry = IconRegistry::default();
    registry.register_pack(owner(1), "A", pack(&[("file", "a-file")]));
    registry.register_pack(owner(1), "B", pack(&[("file", "b-file")]));
    assert_eq!(registry.icon("file"), IconDefinition::lucide("file"));
    assert!(registry.select_pack("A"));
    assert!(!registry.select_pack("A"));
    assert_eq!(registry.icon("file"), svg("a-file"));
    assert_eq!(registry.icon("folder"), IconDefinition::lucide("folder"));
    registry.select_pack("B");
    assert_eq!(registry.icon("file"), svg("b-file"));
    registry.select_pack("");
    assert_eq!(registry.icon("file"), IconDefinition::lucide("file"));
    assert_eq!(registry.pack_names(), ["A", "B"]);
}

#[test]
fn the_latest_registration_of_a_pack_name_defines_it() {
    let mut registry = IconRegistry::default();
    registry.register_pack(owner(1), "A", pack(&[("file", "old")]));
    let later = registry.register_pack(owner(2), "A", pack(&[("file", "new")]));
    registry.select_pack("A");
    assert_eq!(registry.icon("file"), svg("new"));
    registry.unregister_pack(owner(2), later);
    assert_eq!(registry.icon("file"), svg("old"));
}

#[test]
fn defining_a_name_with_a_default_is_a_duplicate() {
    let mut registry = IconRegistry::default();
    assert_eq!(
        registry.define(owner(1), "file", svg("x")),
        Err(DuplicateIcon)
    );
    registry.define(owner(1), "x", svg("x")).unwrap();
    assert_eq!(registry.define(owner(2), "x", svg("y")), Err(DuplicateIcon));
}

#[test]
fn removing_a_lifecycle_removes_its_registrations_only() {
    let mut registry = IconRegistry::default();
    registry.define(owner(1), "x", svg("x")).unwrap();
    registry.register_override(owner(1), "file", svg("o"));
    registry.register_pack(owner(1), "A", pack(&[]));
    registry.register_override(owner(2), "folder", svg("kept"));
    registry.remove_lifecycle(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    assert_eq!(registry.lookup("x"), None);
    assert_eq!(registry.icon("file"), IconDefinition::lucide("file"));
    assert!(registry.pack_names().is_empty());
    assert_eq!(registry.icon("folder"), svg("kept"));
}

#[test]
fn file_icons_try_the_language_then_the_generic_file() {
    let mut registry = IconRegistry::default();
    assert_eq!(
        registry.file_icon(None, Some("rust"), false),
        IconDefinition::lucide("file")
    );
    registry.define(owner(1), "file.rust", svg("crab")).unwrap();
    assert_eq!(registry.file_icon(None, Some("rust"), false), svg("crab"));
    assert_eq!(
        registry.file_icon(None, Some("go"), false),
        IconDefinition::lucide("file")
    );
    assert_eq!(
        registry.file_icon(None, None, false),
        IconDefinition::lucide("file")
    );
    registry.register_override(owner(1), "file", svg("generic"));
    assert_eq!(registry.file_icon(None, Some("go"), false), svg("generic"));
}

#[test]
fn folders_use_the_open_form_only_when_expanded() {
    let mut registry = IconRegistry::default();
    assert_eq!(
        registry.folder_icon(None, false),
        IconDefinition::lucide("folder")
    );
    assert_eq!(
        registry.folder_icon(None, true),
        IconDefinition::lucide("folder-open")
    );
    registry.register_override(owner(1), "folder", svg("closed"));
    assert_eq!(registry.folder_icon(None, false), svg("closed"));
    assert_eq!(
        registry.folder_icon(None, true),
        IconDefinition::lucide("folder-open")
    );
}

fn names(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(pattern, icon)| ((*pattern).to_owned(), (*icon).to_owned()))
        .collect()
}

#[test]
fn associations_pick_names_by_file_name_then_longest_extension_then_language() {
    let mut registry = IconRegistry::default();
    for name in ["file.cargo", "file.ts", "file.dts", "file.rust"] {
        registry.define(owner(1), name, svg(name)).unwrap();
    }
    registry.associate(
        owner(1),
        IconAssociation {
            files: names(&[("Cargo.toml", "file.cargo")]),
            extensions: names(&[("ts", "file.ts"), ("D.TS", "file.dts")]),
            ..Default::default()
        },
    );
    let icon = |file: &str, language| registry.file_icon(Some(file), language, false);
    assert_eq!(icon("Cargo.toml", Some("toml")), svg("file.cargo"));
    assert_eq!(icon("a.ts", None), svg("file.ts"));
    assert_eq!(icon("a.d.ts", None), svg("file.dts"));
    assert_eq!(icon("a.TS", None), svg("file.ts"));
    assert_eq!(icon("main.rs", Some("rust")), svg("file.rust"));
    assert_eq!(icon("notes", None), IconDefinition::lucide("file"));

    // The latest claim of a pattern wins, and disposal reveals the previous.
    let second = registry.associate(
        owner(2),
        IconAssociation {
            extensions: names(&[("ts", "file.rust")]),
            ..Default::default()
        },
    );
    assert_eq!(
        registry.file_icon(Some("a.ts"), None, false),
        svg("file.rust")
    );
    assert!(!registry.unassociate(owner(1), second));
    assert!(registry.unassociate(owner(2), second));
    assert_eq!(
        registry.file_icon(Some("a.ts"), None, false),
        svg("file.ts")
    );
    registry.remove_lifecycle(owner(1).extension, owner(1).lifecycle);
    assert_eq!(
        registry.file_icon(Some("a.ts"), None, false),
        IconDefinition::lucide("file")
    );
}

#[test]
fn an_associated_name_the_active_pack_lacks_falls_through() {
    let mut registry = IconRegistry::default();
    registry.define(owner(1), "file.rust", svg("crab")).unwrap();
    registry.associate(
        owner(1),
        IconAssociation {
            extensions: names(&[("rs", "file.material-rust")]),
            ..Default::default()
        },
    );
    assert_eq!(
        registry.file_icon(Some("a.rs"), Some("rust"), false),
        svg("crab")
    );
    registry.register_pack(
        owner(1),
        "Material",
        pack(&[("file.material-rust", "gear")]),
    );
    registry.select_pack("Material");
    assert_eq!(
        registry.file_icon(Some("a.rs"), Some("rust"), false),
        svg("gear")
    );
}

#[test]
fn named_folders_prefer_their_open_form_when_expanded() {
    let mut registry = IconRegistry::default();
    registry.define(owner(1), "folder.src", svg("src")).unwrap();
    registry
        .define(owner(1), "folder.src.open", svg("src open"))
        .unwrap();
    registry.define(owner(1), "folder.git", svg("git")).unwrap();
    registry.associate(
        owner(1),
        IconAssociation {
            folders: names(&[
                ("src", "folder.src"),
                (".git", "folder.git"),
                ("x", "folder.none"),
            ]),
            ..Default::default()
        },
    );
    let icon = |name, open| registry.folder_icon(Some(name), open);
    assert_eq!(icon("src", false), svg("src"));
    assert_eq!(icon("src", true), svg("src open"));
    assert_eq!(icon(".git", false), svg("git"));
    assert_eq!(icon(".git", true), svg("git"));
    assert_eq!(icon("x", true), IconDefinition::lucide("folder-open"));
    assert_eq!(icon("other", false), IconDefinition::lucide("folder"));
}

fn spec(json: &str) -> Result<IconDefinition, &'static str> {
    serde_json::from_str::<IconSpec>(json).unwrap().definition()
}

#[test]
fn specs_name_one_source_and_a_compatible_color() {
    let svg = r#""<svg viewBox=\"0 0 1 1\"></svg>""#;
    assert_eq!(
        spec(r#"{"lucide":"folder-open"}"#),
        Ok(IconDefinition::lucide("folder-open"))
    );
    assert_eq!(
        spec(&format!(r#"{{"svg":{svg},"color":"original"}}"#))
            .unwrap()
            .color,
        IconColor::Original
    );
    assert_eq!(
        spec(&format!(r#"{{"svg":{svg},"color":"icon.rust"}}"#))
            .unwrap()
            .color,
        IconColor::Key("icon.rust".into())
    );
    for invalid in [
        r#"{}"#,
        r#"{"svg":"<svg></svg>","lucide":"file"}"#,
        r#"{"svg":"just text"}"#,
        r#"{"svg":"<svg>"}"#,
        r#"{"lucide":"Not A Glyph"}"#,
        r#"{"lucide":"file","color":"original"}"#,
        r#"{"lucide":"file","color":""}"#,
    ] {
        assert!(spec(invalid).is_err(), "{invalid}");
    }
    assert!(serde_json::from_str::<IconSpec>(r#"{"lucide":"file","size":2}"#).is_err());
}

#[test]
fn expanded_forms_try_open_before_each_candidate() {
    let mut registry = IconRegistry::default();
    registry.associate(
        owner(1),
        IconAssociation {
            files: names(&[("Makefile", "file.make")]),
            ..Default::default()
        },
    );
    registry.define(owner(1), "file.make", svg("make")).unwrap();
    registry
        .define(owner(1), "file.make.open", svg("make open"))
        .unwrap();
    registry.define(owner(1), "tool", svg("tool")).unwrap();
    registry
        .define(owner(1), "tool.open", svg("tool open"))
        .unwrap();
    assert_eq!(
        registry.file_icon(Some("Makefile"), None, false),
        svg("make")
    );
    assert_eq!(
        registry.file_icon(Some("Makefile"), None, true),
        svg("make open")
    );
    assert_eq!(
        registry.file_icon(Some("other"), None, true),
        IconDefinition::lucide("file")
    );
    assert_eq!(registry.first_defined(&["tool"], true), svg("tool open"));
    assert_eq!(registry.first_defined(&["tool"], false), svg("tool"));
    assert_eq!(
        registry.first_defined(&["nothing", "tool"], true),
        svg("tool open")
    );
}
