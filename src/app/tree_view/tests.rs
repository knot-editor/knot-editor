use std::cell::RefCell;
use std::rc::Rc;

use gpui::{AppContext, Subscription, TestAppContext};

use super::{
    EditSession, EditTarget, TreeGesture, TreeProviderIdentity, TreeView, TreeViewEvent,
    TreeViewRegistrationError,
};
use knot_protocol::{
    CommandArgumentValue, ExtensionId, ExtensionLifecycleId, TreeAction, TreeChildrenResponse,
    TreeCollapsibleState, TreeItem, TreeItemKind, TreeProviderRegistrationId,
};

type CommandLog = Rc<RefCell<Vec<(String, CommandArgumentValue)>>>;

struct EventSubscriber {
    _events: CommandLog,
    _subscription: Subscription,
}

fn subscribe_commands(
    cx: &mut TestAppContext,
    tree: &gpui::Entity<TreeView>,
) -> (CommandLog, gpui::Entity<EventSubscriber>) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let log = events.clone();
    let sink = events.clone();
    let tree = tree.clone();
    let subscriber = cx.update(move |cx| {
        cx.new(move |cx| {
            let subscription = cx.subscribe(&tree, move |_, _, event: &TreeViewEvent, _| {
                if let TreeViewEvent::InvokeCommand {
                    command, arguments, ..
                } = event
                {
                    sink.borrow_mut().push((command.clone(), arguments.clone()));
                }
            });
            EventSubscriber {
                _events: log,
                _subscription: subscription,
            }
        })
    });
    (events, subscriber)
}

fn item(id: &str, label: &str, kind: TreeItemKind, actions: Vec<TreeAction>) -> TreeItem {
    TreeItem {
        id: id.into(),
        label: label.into(),
        description: None,
        detail: None,
        icon: None,
        collapsible_state: TreeCollapsibleState::None,
        command: Some("example.activate".into()),
        arguments: CommandArgumentValue::Null,
        kind,
        muted: false,
        status_text: Some("M".into()),
        badge: Some("warning".to_owned()),
        actions,
    }
}

#[gpui::test]
fn provider_registration_enforces_view_and_ownership(cx: &mut TestAppContext) {
    let registration = TreeProviderRegistrationId::new(3);
    let owner = TreeProviderIdentity {
        extension: ExtensionId::new(7),
        lifecycle: ExtensionLifecycleId::new(2),
        registration,
    };
    let tree = cx.new(|cx| TreeView::new("outline", "1", cx));

    tree.update(cx, |tree, cx| {
        assert_eq!(
            tree.register_provider("wrong-view", owner, Vec::new(), cx),
            Err(TreeViewRegistrationError::WrongView)
        );
        tree.register_provider("outline", owner, Vec::new(), cx)
            .unwrap();
        assert!(tree.owns_provider(registration, owner.extension, owner.lifecycle));
        assert!(!tree.owns_provider(registration, owner.extension, ExtensionLifecycleId::new(1)));
        assert_eq!(
            tree.register_provider(
                "outline",
                TreeProviderIdentity {
                    extension: ExtensionId::new(8),
                    lifecycle: ExtensionLifecycleId::new(1),
                    registration: TreeProviderRegistrationId::new(4),
                },
                Vec::new(),
                cx,
            ),
            Err(TreeViewRegistrationError::ProviderInUse)
        );
        assert_eq!(
            tree.unregister_provider(TreeProviderRegistrationId::new(4), cx),
            Err(TreeViewRegistrationError::ProviderNotFound)
        );
        assert!(tree.owns_provider(registration, owner.extension, owner.lifecycle));
    });
}

