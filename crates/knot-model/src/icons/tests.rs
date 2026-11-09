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
        registry.file_icon(Some("rust")),
        IconDefinition::lucide("file")
    );
    registry.define(owner(1), "file.rust", svg("crab")).unwrap();
    assert_eq!(registry.file_icon(Some("rust")), svg("crab"));
    assert_eq!(
        registry.file_icon(Some("go")),
        IconDefinition::lucide("file")
    );
    assert_eq!(registry.file_icon(None), IconDefinition::lucide("file"));
    registry.register_override(owner(1), "file", svg("generic"));
    assert_eq!(registry.file_icon(Some("go")), svg("generic"));
}

#[test]
fn folders_use_the_open_form_only_when_expanded() {
    let mut registry = IconRegistry::default();
    assert_eq!(
        registry.folder_icon(false),
        IconDefinition::lucide("folder")
    );
    assert_eq!(
        registry.folder_icon(true),
        IconDefinition::lucide("folder-open")
    );
    registry.register_override(owner(1), "folder", svg("closed"));
    assert_eq!(registry.folder_icon(false), svg("closed"));
    assert_eq!(
        registry.folder_icon(true),
        IconDefinition::lucide("folder-open")
    );
}
