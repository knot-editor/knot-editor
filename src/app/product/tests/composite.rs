//! Composite views: opening, the view tree, tab updates, and closing.
use super::*;
use knot_model::workbench::TabSurfaceId;
use knot_protocol::SurfaceKind;

const SOURCE: &str = r#"
import { buffers, commands, workbench } from "knot";
const check = (condition, message) => { if (!condition) throw new Error(message); };
const expectCode = async (operation, code) => {
  try { await operation(); } catch (error) { if (error.code === code) return; throw error; }
  throw new Error(`expected ${code}`);
};
let registration;
let instance;
let other;
await commands.register("composite.open", async () => {
  if (registration === undefined) {
    registration = await workbench.registerComposite({ id: "example.view" });
    await expectCode(() => workbench.registerComposite({ id: "example.view" }), "ViewIdInUse");
    await expectCode(() => workbench.registerComposite({ id: "knot.view" }), "InvalidViewId");
  }
  const buffer = await buffers.create({ text: "one\ntwo\n", language: "plaintext" });
  instance = await registration.open({ title: "Example", icon: "file", buffer });
});
await commands.register("composite.update", async () => {
  await instance.update({ title: "Renamed", resource: "file:///a.txt" });
  await expectCode(() => instance.update({ resource: "not a uri" }), "InvalidUri");
});
await commands.register("composite.clear", async () => {
  await instance.update({ icon: null, resource: null });
});
await commands.register("composite.close", async () => {
  check((await instance.close()).kind === "closed", "closed");
  check((await instance.close()).kind === "closed", "closed again");
  await expectCode(() => instance.update({ title: "Late" }), "InvalidTarget");
});
await commands.register("composite.dispose-registration", async () => {
  await registration[Symbol.asyncDispose]();
  registration = undefined;
});
await commands.register("composite.foreign-buffer", async () => {
  other = await workbench.registerComposite({ id: "example.other" });
  try { await other.open({ title: "x", buffer: {} }); } catch (error) {
    check(error instanceof TypeError, "buffer type");
    return;
  }
  throw new Error("expected TypeError");
});
"#;

fn load(
    cx: &mut TestAppContext,
) -> (
    Entity<crate::app::extensions::host::ProductExtensionHost>,
    knot_protocol::ExtensionKey,
) {
    use knot_protocol::{ExtensionId, ExtensionKey, ExtensionLifecycleId};
    let host = cx.update(crate::app::extensions::host::install);
    let key = ExtensionKey::new(ExtensionId::new(731), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, SOURCE).unwrap();
        cx.spawn(async move |_, _| execution.await.unwrap())
            .detach();
    });
    wait_for_api_condition(cx, |cx| {
        cx.global::<ApplicationProductCommands>()
            .0
            .read(cx)
            .catalog()
            .borrow()
            .resolve("composite.foreign-buffer")
            .is_ok()
    });
    (host, key)
}

fn run(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    name: &'static str,
    cx: &mut TestAppContext,
) {
    let execution = dispatch_product_command(window, native_window, name, cx);
    assert_eq!(
        wait_for_command(execution, cx),
        CommandOutcome::Completed,
        "{name}"
    );
    cx.run_until_parked();
}

fn focus_document_tab(window: &Entity<KnotWindow>, cx: &mut TestAppContext) {
    window.update(cx, |window, cx| {
        window.workbench.update(cx, |workbench, _| {
            let pane = workbench.focused_pane_id().unwrap();
            let first = workbench.tabs().next().unwrap().id();
            workbench.activate_tab(pane, first);
        });
        window.focus_active_surface(cx);
    });
}

fn composite_tabs(window: &Entity<KnotWindow>, cx: &TestAppContext) -> usize {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        workbench
            .tabs()
            .filter(|tab| matches!(tab.surface_id(), TabSurfaceId::Composite(_)))
            .count()
    })
}