#[gpui::test]
fn stale_and_disposed_tree_responses_are_ignored(cx: &mut TestAppContext) {
    let registration = TreeProviderRegistrationId::new(3);
    let tree = cx.new(|cx| TreeView::new("outline", "1", cx));
    let stale_generation = tree.update(cx, |tree, cx| {
        tree.register_provider(
            "outline",
            TreeProviderIdentity {
                extension: ExtensionId::new(7),
                lifecycle: ExtensionLifecycleId::new(2),
                registration,
            },
            Vec::new(),
            cx,
        )
        .unwrap();
        let stale_generation = tree.children[&None].generation;
        tree.invalidate(registration, None, cx).unwrap();
        stale_generation
    });

    let view = cx.read(|cx| tree.read(cx).instance_id());
    tree.update(cx, |tree, cx| {
        assert!(!tree.apply_response(
            TreeChildrenResponse {
                registration,
                window: "1".into(),
                view,
                parent_id: None,
                generation: stale_generation,
                result: Ok(Vec::new()),
            },
            cx,
        ));
        let current_generation = tree.children[&None].generation;
        assert!(tree.apply_response(
            TreeChildrenResponse {
                registration,
                window: "1".into(),
                view,
                parent_id: None,
                generation: current_generation,
                result: Ok(vec![TreeItem {
                    id: "root".into(),
                    label: "Root".into(),
                    description: None,
                    detail: None,
                    icon: None,
                    collapsible_state: TreeCollapsibleState::None,
                    command: None,
                    arguments: knot_protocol::CommandArgumentValue::Null,
                    kind: knot_protocol::TreeItemKind::Item,
                    muted: false,
                    status_text: None,
                    badge: None,
                    actions: Vec::new(),
                }]),
            },
            cx,
        ));
        assert_eq!(tree.children[&None].items[0].id, "root");
        tree.unregister_provider(registration, cx).unwrap();
        assert!(!tree.apply_response(
            TreeChildrenResponse {
                registration,
                window: "1".into(),
                view,
                parent_id: None,
                generation: current_generation,
                result: Ok(Vec::new()),
            },
            cx,
        ));
        tree.register_provider(
            "outline",
            TreeProviderIdentity {
                extension: ExtensionId::new(7),
                lifecycle: ExtensionLifecycleId::new(2),
                registration,
            },
            Vec::new(),
            cx,
        )
        .unwrap();
        let lifecycle_generation = tree.children[&None].generation;
        tree.remove_lifecycle(ExtensionId::new(7), ExtensionLifecycleId::new(2), cx);
        assert!(!tree.apply_response(
            TreeChildrenResponse {
                registration,
                window: "1".into(),
                view,
                parent_id: None,
                generation: lifecycle_generation,
                result: Ok(Vec::new()),
            },
            cx,
        ));
    });
}

fn owner(registration: TreeProviderRegistrationId) -> TreeProviderIdentity {
    TreeProviderIdentity {
        extension: ExtensionId::new(7),
        lifecycle: ExtensionLifecycleId::new(2),
        registration,
    }
}

fn response(
    registration: TreeProviderRegistrationId,
    view: knot_protocol::ViewId,
    parent_id: Option<String>,
    generation: u64,
    items: Vec<TreeItem>,
) -> TreeChildrenResponse {
    TreeChildrenResponse {
        registration,
        window: "1".into(),
        view,
        parent_id,
        generation,
        result: Ok(items),
    }
}

fn tree(cx: &mut TestAppContext) -> gpui::Entity<TreeView> {
    cx.new(|cx| TreeView::new("example.git", "1", cx))
}

#[gpui::test]
fn groups_fetch_children_and_are_not_activated(cx: &mut TestAppContext) {
    let registration = TreeProviderRegistrationId::new(5);
    let tree = tree(cx);
    let (events, _subscriber) = subscribe_commands(cx, &tree);
    let generation = tree.update(cx, |tree, cx| {
        tree.register_provider("example.git", owner(registration), Vec::new(), cx)
            .unwrap();
        tree.root_generation()
    });
    let view = cx.read(|cx| tree.read(cx).instance_id());
    tree.update(cx, |tree, cx| {
        assert!(tree.apply_response(
            response(
                registration,
                view,
                None,
                generation,
                vec![
                    item("staged", "Staged", TreeItemKind::Group, Vec::new()),
                    item("notes", "notes.txt", TreeItemKind::Item, Vec::new()),
                ],
            ),
            cx,
        ));
    });

    let group_generation =
        cx.read(|cx| tree.read(cx).children[&Some("staged".to_owned())].generation);
    tree.update(cx, |tree, cx| {
        assert!(tree.apply_response(
            response(
                registration,
                view,
                Some("staged".into()),
                group_generation,
                vec![item("file", "file.rs", TreeItemKind::Item, Vec::new())],
            ),
            cx,
        ));
    });
    assert_eq!(
        cx.read(|cx| tree.read(cx).visible_labels()),
        ["Staged", "file.rs", "notes.txt"]
    );

    tree.update(cx, |tree, cx| {
        tree.activate("staged", TreeGesture::Click, cx)
    });
    assert!(
        events.borrow().is_empty(),
        "a group header is not activatable"
    );

    tree.update(cx, |tree, cx| {
        tree.activate("notes", TreeGesture::Click, cx)
    });
    let events = events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "example.activate");
    assert_eq!(
        events[0].1,
        gesture_arguments("click", CommandArgumentValue::Null)
    );
}

fn gesture_arguments(gesture: &str, arguments: CommandArgumentValue) -> CommandArgumentValue {
    CommandArgumentValue::Object(
        [
            (
                "gesture".to_owned(),
                CommandArgumentValue::String(gesture.to_owned()),
            ),
            ("arguments".to_owned(), arguments),
        ]
        .into(),
    )
}

