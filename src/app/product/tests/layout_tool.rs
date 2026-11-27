//! Layout tool windows: an embedded tree, an editor with a placeholder, and
//! controls that run commands, which the package replaces per window.

use super::*;
use crate::app::layout_view::LayoutView;
use knot_protocol::ExtensionKey;

const SOURCE: &str = r#"
import { buffers, commands, windows, workbench } from "knot";
const check = (condition, message) => { if (!condition) throw new Error(message); };
const rejected = async (operation, message) => {
  try { await operation(); } catch (error) {
    check(error instanceof TypeError, `${message}: ${error}`);
    return;
  }
  throw new Error(message);
};
const expectCode = async (operation, code) => {
  try { await operation(); } catch (error) { if (error.code === code) return; throw error; }
  throw new Error(`expected ${code}`);
};
const files = await workbench.registerView({
  id: "layout.files", title: "Files", embedded: true,
  content: { kind: "tree", provider: { getChildren: () => [{ id: "a", label: "Alpha", collapsibleState: "none" }] } },
});
const tool = await workbench.registerView({
  id: "layout.tool", title: "Layout", content: { kind: "layout" },
  placement: { location: "rightTop" },
});
const plain = await workbench.registerView({
  id: "layout.plain", title: "Plain", content: { kind: "text" },
  placement: { location: "bottomLeft" },
});
let message;
let clicks = [];
const layoutOf = (checked, placeholder) => ({ type: "column", children: [
  { type: "tree", key: "files", view: files },
  { type: "editor", key: "message", buffer: message, placeholder, gutter: placeholder === "Message" },
  { type: "controls", items: [
    { type: "checkbox", label: "Amend", checked, command: "layout.click", arguments: "amend" },
    { type: "button", label: "Commit", primary: true, command: "layout.click", arguments: "commit" },
    { type: "button", label: "Off", disabled: true, command: "layout.click", arguments: "off" },
  ] },
] });
const button = { label: "Output", icon: "file", command: "layout.click", arguments: "output" };
await commands.register("layout.click", async (context) => {
  clicks.push(context.target.focusedView !== null ? context.arguments : "unfocused");
});
await commands.register("layout.expect-none", async () => {
  check(clicks.length === 0, `no click: ${JSON.stringify(clicks)}`);
});
await commands.register("layout.expect-amend-commit-output", async () => {
  check(JSON.stringify(clicks) === '["amend","commit","output"]', `clicks: ${JSON.stringify(clicks)}`);
});
await commands.register("layout.show", async () => {
  message = await buffers.create();
  await tool.update({ layout: layoutOf(false, "Message"), toolbar: [button] });
});
await commands.register("layout.relayout", async () => {
  await tool.update({ layout: layoutOf(true, "Changed") });
});
await commands.register("layout.clear-toolbar", async () => {
  await tool.update({ toolbar: [] });
});
await commands.register("layout.show-here", async () => {
  message = await buffers.create();
  const [window] = await windows.list();
  await tool.update({ layout: layoutOf(false, "Here"), toolbar: [] }, { window: window.target });
});
await commands.register("layout.invalid", async () => {
  message = await buffers.create();
  await rejected(() => tool.update(), "no changes");
  await rejected(() => tool.update({ layout: { type: "row", children: [] } }), "no view");
  await rejected(() => tool.update({ layout: { type: "row", children: [{ type: "editor", buffer: {} }] } }), "foreign buffer");
  await rejected(() => tool.update({ toolbar: "x" }), "toolbar not a list");
  await rejected(() => tool.update({ layout: { type: "row", children: [
    { type: "text", view: plain },
  ] } }), "view not embedded");
  await rejected(() => workbench.registerView({
    id: "layout.bad", title: "Bad", content: { kind: "layout" }, embedded: true,
  }), "embedded layout");
  await rejected(() => workbench.registerView({
    id: "layout.bad", title: "Bad", content: { kind: "layout" }, language: "plaintext",
  }), "language on a layout");
  await expectCode(() => plain.update({ layout: layoutOf(false) }), "ContentKindMismatch");
});
"#;

fn load(
    host: &Entity<crate::app::extensions::host::ProductExtensionHost>,
    cx: &mut TestAppContext,
) -> ExtensionKey {
    let key = ExtensionKey::new(ExtensionId::new(971), ExtensionLifecycleId::new(1));
    let loaded = Arc::new(Mutex::new(None));
    let result = loaded.clone();
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, SOURCE).unwrap();
        cx.spawn(async move |_, _| {
            *result.lock().unwrap() = Some(execution.await.map(|_| ()));
        })
        .detach();
    });
    wait_for_api_condition(cx, |_| loaded.lock().unwrap().is_some());
    loaded.lock().unwrap().take().unwrap().unwrap();
    key
}

