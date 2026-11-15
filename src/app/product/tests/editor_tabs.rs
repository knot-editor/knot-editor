//! Center tab chrome: the dirty dot, the pinned icon, and the close button.
use super::*;
use gpui::VisualTestContext;

fn painted(visual: &mut VisualTestContext, name: &str, tab: u64) -> bool {
    visual
        .debug_bounds(Box::leak(format!("{name}-{tab}").into_boxed_str()))
        .is_some()
}

#[gpui::test]
fn a_tab_shows_its_dirty_and_pinned_indicators(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("text")));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("text", model.clone(), cx)
    });
    let (window, native_window) = product_window(document, model.clone(), cx);
    let (pane, tab) = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let pane = workbench.focused_pane().unwrap();
        (workbench.focused_pane_id().unwrap(), pane.active_tab_id())
    });
    let tab_value = tab.value();
    let mut visual = VisualTestContext::from_window(native_window, cx);
    visual.run_until_parked();
    assert!(painted(&mut visual, "close-tab", tab_value));
    assert!(!painted(&mut visual, "dirty", tab_value));
    assert!(!painted(&mut visual, "pin", tab_value));

    model.update(&mut visual, |model, _| model.replace(0..0, "x").unwrap());
    window.update(&mut visual, |window, cx| {
        window
            .workbench
            .update(cx, |workbench, _| workbench.toggle_pin(pane, tab).unwrap());
        cx.notify();
    });
    visual.run_until_parked();
    assert!(painted(&mut visual, "dirty", tab_value));
    assert!(painted(&mut visual, "pin", tab_value));
    assert!(painted(&mut visual, "close-tab", tab_value));
}