#[gpui::test]
fn a_row_command_receives_the_gesture_and_the_row_arguments(cx: &mut TestAppContext) {
    let registration = TreeProviderRegistrationId::new(8);
    let tree = tree(cx);
    let (events, _subscriber) = subscribe_commands(cx, &tree);
    let generation = tree.update(cx, |tree, cx| {
        tree.register_provider("example.git", owner(registration), Vec::new(), cx)
            .unwrap();
        tree.root_generation()
    });
    let view = cx.read(|cx| tree.read(cx).instance_id());
    let mut file = item("file", "file.rs", TreeItemKind::Item, Vec::new());
    file.arguments = CommandArgumentValue::String("payload".into());
    let mut folder = item("folder", "folder", TreeItemKind::Item, Vec::new());
    folder.collapsible_state = TreeCollapsibleState::Collapsed;
    tree.update(cx, |tree, cx| {
        assert!(tree.apply_response(
            response(registration, view, None, generation, vec![file, folder]),
            cx
        ));
    });
    for (gesture, name) in [
        (TreeGesture::Click, "click"),
        (TreeGesture::DoubleClick, "doubleClick"),
        (TreeGesture::MiddleClick, "middleClick"),
        (TreeGesture::RightClick, "rightClick"),
        (TreeGesture::Enter, "enter"),
    ] {
        tree.update(cx, |tree, cx| tree.activate("file", gesture, cx));
        let last = events.borrow().last().cloned().unwrap();
        assert_eq!(
            last.1,
            gesture_arguments(name, CommandArgumentValue::String("payload".into()))
        );
    }
    assert_eq!(events.borrow().len(), 5);

    // An expandable row toggles on click and Enter only.
    let expanded =
        |cx: &mut TestAppContext| cx.read(|cx| tree.read(cx).expanded.contains("folder"));
    for (gesture, toggled) in [
        (TreeGesture::DoubleClick, false),
        (TreeGesture::MiddleClick, false),
        (TreeGesture::RightClick, false),
        (TreeGesture::Click, true),
        (TreeGesture::Enter, false),
    ] {
        tree.update(cx, |tree, cx| tree.activate("folder", gesture, cx));
        assert_eq!(expanded(cx), toggled, "{gesture:?}");
    }
    assert_eq!(events.borrow().len(), 5, "folders run no command");
}

#[gpui::test]
fn actions_carry_arguments_and_enter_keyboard_focus(cx: &mut TestAppContext) {
    let registration = TreeProviderRegistrationId::new(6);
    let tree = tree(cx);
    let (events, _subscriber) = subscribe_commands(cx, &tree);
    let frame_action = TreeAction {
        label: "Refresh".into(),
        command: "example.refresh".into(),
        arguments: CommandArgumentValue::Null,
        icon: None,
        menu_only: false,
    };
    let generation = tree.update(cx, |tree, cx| {
        tree.register_provider("example.git", owner(registration), vec![frame_action], cx)
            .unwrap();
        tree.root_generation()
    });
    let view = cx.read(|cx| tree.read(cx).instance_id());
    let stage = TreeAction {
        label: "Stage".into(),
        command: "example.stage".into(),
        arguments: CommandArgumentValue::Object(
            [(
                "path".into(),
                CommandArgumentValue::String("notes.txt".into()),
            )]
            .into(),
        ),
        icon: Some("action.stage".into()),
        menu_only: false,
    };
    tree.update(cx, |tree, cx| {
        assert!(tree.apply_response(
            response(
                registration,
                view,
                None,
                generation,
                vec![item(
                    "notes",
                    "notes.txt",
                    TreeItemKind::Item,
                    vec![stage.clone()],
                )],
            ),
            cx,
        ));
        assert!(tree.select_item("notes", cx));
        tree.enter_action_focus(cx);
        assert_eq!(
            tree.action_focus_for_test(),
            Some((Some("notes".into()), 0))
        );
        tree.invoke_action(stage.clone(), cx);
    });
    let events = events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "example.stage");
    assert_eq!(events[0].1, stage.arguments);
}

#[gpui::test]
fn arrow_keys_move_selection_across_visible_rows(cx: &mut TestAppContext) {
    let registration = TreeProviderRegistrationId::new(9);
    let tree = tree(cx);
    let generation = tree.update(cx, |tree, cx| {
        tree.register_provider("example.git", owner(registration), Vec::new(), cx)
            .unwrap();
        tree.root_generation()
    });
    let view = cx.read(|cx| tree.read(cx).instance_id());
    tree.update(cx, |tree, cx| {
        assert!(tree.apply_response(
            response(
                registration,
                view,
                None,
                generation,
                vec![
                    item("a", "a.rs", TreeItemKind::Item, Vec::new()),
                    item("b", "b.rs", TreeItemKind::Item, Vec::new()),
                    item("c", "c.rs", TreeItemKind::Item, Vec::new()),
                ],
            ),
            cx,
        ));
    });
    let press = |key: &str, cx: &mut TestAppContext| {
        let keystroke = crate::app::keys::Keystroke::parse(key).unwrap();
        tree.update(cx, |tree, cx| tree.key_down(&keystroke, cx));
        cx.read(|cx| tree.read(cx).selected.clone())
    };
    assert_eq!(press("down", cx).as_deref(), Some("a"));
    assert_eq!(press("down", cx).as_deref(), Some("b"));
    assert_eq!(press("down", cx).as_deref(), Some("c"));
    assert_eq!(press("down", cx).as_deref(), Some("c"));
    assert_eq!(press("up", cx).as_deref(), Some("b"));

    // Stale rows stay navigable while a root refresh is in flight.
    tree.update(cx, |tree, cx| {
        tree.invalidate(registration, None, cx).unwrap();
    });
    assert_eq!(press("up", cx).as_deref(), Some("a"));
}

