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
  instance = await registration.open({
    title: "Example",
    icon: "file",
    layout: { type: "row", children: [{ type: "editor", buffer }] },
  });
});
// A column of an editor and a row whose editors have fixed and weighted sizes;
// the layout names the last editor as focused.
await commands.register("composite.open-nested", async () => {
  const kind = registration ?? (registration = await workbench.registerComposite({ id: "example.view" }));
  const [a, b, c] = await Promise.all(["a\n", "b\n", "c\n"].map((text) => buffers.create({ text })));
  instance = await kind.open({
    title: "Nested",
    layout: {
      type: "column",
      children: [
        { type: "editor", buffer: a, size: { weight: 2 } },
        {
          type: "row",
          size: { pixels: 120 },
          children: [
            { type: "editor", buffer: b },
            { type: "editor", buffer: c, size: { pixels: 200 }, focused: true },
          ],
        },
      ],
    },
  });
});
// The same shape without a `focused` member.
await commands.register("composite.open-default", async () => {
  const kind = registration ?? (registration = await workbench.registerComposite({ id: "example.view" }));
  const [a, b, c] = await Promise.all(["a\n", "b\n", "c\n"].map((text) => buffers.create({ text })));
  instance = await kind.open({
    title: "Default",
    layout: {
      type: "column",
      children: [
        { type: "row", children: [{ type: "editor", buffer: a }, { type: "editor", buffer: b }] },
        { type: "editor", buffer: c },
      ],
    },
  });
});
await commands.register("composite.invalid-layouts", async () => {
  const kind = registration ?? (registration = await workbench.registerComposite({ id: "example.view" }));
  const buffer = await buffers.create();
  const editor = { type: "editor", buffer };
  const rejected = async (layout, message) => {
    try { await kind.open({ title: "x", layout }); } catch (error) {
      check(error instanceof TypeError, message);
      return;
    }
    throw new Error(message);
  };
  await rejected(undefined, "missing layout");
  await rejected(editor, "root editor");
  await rejected({ type: "row", children: [] }, "no editor");
  await rejected({ type: "row", children: [{ type: "column", children: [] }] }, "no editor deep");
  await rejected({ type: "row", children: [{ ...editor, focused: true }, { ...editor, focused: true }] }, "two focused");
  await rejected({ type: "row", children: [{ ...editor, size: { weight: 0 } }] }, "zero weight");
  await rejected({ type: "row", children: [{ ...editor, size: { weight: 1, pixels: 1 } }] }, "two size kinds");
  await rejected({ type: "tabs", children: [editor] }, "unknown type");
  await rejected({ type: "row", children: [{ type: "editor", buffer: {} }] }, "foreign buffer");
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
            .resolve("composite.invalid-layouts")
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
        composite.read(cx).editors()[0]
            .read(cx)
            .model()
            .read(cx)
            .text()
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

fn composite_root(info: &knot_protocol::WindowInfo) -> &knot_protocol::ViewNodeInfo {
    &info
        .panes
        .iter()
        .flat_map(|pane| &pane.tabs)
        .find(|tab| tab.kind == SurfaceKind::Composite)
        .unwrap()
        .root
}

/// Move focus into the `index`th editor of the focused composite, as a click
/// in it would: the test window has no platform to report focus.
fn focus_editor(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    index: usize,
    cx: &mut TestAppContext,
) {
    let view = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let tab = workbench.focused_pane().unwrap().active_tab();
        workbench.composite_view(tab).unwrap().clone()
    });
    let handle = cx.read(|cx| view.read(cx).editors()[index].read(cx).focus_handle(cx));
    view.update(cx, |view, cx| view.set_focused_editor(index, cx));
    cx.update_window(native_window, |_, native_window, cx| {
        handle.focus(native_window, cx);
        focus_view(&handle, native_window, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

/// The text of the editor that commands act on in the focused tab.
fn command_text(window: &Entity<KnotWindow>, cx: &TestAppContext) -> String {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        match workbench.focused_command_view().unwrap() {
            crate::app::workbench::CommandView::Editor(editor) => {
                editor.read(cx).model().read(cx).text()
            }
            _ => panic!("a composite's command view is an editor"),
        }
    })
}

#[gpui::test]
fn rows_and_columns_nest_as_composite_views_and_the_layout_names_the_focus(
    cx: &mut TestAppContext,
) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-nested", cx);
    let info = info(&window, cx);
    let root = composite_root(&info);
    assert_eq!(root.kind, "composite");
    assert_eq!(root.children.len(), 2);
    assert_eq!(root.children[0].kind, "editor");
    let nested = &root.children[1];
    assert_eq!(nested.kind, "composite");
    assert_eq!(
        nested
            .children
            .iter()
            .map(|c| c.kind.as_str())
            .collect::<Vec<_>>(),
        ["editor", "editor"]
    );
    // Every view has its own identity.
    let mut identities = vec![&root.target, &root.children[0].target, &nested.target];
    identities.extend(nested.children.iter().map(|c| &c.target));
    for (index, identity) in identities.iter().enumerate() {
        assert!(!identities[..index].contains(identity));
    }
    // The layout names the last editor, which is where the caret is.
    assert_eq!(info.focused_view.as_ref(), Some(&nested.children[1].target));
    assert_eq!(command_text(&window, cx), "c\n");
}

#[gpui::test]
fn the_first_editor_has_focus_when_the_layout_names_none(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-default", cx);
    let info = info(&window, cx);
    let first = &composite_root(&info).children[0].children[0];
    assert_eq!(first.kind, "editor");
    assert_eq!(info.focused_view.as_ref(), Some(&first.target));
    assert_eq!(command_text(&window, cx), "a\n");
}

#[gpui::test]
fn focus_moves_between_editors_and_reselecting_the_tab_restores_the_last(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-nested", cx);
    focus_editor(&window, native_window, 1, cx);
    let info_now = info(&window, cx);
    let root = composite_root(&info_now);
    assert_eq!(
        info_now.focused_view.as_ref(),
        Some(&root.children[1].children[0].target)
    );
    assert_eq!(command_text(&window, cx), "b\n");

    focus_editor(&window, native_window, 0, cx);
    let info_now = info(&window, cx);
    assert_eq!(
        info_now.focused_view.as_ref(),
        Some(&composite_root(&info_now).children[0].target)
    );

    // Leave the tab and select it again.
    focus_document_tab(&window, cx);
    assert_ne!(command_text(&window, cx), "a\n");
    window.update(cx, |window, cx| {
        window.workbench.update(cx, |workbench, _| {
            let pane = workbench.focused_pane_id().unwrap();
            let composite = workbench
                .tabs()
                .find(|tab| matches!(tab.surface_id(), TabSurfaceId::Composite(_)))
                .unwrap()
                .id();
            workbench.activate_tab(pane, composite);
        });
        window.focus_active_surface(cx);
    });
    cx.run_until_parked();
    let info_now = info(&window, cx);
    assert_eq!(
        info_now.focused_view.as_ref(),
        Some(&composite_root(&info_now).children[0].target)
    );
    assert_eq!(command_text(&window, cx), "a\n");
}

#[gpui::test]
fn a_layout_that_is_not_valid_is_a_type_error(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.invalid-layouts", cx);
    assert_eq!(composite_tabs(&window, cx), 0);
}
