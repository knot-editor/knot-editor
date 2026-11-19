//! Dragging tree rows that stand for a resource onto the editor area.
use super::operations::empty_product_window;
use super::*;
use crate::app::tree_view::TreeProviderIdentity;
use crate::app::workbench::Area;
use gpui::{Modifiers, MouseButton, VisualTestContext, point, px};
use knot_protocol::{
    ExtensionId, ExtensionLifecycleId, TreeChildrenResponse, TreeCollapsibleState, TreeItem,
    TreeItemKind, TreeProviderRegistrationId,
};

fn row(id: &str, resource: Option<&str>) -> TreeItem {
    TreeItem {
        id: id.into(),
        label: id.into(),
        description: None,
        detail: None,
        detail_max_lines: None,
        bold: false,
        icon: None,
        collapsible_state: TreeCollapsibleState::None,
        command: None,
        arguments: CommandArgumentValue::Null,
        kind: TreeItemKind::Item,
        muted: false,
        status_text: None,
        badge: None,
        actions: Vec::new(),
        draggable: true,
        drop_target: false,
        resource: resource.map(str::to_owned),
    }
}

/// A window whose test tool lists `notes.txt` and `other.txt` rows that carry
/// their resource, and a `bare` row that carries none.
fn fixture(cx: &mut TestAppContext) -> (Entity<KnotWindow>, gpui::AnyWindowHandle) {
    let documents = install_globals(cx);
    let (filesystems, provider) = memory_filesystems_with_provider();
    for name in ["notes.txt", "other.txt"] {
        provider
            .seed_file(
                ResourceUri::parse(&format!("mem://product/{name}")).unwrap(),
                b"text",
            )
            .unwrap();
    }
    cx.set_global(ApplicationFileSystems(filesystems));
    let (window, native_window) = empty_product_window(&documents, cx);
    window.update(cx, |window, cx| window.ensure_test_tool(true, cx));
    cx.run_until_parked();
    let tree = cx.read(|cx| window.read(cx).test_tool_tree(cx));
    let registration = TreeProviderRegistrationId::new(3);
    cx.update_window(native_window, |_, _, cx| {
        tree.update(cx, |tree, cx| {
            tree.register_provider(
                "test.tool",
                TreeProviderIdentity {
                    extension: ExtensionId::new(1),
                    lifecycle: ExtensionLifecycleId::new(1),
                    registration,
                },
                Vec::new(),
                cx,
            )
            .unwrap();
            tree.invalidate(registration, None, cx).unwrap();
            let response = TreeChildrenResponse {
                registration,
                window: crate::app::product::window_id(window.entity_id()),
                view: tree.instance_id(),
                parent_id: None,
                generation: tree.root_generation(),
                result: Ok(vec![
                    row("notes", Some("mem://product/notes.txt")),
                    row("other", Some("mem://product/other.txt")),
                    row("bare", None),
                ]),
            };
            assert!(tree.apply_response(response, cx));
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.refresh().unwrap();
    (window, native_window)
}

fn open_files(window: &Entity<KnotWindow>, cx: &TestAppContext) -> Vec<Vec<String>> {
    cx.read(|cx| {
        let documents = cx.global::<ApplicationDocuments>().0.read(cx);
        let workbench = window.read(cx).workbench.read(cx);
        workbench
            .panes()
            .iter()
            .filter(|pane| workbench.pane_area(pane.id()) == Some(Area::Center))
            .map(|pane| {
                pane.tabs()
                    .iter()
                    .filter_map(|tab| {
                        let uri = documents.get(tab.document_id()?)?.resource_uri()?;
                        Some(
                            uri.to_string()
                                .trim_start_matches("mem://product/")
                                .to_owned(),
                        )
                    })
                    .collect()
            })
            .collect()
    })
}

/// Drag row `index` of the test tool to `to(visual)` and let go, returning
/// whether a preview showed over the target.
fn drag_row(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    index: usize,
    to: impl Fn(&mut VisualTestContext) -> gpui::Point<gpui::Pixels>,
    cx: &mut TestAppContext,
) -> bool {
    let mut visual = VisualTestContext::from_window(native_window, cx);
    visual.update(|window, _| window.refresh());
    visual.run_until_parked();
    let selector: &'static str = Box::leak(format!("extension-tree-row-{index}").into_boxed_str());
    let from = visual.debug_bounds(selector).unwrap().center();
    visual.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_move(
        from + point(px(12.), px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    let to = to(&mut visual);
    visual.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    let previewed = visual.read(|cx| window.read(cx).tab_drag.borrow().resource_preview.is_some());
    visual.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    previewed
}

#[gpui::test]
fn a_resource_row_dropped_on_a_pane_opens_a_pinned_tab_there(cx: &mut TestAppContext) {
    let (window, native_window) = fixture(cx);
    let previewed = drag_row(
        &window,
        native_window,
        0,
        |visual| visual.debug_bounds("pane-1").unwrap().center(),
        cx,
    );
    assert!(previewed);
    let files = open_files(&window, cx);
    assert_eq!(files.len(), 1);
    assert!(files[0].contains(&"notes.txt".to_owned()));
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert!(
            workbench
                .panes()
                .iter()
                .flat_map(|pane| pane.tabs())
                .all(|tab| !tab.is_preview())
        );
        assert!(window.read(cx).tab_drag.borrow().resource_preview.is_none());
    });
}

#[gpui::test]
fn a_resource_row_dropped_on_a_pane_edge_opens_in_a_new_split(cx: &mut TestAppContext) {
    let (window, native_window) = fixture(cx);
    let previewed = drag_row(
        &window,
        native_window,
        1,
        |visual| {
            let pane = visual.debug_bounds("pane-1").unwrap();
            point(pane.right() - px(5.), pane.center().y)
        },
        cx,
    );
    assert!(previewed);
    let files = open_files(&window, cx);
    assert_eq!(files.len(), 2);
    assert_eq!(files[1], ["other.txt"]);
}

#[gpui::test]
fn a_row_without_a_resource_offers_no_editor_target(cx: &mut TestAppContext) {
    let (window, native_window) = fixture(cx);
    let before = open_files(&window, cx);
    let previewed = drag_row(
        &window,
        native_window,
        2,
        |visual| visual.debug_bounds("pane-1").unwrap().center(),
        cx,
    );
    assert!(!previewed);
    assert_eq!(open_files(&window, cx), before);
}

#[gpui::test]
fn a_resource_that_fails_to_open_reports_through_the_open_notice(cx: &mut TestAppContext) {
    let (window, native_window) = fixture(cx);
    let tree = cx.read(|cx| window.read(cx).test_tool_tree(cx));
    // The row names a resource no provider handles.
    cx.update_window(native_window, |_, _, cx| {
        tree.update(cx, |tree, cx| {
            let registration = TreeProviderRegistrationId::new(3);
            tree.invalidate(registration, None, cx).unwrap();
            let response = TreeChildrenResponse {
                registration,
                window: crate::app::product::window_id(window.entity_id()),
                view: tree.instance_id(),
                parent_id: None,
                generation: tree.root_generation(),
                result: Ok(vec![row("gone", Some("nowhere://product/missing.txt"))]),
            };
            assert!(tree.apply_response(response, cx));
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.refresh().unwrap();
    let before = open_files(&window, cx);
    drag_row(
        &window,
        native_window,
        0,
        |visual| visual.debug_bounds("pane-1").unwrap().center(),
        cx,
    );
    assert_eq!(open_files(&window, cx), before);
    cx.read(|cx| {
        let notices = cx
            .global::<crate::app::notifications::ApplicationNotifications>()
            .0
            .notices();
        assert!(
            notices
                .iter()
                .any(|notice| notice.title.as_deref() == Some("Open failed"))
        );
    });
}

#[gpui::test]
fn a_resource_row_dropped_in_the_empty_center_opens_there(cx: &mut TestAppContext) {
    let (window, native_window) = fixture(cx);
    cx.update_window(native_window, |_, native_window, cx| {
        window.update(cx, |window, cx| {
            window.workbench.update(cx, |workbench, _| {
                let center = workbench.center_pane().unwrap();
                workbench.focus_pane(center);
            });
            window.close_active_tab(native_window, cx)
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.read(|cx| assert!(window.read(cx).workbench.read(cx).center_pane().is_none()));
    let previewed = drag_row(
        &window,
        native_window,
        0,
        |visual| visual.debug_bounds("empty-center").unwrap().center(),
        cx,
    );
    assert!(previewed);
    assert_eq!(open_files(&window, cx), [["notes.txt"]]);
}