#[gpui::test]
fn loading_and_error_states_are_local_to_one_parent(cx: &mut TestAppContext) {
    use knot_protocol::TreeProviderError;

    let registration = TreeProviderRegistrationId::new(8);
    let tree = cx.new(|cx| TreeView::new("example.git", "1", cx));
    let generation = tree.update(cx, |tree, cx| {
        tree.register_provider("example.git", owner(registration), Vec::new(), cx)
            .unwrap();
        tree.root_generation()
    });
    let view = cx.read(|cx| tree.read(cx).instance_id());
    tree.update(cx, |tree, cx| {
        assert!(tree.apply_response(
            response(
                registration,
                view,
                None,
                generation,
                vec![
                    item("staged", "Staged", TreeItemKind::Group, Vec::new()),
                    item("notes", "notes.txt", TreeItemKind::Item, Vec::new()),
                ],
            ),
            cx,
        ));
        assert!(tree.is_loading(), "the group child request is pending");

        let group_generation = tree.children[&Some("staged".to_owned())].generation;
        assert!(tree.apply_response(
            TreeChildrenResponse {
                registration,
                window: "1".into(),
                view,
                parent_id: Some("staged".into()),
                generation: group_generation,
                result: Err(TreeProviderError {
                    message: "boom".into(),
                }),
            },
            cx,
        ));
        assert_eq!(tree.lifecycle_state(), (true, 2, false, 1));
        assert_eq!(tree.visible_labels(), ["Staged", "notes.txt"]);
    });
}

#[gpui::test]
fn root_refresh_keeps_rows_and_reloads_group_children(cx: &mut TestAppContext) {
    let registration = TreeProviderRegistrationId::new(7);
    let tree = cx.new(|cx| TreeView::new("example.git", "1", cx));
    let generation = tree.update(cx, |tree, cx| {
        tree.register_provider("example.git", owner(registration), Vec::new(), cx)
            .unwrap();
        tree.root_generation()
    });
    let view = cx.read(|cx| tree.read(cx).instance_id());
    tree.update(cx, |tree, cx| {
        assert!(tree.apply_response(
            response(
                registration,
                view,
                None,
                generation,
                vec![item("staged", "Staged", TreeItemKind::Group, Vec::new())],
            ),
            cx,
        ));
        let group_generation = tree.children[&Some("staged".to_owned())].generation;
        assert!(tree.apply_response(
            response(
                registration,
                view,
                Some("staged".into()),
                group_generation,
                vec![item("file", "file.rs", TreeItemKind::Item, Vec::new())],
            ),
            cx,
        ));
        assert_eq!(tree.visible_labels(), ["Staged", "file.rs"]);

        // A root refresh marks the root loading but keeps stale rows so
        // keyboard navigation stays usable while the refetch is pending.
        tree.invalidate(registration, None, cx).unwrap();
        assert!(tree.is_loading());
        assert_eq!(tree.visible_labels(), ["Staged", "file.rs"]);

        // The refreshed root reloads group children even when cached.
        let refreshed = tree.children[&None].generation;
        tree.apply_response(
            response(
                registration,
                view,
                None,
                refreshed,
                vec![item("staged", "Staged", TreeItemKind::Group, Vec::new())],
            ),
            cx,
        );
        let reloaded = tree.children[&Some("staged".to_owned())].generation;
        assert!(reloaded > group_generation);
    });
}

#[test]
fn only_expandable_item_rows_have_a_disclosure_icon() {
    let mut expandable = item("a", "a", TreeItemKind::Item, Vec::new());
    expandable.collapsible_state = TreeCollapsibleState::Collapsed;
    assert_eq!(
        TreeView::disclosure_name(&expandable, false),
        Some("tree.collapsed")
    );
    assert_eq!(
        TreeView::disclosure_name(&expandable, true),
        Some("tree.expanded")
    );
    let leaf = item("b", "b", TreeItemKind::Item, Vec::new());
    assert_eq!(TreeView::disclosure_name(&leaf, false), None);
    let mut group = item("c", "c", TreeItemKind::Group, Vec::new());
    group.collapsible_state = TreeCollapsibleState::Expanded;
    assert_eq!(TreeView::disclosure_name(&group, true), None);
}

#[gpui::test]
fn row_icons_follow_the_item_name_and_fall_back_when_unknown(cx: &mut TestAppContext) {
    use knot_model::icons::IconDefinition;
    let mut named = item("a", "a", TreeItemKind::Item, Vec::new());
    cx.update(|cx| {
        assert!(TreeView::icon(&named, false, cx).is_none());
        named.icon = Some("folder".into());
        assert!(TreeView::icon(&named, false, cx).is_some());
        named.icon = Some(knot_protocol::IconRef::Folder {
            folder: "src".into(),
        });
        assert_eq!(
            TreeView::icon_definition(&named, false, cx),
            Some(IconDefinition::lucide("folder"))
        );
        assert_eq!(
            TreeView::icon_definition(&named, true, cx),
            Some(IconDefinition::lucide("folder-open"))
        );
        assert_eq!(
            crate::app::icons::icon_definition("folder", cx),
            IconDefinition::lucide("folder")
        );
        assert_eq!(
            crate::app::icons::icon_definition("no.such.icon", cx),
            IconDefinition::lucide("circle-question-mark")
        );
    });
}

