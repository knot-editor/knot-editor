use super::*;

fn owner(lifecycle: u64) -> StatusOwner {
    StatusOwner {
        extension: ExtensionId::new(1),
        lifecycle: ExtensionLifecycleId::new(lifecycle),
    }
}

fn item(text: &str, side: StatusSide, priority: i64) -> StatusItem {
    StatusItem {
        text: text.into(),
        icon: None,
        tooltip: None,
        command: None,
        color: None,
        side,
        priority,
    }
}

fn texts(items: &[BarItem]) -> Vec<&str> {
    items.iter().map(|bar| bar.item.text.as_str()).collect()
}

#[test]
fn items_sit_on_their_side_with_higher_priority_further_left() {
    let mut registry = StatusRegistry::default();
    registry.register(owner(1), "a".into(), item("a", StatusSide::Left, 0));
    registry.register(owner(1), "b".into(), item("b", StatusSide::Left, 5));
    registry.register(owner(1), "c".into(), item("c", StatusSide::Left, 0));
    registry.register(owner(1), "d".into(), item("d", StatusSide::Right, 1));
    registry.register(owner(1), "e".into(), item("e", StatusSide::Right, 2));
    let items = registry.items("w", &[]);
    assert_eq!(texts(&items.left), ["b", "a", "c"]);
    assert_eq!(texts(&items.right), ["e", "d"]);
}

#[test]
fn the_latest_registration_of_an_id_wins_and_removal_reveals_the_previous() {
    let mut registry = StatusRegistry::default();
    registry.register(
        owner(1),
        "position".into(),
        item("first", StatusSide::Left, 0),
    );
    let second = registry.register(
        owner(2),
        "position".into(),
        item("second", StatusSide::Left, 0),
    );
    assert_eq!(texts(&registry.items("w", &[]).left), ["second"]);
    // A lifecycle cannot remove another's registration.
    assert!(!registry.unregister(owner(1), second));
    assert!(registry.unregister(owner(2), second));
    assert_eq!(texts(&registry.items("w", &[]).left), ["first"]);
    registry.remove_lifecycle(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    assert_eq!(registry.items("w", &[]), BarItems::default());
}

#[test]
fn a_window_override_replaces_the_value_for_that_window_only() {
    let mut registry = StatusRegistry::default();
    let id = registry.register(
        owner(1),
        "position".into(),
        item("Ln 1", StatusSide::Right, 0),
    );
    assert!(registry.update(
        owner(1),
        id,
        Some("w1".into()),
        Some(item("Ln 9", StatusSide::Right, 0)),
    ));
    assert_eq!(texts(&registry.items("w1", &[]).right), ["Ln 9"]);
    assert_eq!(texts(&registry.items("w2", &[]).right), ["Ln 1"]);
    // The application-wide value changes beneath the override.
    assert!(registry.update(owner(1), id, None, Some(item("Ln 2", StatusSide::Right, 0))));
    assert_eq!(texts(&registry.items("w1", &[]).right), ["Ln 9"]);
    assert_eq!(texts(&registry.items("w2", &[]).right), ["Ln 2"]);
    // Clearing the override returns the window to the application value.
    assert!(registry.update(owner(1), id, Some("w1".into()), None));
    assert_eq!(texts(&registry.items("w1", &[]).right), ["Ln 2"]);
    // An override may move an item to the other side.
    registry.update(
        owner(1),
        id,
        Some("w1".into()),
        Some(item("Ln 9", StatusSide::Left, 0)),
    );
    assert_eq!(texts(&registry.items("w1", &[]).left), ["Ln 9"]);
    assert!(registry.items("w1", &[]).right.is_empty());
}

#[test]
fn updates_need_a_live_registration_of_the_owner_and_a_value() {
    let mut registry = StatusRegistry::default();
    let id = registry.register(owner(1), "a".into(), item("a", StatusSide::Left, 0));
    let value = || Some(item("b", StatusSide::Left, 0));
    assert!(!registry.update(owner(2), id, None, value()));
    assert!(!registry.update(owner(1), id + 1, None, value()));
    assert!(!registry.update(owner(1), id, None, None));
    registry.unregister(owner(1), id);
    assert!(!registry.update(owner(1), id, None, value()));
}

#[test]
fn items_without_text_are_not_shown() {
    let mut registry = StatusRegistry::default();
    let id = registry.register(owner(1), "a".into(), item("", StatusSide::Left, 0));
    assert_eq!(registry.items("w", &[]), BarItems::default());
    registry.update(
        owner(1),
        id,
        Some("w".into()),
        Some(item("a", StatusSide::Left, 0)),
    );
    assert_eq!(texts(&registry.items("w", &[]).left), ["a"]);
    assert_eq!(registry.items("other", &[]), BarItems::default());
}

#[test]
fn hidden_ids_are_skipped_but_keep_their_registration() {
    let mut registry = StatusRegistry::default();
    registry.register(owner(1), "a".into(), item("a", StatusSide::Left, 0));
    registry.register(owner(1), "b".into(), item("b", StatusSide::Left, 0));
    assert_eq!(texts(&registry.items("w", &["a".into()]).left), ["b"]);
    assert_eq!(texts(&registry.items("w", &[]).left), ["a", "b"]);
}

#[test]
fn items_decode_from_javascript_values() {
    let item: StatusItem = serde_json::from_value(serde_json::json!({
        "text": "Ln 1, Col 1",
        "icon": "status.position",
        "tooltip": null,
        "command": { "name": "go-to-line.go", "arguments": null },
        "color": "warning",
        "side": "right",
        "priority": 3,
    }))
    .unwrap();
    assert_eq!(item.side, StatusSide::Right);
    assert_eq!(item.icon, Some("status.position".into()));
    assert_eq!(item.command.unwrap().name, "go-to-line.go");
    assert!(serde_json::from_value::<StatusItem>(serde_json::json!({ "text": 1 })).is_err());
    assert!(
        serde_json::from_value::<StatusItem>(serde_json::json!({ "text": "x", "bogus": "y" }))
            .is_err()
    );
}

#[test]
fn holds_tells_whether_an_update_would_change_nothing() {
    let mut registry = StatusRegistry::default();
    let id = registry.register(owner(1), "mode".into(), item("a", StatusSide::Left, 0));
    let a = item("a", StatusSide::Left, 0);
    let b = item("b", StatusSide::Left, 0);
    assert!(registry.holds(owner(1), id, None, Some(&a)));
    assert!(!registry.holds(owner(1), id, None, Some(&b)));
    assert!(!registry.holds(owner(2), id, None, Some(&a)));
    // No window value yet: clearing it changes nothing, setting one does.
    assert!(registry.holds(owner(1), id, Some("w"), None));
    assert!(!registry.holds(owner(1), id, Some("w"), Some(&a)));
    registry.update(owner(1), id, Some("w".into()), Some(b.clone()));
    assert!(registry.holds(owner(1), id, Some("w"), Some(&b)));
    assert!(!registry.holds(owner(1), id, Some("w"), None));
}
