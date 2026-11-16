//! The tab context menu: items from the menu registry run against the clicked
//! tab, whichever tab has focus.
use super::*;
use crate::app::documents::DocumentId;
use crate::app::menus::{ApplicationMenus, MenuItem, MenuLocation, MenuOwner};
use crate::app::product::Focus;

fn owner() -> MenuOwner {
    MenuOwner {
        extension: ExtensionId::new(991),
        lifecycle: ExtensionLifecycleId::new(1),
    }
}

fn add(command: &str, group: &str, view: Option<&str>, cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.default_global::<ApplicationMenus>()
            .0
            .add(
                owner(),
                MenuLocation::Tab,
                MenuItem {
                    command: command.into(),
                    title: None,
                    arguments: CommandArgumentValue::Null,
                    group: Some(group.into()),
                    order: 0,
                    view: view.map(Into::into),
                    context: None,
                },
            )
            .unwrap();
    });
}

/// A window of four untitled documents in one pane, the last active, where
/// the third is dirty.
fn fixture(
    cx: &mut TestAppContext,
) -> (Entity<KnotWindow>, gpui::AnyWindowHandle, [DocumentId; 4]) {
    let documents = install_globals(cx);
    let created = ["a", "b", "c", "d"].map(|text| {
        let model = cx.new(|_| Buffer::new(BufferModel::from_text(text)));
        let document = documents.update(cx, |documents, cx| {
            documents.create_untitled(text, model.clone(), cx)
        });
        (document, model)
    });
    created[2]
        .1
        .update(cx, |model, _| model.replace(0..0, "unsaved ").unwrap());
    let (window, native_window) = product_window(created[0].0, created[0].1.clone(), cx);
    window.update(cx, |window, cx| {
        window.workbench.update(cx, |workbench, cx| {
            let pane = workbench.focused_pane_id().unwrap();
            for (document, model) in &created[1..] {
                workbench
                    .open_tab_for_document(pane, *document, model.clone(), cx)
                    .unwrap();
            }
        });
    });
    cx.update_window(native_window, |_, native_window, _| {
        native_window.activate_window()
    })
    .unwrap();
    cx.run_until_parked();
    (window, native_window, created.map(|(document, _)| document))
}

fn tab_documents(window: &Entity<KnotWindow>, cx: &TestAppContext) -> Vec<DocumentId> {
    cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .tabs()
            .iter()
            .filter_map(|tab| tab.document_id())
            .collect()
    })
}

fn active_document(window: &Entity<KnotWindow>, cx: &TestAppContext) -> DocumentId {
    cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .active_tab()
            .document_id()
            .unwrap()
    })
}

fn right_click_tab(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    index: usize,
    cx: &mut TestAppContext,
) {
    let tab = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .tabs()[index]
            .id()
    });
    cx.refresh().unwrap();
    let selector: &'static str = Box::leak(format!("tab-{}", tab.value()).into_boxed_str());
    let center = VisualTestContext::from_window(native_window, cx)
        .debug_bounds(selector)
        .expect("the tab is laid out")
        .center();
    let mut window_cx = VisualTestContext::from_window(native_window, cx);
    window_cx.simulate_mouse_down(center, MouseButton::Right, Modifiers::default());
    window_cx.simulate_mouse_up(center, MouseButton::Right, Modifiers::default());
    cx.run_until_parked();
}

fn press(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    keys: &str,
    cx: &mut TestAppContext,
) {
    cx.refresh().unwrap();
    cx.simulate_keystrokes(native_window, keys);
    crate::app::product::driver::settle(window, cx);
}

#[gpui::test]
fn a_menu_item_acts_on_the_clicked_tab_and_leaves_focus_and_the_active_tab(
    cx: &mut TestAppContext,
) {
    let (window, native_window, [_, b, _, d]) = fixture(cx);
    add("workbench.toggle-pin-tab", "1_pin", None, cx);
    right_click_tab(&window, native_window, 1, cx);
    assert_eq!(
        cx.read(|cx| window.read(cx).focus == Focus::Popup),
        true,
        "an open menu has the popup focus layer"
    );
    press(&window, native_window, "down enter", cx);

    // The clicked tab is pinned, which moves it first; the active tab and
    // the focus stay.
    assert_eq!(tab_documents(&window, cx)[0], b);
    assert_eq!(active_document(&window, cx), d);
    cx.read(|cx| {
        assert_eq!(window.read(cx).focus, Focus::Workbench);
        let workbench = window.read(cx).workbench.read(cx);
        assert!(workbench.focused_pane().unwrap().tabs()[0].is_pinned());
    });
}

#[gpui::test]
fn items_show_only_on_the_surfaces_they_are_scoped_to(cx: &mut TestAppContext) {
    let (window, native_window, _) = fixture(cx);
    add("workbench.toggle-pin-tab", "1_pin", None, cx);
    add("workbench.close-tab", "2_close", Some("editor"), cx);
    add("workbench.close-pane", "3_pane", Some("terminal"), cx);
    let directory = tempfile::tempdir().unwrap();
    window.update(cx, |window, cx| {
        let (id, view) = window.create_terminal_in(Some(directory.path().to_owned()), cx);
        window.workbench.update(cx, |workbench, _| {
            let pane = workbench.focused_pane_id().unwrap();
            workbench.open_terminal_tab(pane, id, view);
        });
    });
    cx.run_until_parked();
    let commands = |index: usize, cx: &mut TestAppContext| {
        cx.update_window(native_window, |_, _, cx| {
            window.update(cx, |window, cx| {
                let (pane, tab) = {
                    let pane = window.workbench.read(cx).focused_pane().unwrap();
                    (pane.id(), pane.tabs()[index].id())
                };
                let target = window.capture_tab_target(pane, tab, cx).unwrap();
                KnotWindow::menu_groups(MenuLocation::Tab, &target, cx)
                    .into_iter()
                    .map(|group| {
                        group
                            .into_iter()
                            .map(|entry| entry.command)
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
        })
        .unwrap()
    };
    assert_eq!(
        commands(0, cx),
        [["workbench.toggle-pin-tab"], ["workbench.close-tab"]]
    );
    assert_eq!(
        commands(4, cx),
        [["workbench.toggle-pin-tab"], ["workbench.close-pane"]]
    );
}

#[gpui::test]
fn closing_the_tabs_to_the_right_prompts_for_the_dirty_ones(cx: &mut TestAppContext) {
    let (window, native_window, [a, b, _, _]) = fixture(cx);
    add("workbench.close-tabs-to-right", "1_close", None, cx);
    right_click_tab(&window, native_window, 1, cx);
    press(&window, native_window, "down enter", cx);
    crate::test_support::wait_until(cx, |cx| cx.has_pending_prompt());

    // Cancelling leaves every tab; answering closes the two to the right.
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(tab_documents(&window, cx).len(), 4);
    right_click_tab(&window, native_window, 1, cx);
    press(&window, native_window, "down enter", cx);
    crate::test_support::wait_until(cx, |cx| cx.has_pending_prompt());
    cx.simulate_prompt_answer("Don't Save");
    crate::test_support::wait_until(cx, |cx| tab_documents(&window, cx).len() == 2);
    assert_eq!(tab_documents(&window, cx), [a, b]);
}