/// A tree with a root folder `a` holding a folder `b` that holds a file `c`,
/// answering only the requests the test allows through.
struct RevealFixture {
    tree: gpui::Entity<TreeView>,
    registration: TreeProviderRegistrationId,
    view: knot_protocol::ViewId,
}

impl RevealFixture {
    fn new(cx: &mut TestAppContext) -> Self {
        let registration = TreeProviderRegistrationId::new(11);
        let tree = tree(cx);
        tree.update(cx, |tree, cx| {
            tree.register_provider("example.git", owner(registration), Vec::new(), cx)
                .unwrap();
        });
        let view = cx.read(|cx| tree.read(cx).instance_id());
        Self {
            tree,
            registration,
            view,
        }
    }

    fn generation(&self, cx: &mut TestAppContext, parent: Option<&str>) -> u64 {
        cx.read(|cx| self.tree.read(cx).children[&parent.map(str::to_owned)].generation)
    }

    fn answer(&self, cx: &mut TestAppContext, parent: Option<&str>, ids: &[&str]) -> bool {
        let generation = self.generation(cx, parent);
        self.answer_generation(cx, parent, generation, ids)
    }

    fn answer_generation(
        &self,
        cx: &mut TestAppContext,
        parent: Option<&str>,
        generation: u64,
        ids: &[&str],
    ) -> bool {
        let items = ids
            .iter()
            .map(|id| {
                let mut row = item(id, id, TreeItemKind::Item, Vec::new());
                if !id.starts_with('f') {
                    row.collapsible_state = TreeCollapsibleState::Collapsed;
                }
                row
            })
            .collect();
        self.tree.update(cx, |tree, cx| {
            tree.apply_response(
                response(
                    self.registration,
                    self.view,
                    parent.map(str::to_owned),
                    generation,
                    items,
                ),
                cx,
            )
        })
    }

    fn labels(&self, cx: &mut TestAppContext) -> Vec<String> {
        cx.read(|cx| self.tree.read(cx).visible_labels())
    }

    fn selected(&self, cx: &mut TestAppContext) -> Option<String> {
        cx.read(|cx| self.tree.read(cx).selected_item_id())
    }

    fn reveal(&self, cx: &mut TestAppContext, path: &[&str]) {
        let path = path.iter().map(|id| (*id).to_owned()).collect();
        self.tree.update(cx, |tree, cx| tree.reveal(path, cx));
    }
}

#[gpui::test]
fn reveal_expands_unloaded_ancestors_in_turn_then_selects(cx: &mut TestAppContext) {
    let fixture = RevealFixture::new(cx);
    fixture.answer(cx, None, &["a", "z"]);
    fixture.reveal(cx, &["a", "b", "f-c"]);
    // Waiting for the children of a.
    assert_eq!(fixture.labels(cx), ["a", "z"]);
    assert_eq!(fixture.selected(cx), None);
    fixture.answer(cx, Some("a"), &["b"]);
    assert_eq!(fixture.selected(cx), None, "b is still loading");
    fixture.answer(cx, Some("b"), &["f-c"]);
    assert_eq!(fixture.labels(cx), ["a", "b", "f-c", "z"]);
    assert_eq!(fixture.selected(cx).as_deref(), Some("f-c"));
}

#[gpui::test]
fn reveal_waits_for_a_refresh_and_drops_stale_answers(cx: &mut TestAppContext) {
    let fixture = RevealFixture::new(cx);
    // The root is still loading when the reveal starts.
    fixture.reveal(cx, &["a", "f-c"]);
    let stale = fixture.generation(cx, None);
    cx.read(|cx| assert!(fixture.tree.read(cx).is_loading()));
    fixture.tree.update(cx, |tree, cx| tree.refresh_root(cx));
    // The first answer is for a superseded generation and changes nothing.
    assert!(!fixture.answer_generation(cx, None, stale, &["a"]));
    assert_eq!(fixture.selected(cx), None);
    fixture.answer(cx, None, &["a"]);
    fixture.answer(cx, Some("a"), &["f-c"]);
    assert_eq!(fixture.selected(cx).as_deref(), Some("f-c"));
}

#[gpui::test]
fn reveal_stops_at_a_missing_row_and_never_resumes(cx: &mut TestAppContext) {
    let fixture = RevealFixture::new(cx);
    fixture.answer(cx, None, &["a"]);
    fixture.reveal(cx, &["a", "missing", "f-c"]);
    fixture.answer(cx, Some("a"), &["b"]);
    assert_eq!(fixture.selected(cx), None);
    // A later load does not select the abandoned target.
    fixture.tree.update(cx, |tree, cx| {
        tree.refresh_root(cx);
    });
    fixture.answer(cx, None, &["a"]);
    assert_eq!(fixture.selected(cx), None);
}