/// The shown tab of the first composite view.
fn tab_state(
    window: &Entity<KnotWindow>,
    cx: &TestAppContext,
) -> crate::app::composite_view::CompositeTab {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let tab = workbench
            .tabs()
            .find(|tab| matches!(tab.surface_id(), TabSurfaceId::Composite(_)))
            .unwrap();
        workbench
            .composite_view(tab)
            .unwrap()
            .read(cx)
            .tab()
            .clone()
    })
}

fn info(window: &Entity<KnotWindow>, cx: &TestAppContext) -> knot_protocol::WindowInfo {
    cx.read(|cx| {
        window.read(cx).application_window(
            window.entity_id(),
            &std::collections::HashMap::new(),
            cx,
        )
    })
}

fn window_with_extension(
    cx: &mut TestAppContext,
) -> (
    Entity<KnotWindow>,
    gpui::AnyWindowHandle,
    Entity<crate::app::extensions::host::ProductExtensionHost>,
    knot_protocol::ExtensionKey,
) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let (host, key) = load(cx);
    (window, native_window, host, key)
}

#[gpui::test]
fn an_extension_opens_a_composite_with_an_editor_in_its_view_tree(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open", cx);
    assert_eq!(composite_tabs(&window, cx), 1);

    let info = info(&window, cx);
    let tab = info
        .panes
        .iter()
        .flat_map(|pane| &pane.tabs)
        .find(|tab| tab.kind == SurfaceKind::Composite)
        .unwrap();
    assert_eq!(tab.root.kind, "composite");
    assert_eq!(tab.root.children.len(), 1);
    let editor = &tab.root.children[0];
    assert_eq!(editor.kind, "editor");
    assert_eq!(editor.document, None);
    assert_eq!(editor.read_only, Some(false));
    // The new tab has focus, so the editor is the focused and status view.
    assert_eq!(info.focused_view.as_ref(), Some(&editor.target));
    assert_eq!(info.status_view, info.focused_view);

    let text = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let tab = workbench.focused_pane().unwrap().active_tab();
        let composite = workbench.composite_view(tab).unwrap();
        composite.read(cx).editor().read(cx).model().read(cx).text()
    });
    assert_eq!(text, "one\ntwo\n");
}

#[gpui::test]
fn the_tab_title_icon_and_resource_follow_updates(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open", cx);
    let shown = tab_state(&window, cx);
    assert_eq!(shown.title, "Example");
    assert_eq!(
        shown.icon,
        Some(knot_protocol::IconRef::Name("file".into()))
    );
    assert_eq!(shown.resource, None);

    run(&window, native_window, "composite.update", cx);
    let shown = tab_state(&window, cx);
    assert_eq!(shown.title, "Renamed");
    assert_eq!(
        shown.icon,
        Some(knot_protocol::IconRef::Name("file".into()))
    );
    assert_eq!(
        shown.resource.as_ref().map(ToString::to_string).as_deref(),
        Some("file:///a.txt")
    );

    run(&window, native_window, "composite.clear", cx);
    let shown = tab_state(&window, cx);
    assert_eq!((shown.icon, shown.resource), (None, None));
}

#[gpui::test]
fn closing_disposing_and_unloading_end_the_tab(cx: &mut TestAppContext) {
    let (window, native_window, host, key) = window_with_extension(cx);
    run(&window, native_window, "composite.open", cx);
    // A tab the package closes itself, from the document's tab since a
    // command captured in the composite would end with it.
    focus_document_tab(&window, cx);
    run(&window, native_window, "composite.close", cx);
    assert_eq!(composite_tabs(&window, cx), 0);

    // Disposing the registration closes its instances and frees the kind.
    run(&window, native_window, "composite.open", cx);
    assert_eq!(composite_tabs(&window, cx), 1);
    focus_document_tab(&window, cx);
    run(&window, native_window, "composite.dispose-registration", cx);
    assert_eq!(composite_tabs(&window, cx), 0);

    run(&window, native_window, "composite.open", cx);
    assert_eq!(composite_tabs(&window, cx), 1);
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
    cx.run_until_parked();
    assert_eq!(composite_tabs(&window, cx), 0);
}

#[gpui::test]
fn a_buffer_that_is_not_the_packages_is_a_type_error(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.foreign-buffer", cx);
}