fn window_with_layout_tool(cx: &mut TestAppContext) -> (Entity<KnotWindow>, gpui::AnyWindowHandle) {
    let documents = install_globals(cx);
    let host = cx.update(crate::app::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    load(&host, cx);
    cx.refresh().unwrap();
    cx.run_until_parked();
    wait_for_api_condition(cx, |cx| {
        window
            .read(cx)
            .contributed_views
            .contains_key("layout.tool")
    });
    cx.run_until_parked();
    (window, native_window)
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

fn tool(window: &Entity<KnotWindow>, cx: &TestAppContext) -> Entity<LayoutView> {
    cx.read(|cx| {
        window.read(cx).contributed_views["layout.tool"]
            .layout()
            .clone()
    })
}

/// The ids of the views of the tool window's layout.
fn leaf_ids(window: &Entity<KnotWindow>, cx: &TestAppContext) -> Vec<gpui::EntityId> {
    let view = tool(window, cx);
    cx.read(|cx| {
        view.read(cx)
            .leaves()
            .iter()
            .map(crate::app::view_layout::Leaf::entity_id)
            .collect()
    })
}

fn shows_gutter(window: &Entity<KnotWindow>, cx: &TestAppContext) -> bool {
    let view = tool(window, cx);
    cx.read(|cx| {
        view.read(cx).leaves()[1]
            .editor()
            .unwrap()
            .read(cx)
            .shows_gutter()
    })
}

fn placeholder(window: &Entity<KnotWindow>, cx: &TestAppContext) -> Option<String> {
    let view = tool(window, cx);
    cx.read(|cx| {
        let leaves = view.read(cx).leaves();
        leaves[1]
            .editor()
            .unwrap()
            .read(cx)
            .placeholder()
            .map(str::to_owned)
    })
}

#[gpui::test]
fn a_layout_tool_window_is_empty_until_updated_and_then_hosts_its_views(cx: &mut TestAppContext) {
    let (window, native_window) = window_with_layout_tool(cx);
    assert!(leaf_ids(&window, cx).is_empty());

    run(&window, native_window, "layout.show", cx);
    assert_eq!(leaf_ids(&window, cx).len(), 2);
    assert_eq!(placeholder(&window, cx).as_deref(), Some("Message"));
    assert!(shows_gutter(&window, cx));
    let view = tool(&window, cx);
    cx.read(|cx| {
        let view = view.read(cx);
        assert_eq!(view.toolbar().len(), 1);
        // The embedded tree is bound to its definition in this window.
        let hosted = view.hosted_views();
        assert_eq!(hosted.len(), 1);
        assert_eq!(hosted[0].kind(cx), "layout.files");
    });
}

#[gpui::test]
fn an_update_keeps_the_views_of_keyed_members_and_changes_the_rest(cx: &mut TestAppContext) {
    let (window, native_window) = window_with_layout_tool(cx);
    run(&window, native_window, "layout.show", cx);
    let before = leaf_ids(&window, cx);
    run(&window, native_window, "layout.relayout", cx);
    assert_eq!(leaf_ids(&window, cx), before);
    assert_eq!(placeholder(&window, cx).as_deref(), Some("Changed"));
    assert!(!shows_gutter(&window, cx));
    // A toolbar left out of an update stays; `[]` removes it.
    let view = tool(&window, cx);
    cx.read(|cx| assert_eq!(view.read(cx).toolbar().len(), 1));
    run(&window, native_window, "layout.clear-toolbar", cx);
    cx.read(|cx| assert!(view.read(cx).toolbar().is_empty()));
}

#[gpui::test]
fn controls_and_toolbar_buttons_run_their_commands_with_the_tool_window_as_target(
    cx: &mut TestAppContext,
) {
    use super::composite::click_toolbar_button;

    let (window, native_window) = window_with_layout_tool(cx);
    run(&window, native_window, "layout.show", cx);
    // A disabled button does nothing.
    click_toolbar_button(native_window, "control-2", cx);
    run(&window, native_window, "layout.expect-none", cx);
    click_toolbar_button(native_window, "control-0", cx);
    click_toolbar_button(native_window, "control-1", cx);
    click_toolbar_button(native_window, "layout-toolbar-0", cx);
    run(
        &window,
        native_window,
        "layout.expect-amend-commit-output",
        cx,
    );
}

#[gpui::test]
fn an_update_without_a_window_is_retained_for_windows_created_later(cx: &mut TestAppContext) {
    let (window, native_window) = window_with_layout_tool(cx);
    run(&window, native_window, "layout.show-here", cx);
    assert_eq!(placeholder(&window, cx).as_deref(), Some("Here"));
    run(&window, native_window, "layout.show", cx);

    let documents = cx.read(|cx| {
        cx.global::<crate::app::documents::ApplicationDocuments>()
            .0
            .clone()
    });
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (second, _) = product_window(document, model, cx);
    wait_for_api_condition(cx, |cx| {
        second
            .read(cx)
            .contributed_views
            .contains_key("layout.tool")
    });
    cx.run_until_parked();
    assert_eq!(leaf_ids(&second, cx).len(), 2);
    assert_eq!(placeholder(&second, cx).as_deref(), Some("Message"));
}

#[gpui::test]
fn an_update_to_one_window_is_not_retained_for_others(cx: &mut TestAppContext) {
    let (window, native_window) = window_with_layout_tool(cx);
    run(&window, native_window, "layout.show-here", cx);

    let documents = cx.read(|cx| {
        cx.global::<crate::app::documents::ApplicationDocuments>()
            .0
            .clone()
    });
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (second, _) = product_window(document, model, cx);
    wait_for_api_condition(cx, |cx| {
        second
            .read(cx)
            .contributed_views
            .contains_key("layout.tool")
    });
    cx.run_until_parked();
    assert!(leaf_ids(&second, cx).is_empty());
    assert_eq!(leaf_ids(&window, cx).len(), 2);
}

#[gpui::test]
fn invalid_updates_and_registrations_are_rejected(cx: &mut TestAppContext) {
    let (window, native_window) = window_with_layout_tool(cx);
    run(&window, native_window, "layout.invalid", cx);
    assert!(leaf_ids(&window, cx).is_empty());
}