#[gpui::test]
fn reveal_of_a_loaded_row_selects_it_at_once(cx: &mut TestAppContext) {
    let fixture = RevealFixture::new(cx);
    fixture.answer(cx, None, &["a", "z"]);
    fixture.reveal(cx, &["z"]);
    assert_eq!(fixture.selected(cx).as_deref(), Some("z"));
}

fn press(tree: &gpui::Entity<TreeView>, key: &str, cx: &mut TestAppContext) -> bool {
    let keystroke = crate::app::keys::Keystroke::parse(key).unwrap();
    tree.update(cx, |tree, cx| tree.key_down(&keystroke, cx))
}

#[gpui::test]
fn speed_search_jumps_to_the_best_match_and_moves_between_matches(cx: &mut TestAppContext) {
    let fixture = RevealFixture::new(cx);
    fixture.answer(cx, None, &["f-alpha", "f-banana", "f-bandana", "f-cherry"]);
    let tree = &fixture.tree;
    let search = |cx: &mut TestAppContext| {
        cx.read(|cx| {
            tree.read(cx)
                .search
                .as_ref()
                .map(|search| (search.query.clone(), search.matches.clone(), search.current))
        })
    };

    assert!(press(tree, "b", cx));
    assert_eq!(fixture.selected(cx).as_deref(), Some("f-banana"));
    assert!(press(tree, "a", cx));
    assert!(press(tree, "n", cx));
    assert!(press(tree, "d", cx));
    assert_eq!(fixture.selected(cx).as_deref(), Some("f-bandana"));
    let (query, matches, _) = search(cx).unwrap();
    assert_eq!(query, "band");
    assert_eq!(matches, ["f-bandana"]);
    // The matched text is what the row highlights.
    cx.read(|cx| {
        let ranges = &tree.read(cx).search.as_ref().unwrap().ranges["f-bandana"];
        assert_eq!(ranges.len(), 1);
        assert_eq!((ranges[0].start, ranges[0].end), (2, 6));
    });

    assert!(press(tree, "backspace", cx));
    assert!(press(tree, "backspace", cx));
    let (query, matches, _) = search(cx).unwrap();
    assert_eq!(query, "ba");
    assert_eq!(matches, ["f-banana", "f-bandana"]);
    assert!(press(tree, "down", cx));
    assert_eq!(fixture.selected(cx).as_deref(), Some("f-bandana"));
    assert!(press(tree, "down", cx));
    assert_eq!(fixture.selected(cx).as_deref(), Some("f-banana"));
    assert!(press(tree, "up", cx));
    assert_eq!(fixture.selected(cx).as_deref(), Some("f-bandana"));
    // Rows are highlighted, not filtered out.
    assert_eq!(fixture.labels(cx).len(), 4);
}

#[gpui::test]
fn speed_search_ends_on_escape_enter_and_an_empty_query(cx: &mut TestAppContext) {
    let fixture = RevealFixture::new(cx);
    fixture.answer(cx, None, &["f-alpha", "f-beta"]);
    let tree = &fixture.tree;
    let active = |cx: &mut TestAppContext| cx.read(|cx| tree.read(cx).search.is_some());

    assert!(press(tree, "z", cx));
    assert!(active(cx), "a query without matches stays open");
    assert_eq!(fixture.selected(cx), None);
    assert!(press(tree, "escape", cx));
    assert!(!active(cx));

    assert!(press(tree, "B", cx));
    assert_eq!(fixture.selected(cx).as_deref(), Some("f-beta"));
    assert!(press(tree, "backspace", cx));
    assert!(!active(cx));

    // Enter ends the search and activates the selected row.
    let (events, _subscriber) = subscribe_commands(cx, tree);
    assert!(press(tree, "a", cx));
    assert!(press(tree, "enter", cx));
    assert!(!active(cx));
    assert_eq!(events.borrow().len(), 1);

    // Modified keys and a lone space do not start a search.
    assert!(!press(tree, "ctrl-a", cx));
    assert!(!press(tree, "space", cx));
    assert!(!active(cx));
}

#[gpui::test]
fn speed_search_follows_rows_that_load_later(cx: &mut TestAppContext) {
    let fixture = RevealFixture::new(cx);
    fixture.answer(cx, None, &["a", "f-zebra"]);
    assert!(press(&fixture.tree, "z", cx));
    fixture.tree.update(cx, |tree, cx| {
        tree.activate("a", TreeGesture::Click, cx);
    });
    fixture.answer(cx, Some("a"), &["f-zoo"]);
    let matches = cx.read(|cx| {
        fixture
            .tree
            .read(cx)
            .search
            .as_ref()
            .unwrap()
            .matches
            .clone()
    });
    assert_eq!(matches, ["f-zoo", "f-zebra"]);
}

