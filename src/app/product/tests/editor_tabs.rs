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

#[gpui::test]
fn a_decorated_document_tab_keeps_its_indicators_and_follows_the_registry(cx: &mut TestAppContext) {
    use crate::app::resource::ResourceUri;
    use crate::app::resource_decorations::{
        ApplicationResourceDecorations, Decoration, DecorationOwner, decoration_of,
    };
    use knot_protocol::{ExtensionId, ExtensionLifecycleId};

    let documents = install_globals(cx);
    cx.set_global(ApplicationResourceDecorations::default());
    let uri = ResourceUri::parse("mem://tabs/a.txt").unwrap();
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("text")));
    let document = documents.update(cx, |documents, cx| {
        documents.create_persisted(
            "a.txt",
            model.clone(),
            uri.clone(),
            0,
            ResourceVersion::new([1]),
            cx,
        )
    });
    let (window, native_window) = product_window(document, model.clone(), cx);
    let (pane, tab) = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        (
            workbench.focused_pane_id().unwrap(),
            workbench.focused_pane().unwrap().active_tab_id(),
        )
    });
    let owner = DecorationOwner {
        extension: ExtensionId::new(1),
        lifecycle: ExtensionLifecycleId::new(1),
    };
    let source = cx.update(|cx| {
        let registry = &mut cx.global_mut::<ApplicationResourceDecorations>().0;
        let source = registry.create_source(owner);
        registry.set(
            owner,
            source,
            vec![(
                uri.clone(),
                Some(Decoration {
                    color: Some("diff.modified".into()),
                    tooltip: Some("Modified".into()),
                    ..Default::default()
                }),
            )],
        );
        cx.refresh_windows();
        source
    });
    let decoration = cx.read(|cx| decoration_of(&uri, cx)).unwrap();
    assert_eq!(decoration.color.as_deref(), Some("diff.modified"));
    assert_eq!(decoration.tooltip.as_deref(), Some("Modified"));
    // The color replaces the theme's foreground, active or not; a decoration
    // without a color leaves it.
    for active in [true, false] {
        assert_eq!(
            super::super::render::tab_foreground(Some(&decoration), active),
            "diff.modified"
        );
    }
    assert_eq!(
        super::super::render::tab_foreground(Some(&Default::default()), true),
        crate::app::theme::color::TAB_ACTIVE_FOREGROUND
    );

    // The dirty dot and the pin stay beside the colored title.
    model.update(cx, |model, _| model.replace(0..0, "x").unwrap());
    window.update(cx, |window, cx| {
        window
            .workbench
            .update(cx, |workbench, _| workbench.toggle_pin(pane, tab).unwrap());
        cx.notify();
    });
    let mut visual = VisualTestContext::from_window(native_window, cx);
    visual.run_until_parked();
    assert!(painted(&mut visual, "dirty", tab.value()));
    assert!(painted(&mut visual, "pin", tab.value()));

    visual.update(|_, cx| {
        cx.global_mut::<ApplicationResourceDecorations>()
            .0
            .remove_source(owner, source);
        cx.refresh_windows();
    });
    visual.run_until_parked();
    assert_eq!(visual.read(|cx| decoration_of(&uri, cx)), None);
    assert!(painted(&mut visual, "dirty", tab.value()));
}
