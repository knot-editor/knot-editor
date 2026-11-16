use super::*;

fn owner(lifecycle: u64) -> MenuOwner {
    MenuOwner {
        extension: ExtensionId::new(1),
        lifecycle: ExtensionLifecycleId::new(lifecycle),
    }
}

fn item(command: &str) -> MenuItem {
    MenuItem {
        command: command.into(),
        title: None,
        arguments: CommandArgumentValue::Null,
        group: None,
        order: 0,
        view: None,
        context: None,
    }
}

fn titles(groups: &[Vec<MenuEntry>]) -> Vec<Vec<&str>> {
    groups
        .iter()
        .map(|group| group.iter().map(|entry| entry.title.as_str()).collect())
        .collect()
}

fn show(registry: &MenuRegistry, view: Option<&str>, contexts: &[&str]) -> Vec<Vec<MenuEntry>> {
    registry.groups(MenuLocation::Tab, MenuScope { view, contexts }, |command| {
        command.to_uppercase()
    })
}

#[test]
fn locations_are_a_fixed_set() {
    assert_eq!(MenuLocation::parse("tab"), Some(MenuLocation::Tab));
    assert_eq!(
        MenuLocation::parse("menubar.go"),
        Some(MenuLocation::MenubarGo)
    );
    assert_eq!(MenuLocation::parse("menubar.help"), None);
    assert_eq!(MenuLocation::parse("Tab"), None);
}

#[test]
fn items_sort_by_group_then_order_then_title_whatever_the_registration_order() {
    let mut registry = MenuRegistry::default();
    let add = |registry: &mut MenuRegistry, command: &str, group: Option<&str>, order: i64| {
        registry
            .add(
                owner(1),
                MenuLocation::Tab,
                MenuItem {
                    group: group.map(Into::into),
                    order,
                    ..item(command)
                },
            )
            .unwrap();
    };
    add(&mut registry, "ungrouped", None, 0);
    add(&mut registry, "split", Some("2_split"), 0);
    add(&mut registry, "close-b", Some("1_close"), 1);
    add(&mut registry, "close-z", Some("1_close"), 0);
    add(&mut registry, "close-a", Some("1_close"), 0);
    assert_eq!(
        titles(&show(&registry, None, &[])),
        [
            vec!["CLOSE-A", "CLOSE-Z", "CLOSE-B"],
            vec!["SPLIT"],
            vec!["UNGROUPED"],
        ]
    );
}

#[test]
fn a_title_overrides_the_commands_and_arguments_travel() {
    let mut registry = MenuRegistry::default();
    registry
        .add(
            owner(1),
            MenuLocation::Tab,
            MenuItem {
                title: Some("Mine".into()),
                arguments: CommandArgumentValue::Boolean(true),
                ..item("a.b")
            },
        )
        .unwrap();
    let groups = show(&registry, None, &[]);
    assert_eq!(groups[0][0].title, "Mine");
    assert_eq!(groups[0][0].command, "a.b");
    assert_eq!(groups[0][0].arguments, CommandArgumentValue::Boolean(true));
}

#[test]
fn scoped_items_show_only_on_matching_surfaces() {
    let mut registry = MenuRegistry::default();
    let scoped = |command: &str, view: Option<&str>, context: Option<&str>| MenuItem {
        view: view.map(Into::into),
        context: context.map(Into::into),
        ..item(command)
    };
    for scoped in [
        scoped("all", None, None),
        scoped("editor", Some("editor"), None),
        scoped("terminal", Some("terminal"), None),
        scoped("selection", Some("editor"), Some("selection")),
    ] {
        registry.add(owner(1), MenuLocation::Tab, scoped).unwrap();
    }
    let shown = |view, contexts: &[&str]| {
        let groups = show(&registry, view, contexts);
        let mut names = groups
            .into_iter()
            .flatten()
            .map(|entry| entry.command)
            .collect::<Vec<_>>();
        names.sort();
        names
    };
    assert_eq!(shown(Some("editor"), &[]), ["all", "editor"]);
    assert_eq!(
        shown(Some("editor"), &["selection"]),
        ["all", "editor", "selection"]
    );
    assert_eq!(shown(Some("terminal"), &["selection"]), ["all", "terminal"]);
    assert_eq!(shown(None, &[]), ["all"]);
}

#[test]
fn locations_keep_their_own_items() {
    let mut registry = MenuRegistry::default();
    registry
        .add(owner(1), MenuLocation::Content, item("content"))
        .unwrap();
    assert!(show(&registry, None, &[]).is_empty());
}

#[test]
fn removing_an_item_or_a_lifecycle_removes_its_items() {
    let mut registry = MenuRegistry::default();
    let first = registry
        .add(owner(1), MenuLocation::Tab, item("one"))
        .unwrap();
    registry
        .add(owner(2), MenuLocation::Tab, item("two"))
        .unwrap();
    registry
        .add(owner(2), MenuLocation::Tab, item("three"))
        .unwrap();
    assert!(!registry.remove(owner(2), first));
    assert!(registry.remove(owner(1), first));
    assert!(!registry.remove(owner(1), first));
    registry.remove_lifecycle(ExtensionId::new(1), ExtensionLifecycleId::new(2));
    assert!(show(&registry, None, &[]).is_empty());
}

#[test]
fn invalid_items_are_rejected() {
    let mut registry = MenuRegistry::default();
    let add = |registry: &mut MenuRegistry, location, item| registry.add(owner(1), location, item);
    assert_eq!(
        add(&mut registry, MenuLocation::Tab, item("")),
        Err(MenuError::Command)
    );
    for location in [MenuLocation::MenubarFile, MenuLocation::MenubarGo] {
        assert_eq!(
            add(
                &mut registry,
                location,
                MenuItem {
                    view: Some("editor".into()),
                    ..item("a")
                }
            ),
            Err(MenuError::MenubarScope)
        );
        assert_eq!(
            add(
                &mut registry,
                location,
                MenuItem {
                    view: Some("editor".into()),
                    context: Some("selection".into()),
                    ..item("a")
                }
            ),
            Err(MenuError::MenubarScope)
        );
    }
    assert_eq!(
        add(
            &mut registry,
            MenuLocation::Content,
            MenuItem {
                context: Some("selection".into()),
                ..item("a")
            }
        ),
        Err(MenuError::Context)
    );
    assert!(add(&mut registry, MenuLocation::MenubarEdit, item("a")).is_ok());
}