/// A tree in a window, with the folder `a` and the file `f-b` loaded.
struct EditFixture {
    window: gpui::WindowHandle<TreeView>,
    registration: TreeProviderRegistrationId,
    view: knot_protocol::ViewId,
}

impl EditFixture {
    fn new(cx: &mut TestAppContext) -> Self {
        let registration = TreeProviderRegistrationId::new(21);
        let window = cx.add_window(|_, cx| TreeView::new("example.git", "1", cx));
        window
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        let view = window
            .update(cx, |tree, _, cx| {
                tree.register_provider("example.git", owner(registration), Vec::new(), cx)
                    .unwrap();
                tree.instance_id()
            })
            .unwrap();
        let fixture = Self {
            window,
            registration,
            view,
        };
        fixture.answer(cx, None, &["a", "f-b"]);
        // An edit starts while the tree has focus.
        fixture
            .window
            .update(cx, |tree, window, cx| tree.focus.focus(window, cx))
            .unwrap();
        draw(cx, fixture.window);
        fixture
    }

    fn answer(&self, cx: &mut TestAppContext, parent: Option<&str>, ids: &[&str]) {
        self.window
            .update(cx, |tree, _, cx| {
                let generation = tree.children[&parent.map(str::to_owned)].generation;
                let items = ids
                    .iter()
                    .map(|id| {
                        let mut row = item(id, id, TreeItemKind::Item, Vec::new());
                        if !id.starts_with('f') {
                            row.collapsible_state = TreeCollapsibleState::Collapsed;
                        }
                        row
                    })
                    .collect();
                assert!(tree.apply_response(
                    response(
                        self.registration,
                        self.view,
                        parent.map(str::to_owned),
                        generation,
                        items
                    ),
                    cx
                ));
            })
            .unwrap();
    }

    fn begin(
        &self,
        cx: &mut TestAppContext,
        target: EditTarget,
        value: &str,
        selection: Option<std::ops::Range<usize>>,
        watching: bool,
    ) -> Option<EditSession> {
        let session = self
            .window
            .update(cx, |tree, window, cx| {
                tree.begin_edit(target, value.into(), selection, watching, window, cx)
            })
            .unwrap();
        // The field is painted, so the window tracks its focus.
        draw(cx, self.window);
        // The window's focus listeners activate in the next effect cycle.
        cx.run_until_parked();
        session
    }

    fn type_text(&self, cx: &mut TestAppContext, text: &str) {
        self.window
            .update(cx, |tree, _, cx| tree.edit_insert_text(text, cx))
            .unwrap();
    }

    fn press(&self, cx: &mut TestAppContext, key: &str) {
        press_window(self, cx, key);
    }

    fn text(&self, cx: &mut TestAppContext) -> Option<String> {
        self.window
            .update(cx, |tree, _, cx| tree.edit_text_for_test(cx))
            .unwrap()
    }

    fn drafts(&self, cx: &mut TestAppContext) -> usize {
        self.window
            .update(cx, |tree, _, _| {
                tree.visible_rows()
                    .iter()
                    .filter(|row| matches!(row, super::VisibleRow::Draft { .. }))
                    .count()
            })
            .unwrap()
    }
}

/// Paint the window, so it tracks the focus of what it shows.
fn draw(cx: &mut TestAppContext, window: gpui::WindowHandle<TreeView>) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx)
    })
    .unwrap();
}

fn press_window(fixture: &EditFixture, cx: &mut TestAppContext, key: &str) {
    let keystroke = crate::app::keys::Keystroke::parse(key).unwrap();
    fixture
        .window
        .update(cx, |tree, _, cx| tree.key_down(&keystroke, cx))
        .unwrap();
    cx.run_until_parked();
}

fn outcome(session: &mut EditSession) -> Option<crate::app::transient::TransientOutcome> {
    session.completion.try_recv().ok()
}

#[gpui::test]
fn a_label_edit_replaces_the_selection_and_commits_on_enter(cx: &mut TestAppContext) {
    use crate::app::transient::TransientOutcome;
    let fixture = EditFixture::new(cx);
    let mut session = fixture
        .begin(
            cx,
            EditTarget::Label("f-b".into()),
            "name.txt",
            Some(0..4),
            false,
        )
        .unwrap();
    fixture.type_text(cx, "other");
    assert_eq!(fixture.text(cx).as_deref(), Some("other.txt"));
    assert!(outcome(&mut session).is_none());
    fixture.press(cx, "enter");
    assert_eq!(
        outcome(&mut session),
        Some(TransientOutcome::Input("other.txt".into()))
    );
    assert_eq!(fixture.text(cx), None);
}

