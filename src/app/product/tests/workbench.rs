//! Tabs, splits, blank replacements, and view lifetime across windows.
use super::*;

#[gpui::test]
fn window_actions_render_tabs_and_both_split_directions(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, cx) = cx.add_window_view(|_, cx| {
        let workbench = cx.new(|cx| Workbench::new_for_document(document, model, cx));
        KnotWindow::new(workbench, cx)
    });

    cx.update(|_, cx| {
        window.update(cx, |window, cx| window.new_tab(cx));
        window.update(cx, |window, cx| {
            window.split(SplitDirection::Horizontal, cx)
        });
        window.update(cx, |window, cx| window.split(SplitDirection::Vertical, cx));
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert_eq!(workbench.panes().len(), 3);
        assert!(matches!(
            workbench.center(),
            Some(WorkbenchLayout::Split {
                direction: SplitDirection::Horizontal,
                second,
                ..
            }) if matches!(
                second.as_ref(),
                WorkbenchLayout::Split {
                    direction: SplitDirection::Vertical,
                    ..
                }
            )
        ));
        assert_eq!(
            workbench
                .panes()
                .iter()
                .map(|pane| pane.tabs().len())
                .sum::<usize>(),
            4
        );
    });
}

#[gpui::test]
fn new_tab_targets_the_pane_whose_editor_has_focus(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, cx) = cx.add_window_view(|_, cx| {
        let workbench = cx.new(|cx| Workbench::new_for_document(document, model, cx));
        KnotWindow::new(workbench, cx)
    });

    cx.update(|_, cx| {
        window.update(cx, |window, cx| {
            window.split(SplitDirection::Horizontal, cx)
        });
    });
    cx.run_until_parked();
    cx.refresh().unwrap();

    let (first_pane, first_editor, second_editor) = cx.read(|cx| {
        let window = window.read(cx);
        let workbench = window.workbench.read(cx);
        let first_pane = workbench.panes()[0].id();
        (
            first_pane,
            workbench
                .editor(workbench.pane(first_pane).unwrap().active_tab())
                .unwrap()
                .clone(),
            workbench
                .editor(workbench.panes()[1].active_tab())
                .unwrap()
                .clone(),
        )
    });
    cx.update(|window, cx| focus_view(&second_editor.focus_handle(cx), window, cx));
    cx.run_until_parked();
    cx.refresh().unwrap();
    assert!(cx.update(|window, cx| second_editor.focus_handle(cx).is_focused(window)));
    cx.update(|window, cx| focus_view(&first_editor.focus_handle(cx), window, cx));
    cx.run_until_parked();
    cx.refresh().unwrap();
    assert!(cx.update(|window, cx| first_editor.focus_handle(cx).is_focused(window)));

    cx.update(|_, cx| {
        window.update(cx, |window, cx| window.new_tab(cx));
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert_eq!(workbench.pane(first_pane).unwrap().tabs().len(), 2);
        assert_eq!(
            workbench
                .panes()
                .iter()
                .find(|pane| pane.id() != first_pane)
                .unwrap()
                .tabs()
                .len(),
            1
        );
    });
}

#[gpui::test]
fn closing_the_final_clean_tab_leaves_an_empty_center_that_takes_new_documents(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    let original = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(original).unwrap().model().clone());
    let (window, cx) = cx.add_window_view(|_, cx| {
        let workbench = cx.new(|cx| Workbench::new_for_document(original, model, cx));
        KnotWindow::new(workbench, cx)
    });

    cx.update(|native_window, cx| {
        window.update(cx, |window, cx| window.close_active_tab(native_window, cx));
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert!(workbench.panes().is_empty());
        assert!(documents.read(cx).get(original).is_none());
        assert_eq!(documents.read(cx).documents().count(), 0);
    });
    // The empty center paints and still opens documents.
    cx.run_until_parked();
    assert!(cx.debug_bounds("empty-center").is_some());
    window.update(cx, |window, cx| window.new_tab(cx));
    cx.run_until_parked();
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let pane = workbench.focused_pane().unwrap();
        assert_eq!(pane.area(), crate::app::workbench::Area::Center);
        assert!(pane.active_tab().document_id().is_some());
    });
    assert!(cx.debug_bounds("empty-center").is_none());
}

#[gpui::test]
fn replacing_a_shared_blank_tab_preserves_the_other_windows_document(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let blank = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(blank).unwrap().model().clone());
    let (first, first_window) = product_window(blank, model.clone(), cx);
    let (second, _) = product_window(blank, model.clone(), cx);
    let replacement = cx.update(|cx| create_untitled_document(&documents, cx));
    cx.update_window(first_window, |_, _window, cx| {
        first.update(cx, |window, cx| {
            let workbench = window.workbench.read(cx);
            let pane = workbench.focused_pane().unwrap();
            let target = super::super::OpenTarget {
                window: cx.weak_entity(),
                workbench: window.workbench.downgrade(),
                tab: Some(crate::app::commands::dispatch::TargetTab {
                    pane: pane.id(),
                    tab: pane.active_tab_id(),
                    surface: pane.active_tab().surface_id(),
                    presentation: pane.active_tab().presentation(),
                }),
            };
            window.present_open_document(replacement, &target, true, false, cx);
        });
    })
    .unwrap();
    cx.read(|cx| {
        assert!(documents.read(cx).get(blank).is_some());
        assert!(model.read(cx).is_open());
        assert_eq!(first.read(cx).workbench.read(cx).view_count(blank), 0);
        assert_eq!(second.read(cx).workbench.read(cx).view_count(blank), 1);
    });
}

#[gpui::test]
fn workbench_registry_counts_views_across_windows(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (_first, _) = product_window(document, model.clone(), cx);
    let (_second, _) = product_window(document, model, cx);
    cx.read(|cx| {
        assert_eq!(
            cx.global::<ApplicationWindows>().view_count(document, cx),
            2
        )
    });
}

#[gpui::test]
async fn retained_closed_window_does_not_keep_the_last_live_document_open(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (closed_window, closed_native_window) = product_window(document, model.clone(), cx);
    let (live_window, live_native_window) = product_window(document, model.clone(), cx);
    assert_eq!(
        dispatch_product_command(
            &closed_window,
            closed_native_window,
            super::super::CLOSE_WINDOW_COMMAND,
            cx
        )
        .completion
        .await
        .unwrap(),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        assert_eq!(
            closed_window
                .read(cx)
                .workbench
                .read(cx)
                .view_count(document),
            1
        );
        assert!(documents.read(cx).get(document).is_some());
    });
    assert_eq!(
        dispatch_product_command(&live_window, live_native_window, CLOSE_TAB_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        assert!(documents.read(cx).get(document).is_none());
        assert!(!model.read(cx).is_open());
    });
}