#[gpui::test]
fn escape_dismisses_and_focus_loss_commits(cx: &mut TestAppContext) {
    use crate::app::transient::TransientOutcome;
    let fixture = EditFixture::new(cx);
    let mut session = fixture
        .begin(cx, EditTarget::Label("f-b".into()), "name", None, false)
        .unwrap();
    fixture.press(cx, "escape");
    assert_eq!(outcome(&mut session), Some(TransientOutcome::Dismissed));

    let mut session = fixture
        .begin(cx, EditTarget::Label("f-b".into()), "name", None, false)
        .unwrap();
    fixture.type_text(cx, "kept");
    fixture
        .window
        .update(cx, |tree, window, cx| tree.focus.focus(window, cx))
        .unwrap();
    draw(cx, fixture.window);
    cx.run_until_parked();
    assert_eq!(
        outcome(&mut session),
        Some(TransientOutcome::Input("kept".into()))
    );
}

#[gpui::test]
fn a_new_row_edit_expands_its_parent_and_an_empty_commit_dismisses(cx: &mut TestAppContext) {
    use crate::app::transient::TransientOutcome;
    let fixture = EditFixture::new(cx);
    let mut session = fixture
        .begin(
            cx,
            EditTarget::New {
                parent: "a".into(),
                icon: Some("file".into()),
            },
            "",
            None,
            false,
        )
        .unwrap();
    assert_eq!(fixture.drafts(cx), 1);
    fixture.answer(cx, Some("a"), &["f-existing"]);
    assert_eq!(fixture.drafts(cx), 1);
    fixture.press(cx, "enter");
    assert_eq!(outcome(&mut session), Some(TransientOutcome::Dismissed));
    assert_eq!(fixture.drafts(cx), 0);

    let mut session = fixture
        .begin(
            cx,
            EditTarget::New {
                parent: "a".into(),
                icon: None,
            },
            "",
            None,
            false,
        )
        .unwrap();
    fixture.type_text(cx, "a/b.txt");
    fixture.press(cx, "enter");
    assert_eq!(
        outcome(&mut session),
        Some(TransientOutcome::Input("a/b.txt".into()))
    );
}

#[gpui::test]
fn validation_blocks_the_commit_until_the_current_text_is_answered_clean(cx: &mut TestAppContext) {
    use crate::app::transient::TransientOutcome;
    let fixture = EditFixture::new(cx);
    let mut session = fixture
        .begin(
            cx,
            EditTarget::Label("f-b".into()),
            "name",
            Some(4..4),
            true,
        )
        .unwrap();
    // The initial value is published for validation.
    assert_eq!(session.changes.borrow_and_update().as_deref(), Some("name"));
    let answer = |cx: &mut TestAppContext, text: &str, message: Option<&str>| {
        fixture
            .window
            .update(cx, |tree, _, cx| {
                tree.set_edit_feedback(Some(text), message.map(str::to_owned), cx)
            })
            .unwrap();
    };

    // Enter before the answer waits for it.
    fixture.press(cx, "enter");
    assert!(outcome(&mut session).is_none());
    answer(cx, "name", Some("taken"));
    assert!(
        outcome(&mut session).is_none(),
        "a message blocks the commit"
    );
    let shown = fixture
        .window
        .update(cx, |tree, _, _| tree.edit_message_for_test())
        .unwrap();
    assert_eq!(shown.as_deref(), Some("taken"));

    // Typing changes the value, so the old answer no longer applies.
    fixture.type_text(cx, "2");
    assert_eq!(
        session.changes.borrow_and_update().as_deref(),
        Some("name2")
    );
    answer(cx, "name", None);
    fixture.press(cx, "enter");
    assert!(
        outcome(&mut session).is_none(),
        "an answer for older text is dropped"
    );
    answer(cx, "name2", None);
    assert_eq!(
        outcome(&mut session),
        Some(TransientOutcome::Input("name2".into()))
    );
}

#[gpui::test]
fn focus_loss_with_a_message_dismisses_and_a_vanished_row_ends_the_edit(cx: &mut TestAppContext) {
    use crate::app::transient::TransientOutcome;
    let fixture = EditFixture::new(cx);
    let mut session = fixture
        .begin(cx, EditTarget::Label("f-b".into()), "name", None, true)
        .unwrap();
    fixture
        .window
        .update(cx, |tree, _, cx| {
            tree.set_edit_feedback(Some("name"), Some("taken".into()), cx)
        })
        .unwrap();
    fixture
        .window
        .update(cx, |tree, window, cx| tree.focus.focus(window, cx))
        .unwrap();
    draw(cx, fixture.window);
    cx.run_until_parked();
    assert_eq!(outcome(&mut session), Some(TransientOutcome::Dismissed));

    let mut session = fixture
        .begin(cx, EditTarget::Label("f-b".into()), "name", None, false)
        .unwrap();
    fixture
        .window
        .update(cx, |tree, _, cx| {
            let generation = tree.children[&None].generation;
            assert!(tree.apply_response(
                response(fixture.registration, fixture.view, None, generation, vec![]),
                cx
            ));
        })
        .unwrap();
    assert_eq!(outcome(&mut session), Some(TransientOutcome::Dismissed));
    assert!(
        fixture
            .begin(cx, EditTarget::Label("missing".into()), "x", None, false)
            .is_none()
    );
}
