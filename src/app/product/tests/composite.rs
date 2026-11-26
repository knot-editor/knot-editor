//! Composite views: opening, the view tree, tab updates, and closing.
use super::*;
use knot_model::workbench::TabSurfaceId;
use knot_protocol::SurfaceKind;

const SOURCE: &str = r#"
import { buffers, commands, documents, workbench } from "knot";
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
// A tree and a text frame beside tabs of an editor and another text frame;
// the layout names the log tab as focused when `which` says so.
let embedded;
const openViews = async (focusLog) => {
  const kind = registration ?? (registration = await workbench.registerComposite({ id: "example.view" }));
  if (embedded === undefined) {
    const tree = await workbench.registerView({
      id: "example.files",
      title: "Files",
      embedded: true,
      content: {
        kind: "tree",
        provider: { getChildren: () => [
          { id: "a", label: "Alpha", collapsibleState: "none" },
          { id: "b", label: "Beta", collapsibleState: "none" },
        ] },
      },
    });
    const log = await workbench.registerView({ id: "example.log", title: "Log", embedded: true, content: { kind: "text" } });
    await log.setText("log text\n");
    embedded = { tree, log };
  }
  const buffer = await buffers.create({ text: "code\n" });
  instance = await kind.open({
    title: "Views",
    layout: {
      type: "row",
      children: [
        { type: "tree", view: embedded.tree, size: { pixels: 160 } },
        {
          type: "tabs",
          children: [
            { type: "editor", title: "Code", buffer },
            { type: "text", title: "Log", view: embedded.log, focused: focusLog },
          ],
        },
      ],
    },
  });
};
await commands.register("composite.open-views", () => openViews(false));
await commands.register("composite.open-views-log", () => openViews(true));
await commands.register("composite.capture", async (context) => {
  check(context.editor !== null && context.buffer !== null, "editor and buffer captured");
  check(context.target.focusedView !== null, "focused view");
});
// A toolbar whose buttons run a command that records what it was given; the
// second button is disabled.
const button = { label: "Press", icon: "arrow-down", command: "composite.toolbar-press", arguments: { n: 1 } };
let pressed = [];
await commands.register("composite.toolbar-press", async (context) => {
  pressed.push(
    context.editor !== null && context.target.viewChain.length === 1 && context.arguments.n === 1,
  );
});
await commands.register("composite.toolbar-expect-one", async () => {
  check(pressed.length === 1 && pressed[0], `one press with the composite as target: ${pressed}`);
});
await commands.register("composite.toolbar-expect-none", async () => {
  check(pressed.length === 0, `no press: ${pressed}`);
});
await commands.register("composite.open-toolbar", async () => {
  const kind = registration ?? (registration = await workbench.registerComposite({ id: "example.view" }));
  const buffer = await buffers.create({ text: "one\n" });
  instance = await kind.open({
    title: "Toolbar",
    layout: { type: "row", children: [{ type: "editor", buffer }] },
    toolbar: [button, { ...button, label: "Off", icon: "lock", disabled: true }],
  });
});
await commands.register("composite.toolbar-toggle", async () => {
  await instance.update({ toolbar: [{ ...button, toggled: true }] });
});
await commands.register("composite.toolbar-clear", async () => {
  await instance.update({ toolbar: [] });
});
// Editors on the first document, twice.
await commands.register("composite.open-document", async () => {
  const kind = registration ?? (registration = await workbench.registerComposite({ id: "example.view" }));
  const [document] = await documents.list();
  instance = await kind.open({
    title: "Document",
    layout: { type: "row", children: [
      { type: "editor", document: document.id },
      { type: "editor", document: document.id },
    ] },
  });
});
// Layout updates: a member named by a key keeps its view.
let keyed;
const keyedKind = async () => registration ?? (registration = await workbench.registerComposite({ id: "example.view" }));
await commands.register("composite.open-keyed", async () => {
  const kind = await keyedKind();
  const [a, b, c, d] = await Promise.all(["a\n", "b\n", "c\n", "d\n"].map((text) => buffers.create({ text })));
  keyed = { a, b, c, d };
  instance = await kind.open({
    title: "Keyed",
    layout: { type: "column", children: [
      { type: "editor", key: "a", buffer: a },
      { type: "row", key: "pair", children: [
        { type: "editor", key: "b", buffer: b },
        { type: "editor", key: "c", buffer: c, focused: true },
      ] },
    ] },
  });
});
// `a` and `c` stay in new places, `pair` stays as a column, `b` goes and `d`
// is new; `d` asks for focus, which stays where it was.
await commands.register("composite.relayout", async () => {
  const { a, c, d } = keyed;
  await instance.update({ layout: { type: "row", children: [
    { type: "editor", key: "c", buffer: c },
    { type: "column", key: "pair", children: [
      { type: "editor", key: "a", buffer: a },
      { type: "editor", key: "d", buffer: d, focused: true },
    ] },
  ] } });
});
// The focused `c` goes; `d` is the layout's initial focus.
await commands.register("composite.relayout-drop-focused", async () => {
  const { a, d } = keyed;
  await instance.update({ layout: { type: "row", children: [
    { type: "editor", key: "a", buffer: a },
    { type: "editor", key: "d", buffer: d, focused: true },
  ] } });
});
await commands.register("composite.relayout-drop-focused-default", async () => {
  const { a, d } = keyed;
  await instance.update({ layout: { type: "row", children: [
    { type: "editor", key: "d", buffer: d },
    { type: "editor", key: "a", buffer: a },
  ] } });
});
// `a` keeps its key and shows another buffer; nothing else has a key.
await commands.register("composite.relayout-replace", async () => {
  const { b, d } = keyed;
  await instance.update({ layout: { type: "row", children: [
    { type: "editor", key: "a", buffer: d },
    { type: "editor", buffer: b },
  ] } });
});
await commands.register("composite.relayout-invalid", async () => {
  const { a, b } = keyed;
  const rejected = async (children, message) => {
    try { await instance.update({ layout: { type: "row", children } }); } catch (error) {
      check(error instanceof TypeError, message);
      return;
    }
    throw new Error(message);
  };
  await rejected([{ type: "editor", key: "x", buffer: a }, { type: "editor", key: "x", buffer: b }], "duplicate key");
  await rejected([{ type: "editor", key: "", buffer: a }], "empty key");
  await rejected([{ type: "editor", key: 1, buffer: a }], "key not text");
  await rejected([{ type: "row", key: "g", children: [{ type: "editor", key: "g", buffer: a }] }], "group key reused");
  await rejected([{ type: "row", children: [] }], "no view");
});
// Tabs `x` and `y`, of which `y` is shown after a click; reordered.
await commands.register("composite.open-keyed-tabs", async () => {
  const kind = await keyedKind();
  const [a, b] = await Promise.all(["x\n", "y\n"].map((text) => buffers.create({ text })));
  keyed = { a, b };
  instance = await kind.open({
    title: "Keyed tabs",
    layout: { type: "row", children: [{ type: "tabs", key: "t", children: [
      { type: "editor", key: "x", title: "X", buffer: a },
      { type: "editor", key: "y", title: "Y", buffer: b },
    ] }] },
  });
});
await commands.register("composite.relayout-tabs", async () => {
  const { a, b } = keyed;
  await instance.update({ layout: { type: "row", children: [{ type: "tabs", key: "t", children: [
    { type: "editor", key: "y", title: "Y", buffer: b },
    { type: "editor", key: "x", title: "X", buffer: a },
  ] }] } });
});
// A tree and a text frame; the update keeps the tree, drops the text frame,
// and adds an editor.
await commands.register("composite.open-keyed-views", async () => {
  const kind = await keyedKind();
  const tree = await workbench.registerView({
    id: "example.keyed-files", title: "Files", embedded: true,
    content: { kind: "tree", provider: { getChildren: () => [{ id: "a", label: "Alpha", collapsibleState: "none" }] } },
  });
  const log = await workbench.registerView({ id: "example.keyed-log", title: "Log", embedded: true, content: { kind: "text" } });
  const buffer = await buffers.create({ text: "code\n" });
  keyed = { tree, log, buffer };
  instance = await kind.open({
    title: "Keyed views",
    layout: { type: "row", children: [
      { type: "tree", key: "files", view: tree },
      { type: "text", key: "log", view: log },
    ] },
  });
});
await commands.register("composite.relayout-views", async () => {
  const { tree, buffer } = keyed;
  await instance.update({ layout: { type: "column", children: [
    { type: "editor", buffer },
    { type: "tree", key: "files", view: tree },
  ] } });
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
  await rejected({ type: "grid", children: [editor] }, "unknown type");
  await rejected({ type: "tabs", children: [{ ...editor, title: "A" }] }, "root tabs");
  await rejected({ type: "row", children: [{ type: "tabs", children: [editor] }] }, "tab without title");
  await rejected({ type: "row", children: [{ ...editor, title: "A" }] }, "title outside tabs");
  const plain = await workbench.registerView({ id: "example.plain", title: "Plain", content: { kind: "text" } });
  await rejected({ type: "row", children: [{ type: "text", view: plain }] }, "view not embedded");
  const tree = await workbench.registerView({
    id: "example.other-tree", title: "T", embedded: true,
    content: { kind: "tree", provider: { getChildren: () => [] } },
  });
  await rejected({ type: "row", children: [{ type: "text", view: tree }] }, "wrong content kind");
  await rejected({ type: "row", children: [{ type: "editor", buffer: {} }] }, "foreign buffer");
  const [document] = await documents.list();
  const scratch = await buffers.create({ text: "x" });
  await rejected({ type: "row", children: [{ type: "editor", buffer: scratch, document: document.id }] }, "buffer and document");
  await rejected({ type: "row", children: [{ type: "editor", document: 5 }] }, "document not an ID");
  await expectCode(
    () => kind.open({ title: "x", layout: { type: "row", children: [{ type: "editor", document: "nope" }] } }),
    "DocumentNotFound",
  );
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
// Two editors of a hundred lines, the second with ten more at line 10, linked.
const hundred = (from, to) => Array.from({ length: to - from }, (_, line) => `line ${from + line}\n`).join("");
// An invalid link is a TypeError, which has no code, or a host error with one.
const expectLinkError = async (operation, code) => {
  try { await operation(); } catch (error) {
    if (code === undefined ? error instanceof TypeError : error.code === code) return;
    throw error;
  }
  throw new Error(`expected ${code ?? "TypeError"}`);
};
const insertion = [{ first: { start: 10, end: 10 }, second: { start: 10, end: 20 } }];
await commands.register("composite.open-linked", async () => {
  const kind = await keyedKind();
  const first = await buffers.create({ text: hundred(0, 100) });
  const second = await buffers.create({ text: hundred(0, 10) + hundred(200, 210) + hundred(10, 100) });
  keyed = { first, second };
  instance = await kind.open({
    title: "Linked",
    layout: { type: "row", children: [
      { type: "editor", key: "first", buffer: first },
      { type: "column", key: "group", children: [{ type: "editor", key: "second", buffer: second }] },
    ] },
    scrollLink: { first: "first", second: "second", map: insertion },
  });
});
await commands.register("composite.link-map", async () => {
  await instance.update({ scrollLink: { first: "first", second: "second", map: [] } });
});
await commands.register("composite.unlink", async () => {
  await instance.update({ scrollLink: null });
});
await commands.register("composite.link-kept-by-other-updates", async () => {
  await instance.update({ title: "Renamed" });
});
await commands.register("composite.invalid-links", async () => {
  const link = (first, second, map = []) => ({ first, second, map });
  const bad = async (scrollLink, code) => {
    await expectLinkError(() => instance.update({ scrollLink }), code);
  };
  await bad(link("first", "first"), undefined);
  await bad(link("first", "nope"), "InvalidTarget");
  await bad(link("first", "second", [{ first: { start: 3, end: 2 }, second: { start: 0, end: 1 } }]), undefined);
  await bad(link("first", "second", [
    { first: { start: 0, end: 5 }, second: { start: 0, end: 5 } },
    { first: { start: 4, end: 6 }, second: { start: 5, end: 6 } },
  ]), undefined);
  // The group's key names a member that is not an editor.
  await bad(link("first", "group"), "InvalidTarget");
  // A layout without a linked key is refused while the link stays.
  const scratch = await buffers.create();
  await expectLinkError(() => instance.update({ layout: { type: "row", children: [
    { type: "editor", key: "first", buffer: keyed.first },
    { type: "editor", buffer: scratch },
  ] } }), "InvalidTarget");
});
// Two editors with a connector between them.
const connected = (links) => ({ type: "row", children: [
  { type: "editor", key: "left", buffer: keyed.left },
  { type: "connector", first: "left", second: "right", links },
  { type: "editor", key: "right", buffer: keyed.right },
] });
const modified = { first: { start: 1, end: 2 }, second: { start: 1, end: 4 }, color: "diff.modified.background" };
await commands.register("composite.open-connected", async () => {
  const kind = await keyedKind();
  const [left, right] = await Promise.all([buffers.create({ text: "a\nb\n" }), buffers.create({ text: "a\nx\ny\nz\n" })]);
  keyed = { left, right };
  instance = await kind.open({ title: "Connected", layout: connected([modified]) });
});
await commands.register("composite.reconnect", async () => {
  await instance.update({ layout: connected([modified, { ...modified, color: "diff.added.background" }]) });
});
await commands.register("composite.invalid-connectors", async () => {
  const editors = [
    { type: "editor", key: "left", buffer: keyed.left },
    { type: "editor", key: "right", buffer: keyed.right },
  ];
  const bad = async (layout) => {
    await expectLinkError(() => instance.update({ layout: { type: "row", children: layout } }), undefined);
  };
  const connector = (first, second, extra = {}) => ({ type: "connector", first, second, links: [], ...extra });
  await bad([...editors, connector("left", "left")]);
  await bad([...editors, connector("left", "nope")]);
  await bad([...editors, connector("left", "right", { key: "strip" })]);
  await bad([...editors, connector("left", "right", { focused: true })]);
  await bad([{ type: "tabs", children: [
    { type: "editor", key: "left", title: "L", buffer: keyed.left },
    { type: "editor", key: "right", title: "R", buffer: keyed.right },
    connector("left", "right", { title: "C" }),
  ] }]);
  await bad([...editors, { ...connector("left", "right"), links: [{ first: { start: 2, end: 1 }, second: { start: 0, end: 1 }, color: "x" }] }]);
  await bad([...editors, { ...connector("left", "right"), links: [{ first: { start: 0, end: 1 }, second: { start: 0, end: 1 } }] }]);
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
            .resolve("composite.dispose-registration")
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

/// Run `name` from the document's tab and then select the composite's, as an
/// update that removes the view a command ran in would leave that command's
/// target gone.
fn run_elsewhere(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    name: &'static str,
    cx: &mut TestAppContext,
) {
    focus_document_tab(window, cx);
    run(window, native_window, name, cx);
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
        composite.read(cx).leaves()[0]
            .editor()
            .unwrap()
            .read(cx)
            .model()
            .read(cx)
            .text()
    });
    assert_eq!(text, "one\ntwo\n");
}

#[gpui::test]
fn an_extension_opens_editors_on_a_document_and_the_composite_is_one_view_of_it(
    cx: &mut TestAppContext,
) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-document", cx);
    let document = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        workbench.tabs().next().unwrap().document_id().unwrap()
    });
    cx.read(|cx| {
        assert_eq!(window.read(cx).workbench.read(cx).view_count(document), 2);
    });
    let info = info(&window, cx);
    let tab = info
        .panes
        .iter()
        .flat_map(|pane| &pane.tabs)
        .find(|tab| tab.kind == SurfaceKind::Composite)
        .unwrap();
    assert_eq!(tab.root.children.len(), 2);
    for editor in &tab.root.children {
        assert_eq!(editor.kind, "editor");
        assert!(editor.document.is_some());
        assert_eq!(editor.read_only, Some(false));
    }
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
fn focus_leaf(
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
    let handle = cx.read(|cx| view.read(cx).leaves()[index].focus_handle(cx));
    view.update(cx, |view, cx| view.set_focused_leaf(index, cx));
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
    focus_leaf(&window, native_window, 1, cx);
    let info_now = info(&window, cx);
    let root = composite_root(&info_now);
    assert_eq!(
        info_now.focused_view.as_ref(),
        Some(&root.children[1].children[0].target)
    );
    assert_eq!(command_text(&window, cx), "b\n");
    // A command captures the focused editor, as find needs.
    run(&window, native_window, "composite.capture", cx);

    focus_leaf(&window, native_window, 0, cx);
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

/// The focused composite's view of the `index`th leaf in layout order.
fn leaf_of(
    window: &Entity<KnotWindow>,
    index: usize,
    cx: &TestAppContext,
) -> crate::app::composite_view::Leaf {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let tab = workbench.focused_pane().unwrap().active_tab();
        workbench.composite_view(tab).unwrap().read(cx).leaves()[index].clone()
    })
}

/// Show the `index`th member of the composite's only tabs group and move
/// focus into it, as clicking its title would.
fn select_tab(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    index: usize,
    cx: &mut TestAppContext,
) {
    // The tabs follow the tree and the editor in layout order.
    select_tab_in(window, native_window, 0, index, cx);
}

/// Show the `index`th member of the composite's `group`th tabs group and move
/// focus into it; the group's members are the first views in layout order,
/// after `group`'s... one view when the group follows a tree.
fn select_tab_in(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    group: usize,
    index: usize,
    cx: &mut TestAppContext,
) {
    let view = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let tab = workbench.focused_pane().unwrap().active_tab();
        workbench.composite_view(tab).unwrap().clone()
    });
    view.update(cx, |view, cx| view.select_tab(group, index, cx))
        .unwrap();
    let before = if group == 0 && cx.read(|cx| view.read(cx).leaves().len()) > 2 {
        1
    } else {
        0
    };
    focus_leaf(window, native_window, before + index, cx);
}

fn command_view_kind(window: &Entity<KnotWindow>, cx: &TestAppContext) -> &'static str {
    use crate::app::workbench::{CommandView, ExtensionView};
    cx.read(|cx| {
        match window
            .read(cx)
            .workbench
            .read(cx)
            .focused_command_view()
            .unwrap()
        {
            CommandView::Editor(_) => "editor",
            CommandView::Extension(ExtensionView::Tree(_)) => "tree",
            CommandView::Extension(ExtensionView::Text(_)) => "text",
            CommandView::Terminal(_) => "terminal",
        }
    })
}

#[gpui::test]
fn trees_and_text_frames_are_views_beside_tabs_of_views(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-views", cx);
    // An embedded view has no tool window.
    assert!(
        cx.read(crate::app::extensions::host::view_definitions)
            .is_empty()
    );

    let info = info(&window, cx);
    let root = composite_root(&info);
    assert_eq!(root.children[0].kind, "tree");
    let tabs = &root.children[1];
    assert_eq!(tabs.kind, "composite");
    assert_eq!(
        tabs.children
            .iter()
            .map(|c| c.kind.as_str())
            .collect::<Vec<_>>(),
        ["editor", "text"]
    );
    // The first view has focus, and it is not an editor.
    assert_eq!(info.focused_view.as_ref(), Some(&root.children[0].target));
    assert_eq!(info.status_view, None);
    assert_eq!(command_view_kind(&window, cx), "tree");

    // The provider filled the tree, and the text frame has its text.
    let crate::app::composite_view::Leaf::Tree(tree) = leaf_of(&window, 0, cx) else {
        panic!("the first view is a tree");
    };
    assert_eq!(
        cx.read(|cx| tree.read(cx).visible_labels()),
        ["Alpha", "Beta"]
    );
    let crate::app::composite_view::Leaf::Text(text) = leaf_of(&window, 2, cx) else {
        panic!("the third view is a text frame");
    };
    assert_eq!(cx.read(|cx| text.read(cx).text(cx)), "log text\n");
}

#[gpui::test]
fn selecting_a_tab_shows_its_member_and_moves_focus_into_it(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-views", cx);

    select_tab(&window, native_window, 1, cx);
    let info_now = info(&window, cx);
    let tabs = &composite_root(&info_now).children[1];
    assert_eq!(
        info_now.focused_view.as_ref(),
        Some(&tabs.children[1].target)
    );
    assert_eq!(command_view_kind(&window, cx), "text");

    select_tab(&window, native_window, 0, cx);
    let info_now = info(&window, cx);
    let tabs = &composite_root(&info_now).children[1];
    assert_eq!(
        info_now.focused_view.as_ref(),
        Some(&tabs.children[0].target)
    );
    assert_eq!(info_now.status_view, info_now.focused_view);
    assert_eq!(command_view_kind(&window, cx), "editor");
    assert_eq!(command_text(&window, cx), "code\n");

    // A command captures the editor it ran in.
    run(&window, native_window, "composite.capture", cx);
}

#[gpui::test]
fn a_focused_member_of_tabs_is_shown_and_has_focus_on_open(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-views-log", cx);
    let info = info(&window, cx);
    let tabs = &composite_root(&info).children[1];
    assert_eq!(info.focused_view.as_ref(), Some(&tabs.children[1].target));
    assert_eq!(command_view_kind(&window, cx), "text");
}

#[gpui::test]
fn clicking_a_tab_title_shows_its_member_and_focuses_it(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-views", cx);
    cx.refresh().unwrap();
    let mut visual = gpui::VisualTestContext::from_window(native_window, cx);
    let title = visual.debug_bounds("composite-tab-1").unwrap().center();
    visual.simulate_mouse_down(title, gpui::MouseButton::Left, gpui::Modifiers::default());
    visual.simulate_mouse_up(title, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.run_until_parked();
    cx.refresh().unwrap();
    // The log tab shows its text, and its focus handle has focus.
    let text = leaf_of(&window, 2, cx);
    let focused = cx
        .update_window(native_window, |_, native_window, cx| {
            text.focus_handle(cx).is_focused(native_window)
        })
        .unwrap();
    assert!(focused);
}

/// Click the toolbar button of the debug selector `selector`.
fn click_toolbar_button(
    native_window: gpui::AnyWindowHandle,
    selector: &'static str,
    cx: &mut TestAppContext,
) {
    cx.refresh().unwrap();
    let mut visual = gpui::VisualTestContext::from_window(native_window, cx);
    let button = visual.debug_bounds(selector).unwrap().center();
    visual.simulate_mouse_down(button, gpui::MouseButton::Left, gpui::Modifiers::default());
    visual.simulate_mouse_up(button, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.run_until_parked();
}

#[gpui::test]
fn a_toolbar_button_runs_its_command_with_the_composite_as_target(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-toolbar", cx);
    let toolbar = tab_state(&window, cx).toolbar;
    assert_eq!(toolbar.len(), 2);
    assert!(toolbar[1].disabled && !toolbar[0].toggled);

    click_toolbar_button(native_window, "composite-toolbar-1", cx);
    run(&window, native_window, "composite.toolbar-expect-none", cx);
    click_toolbar_button(native_window, "composite-toolbar-0", cx);
    run(&window, native_window, "composite.toolbar-expect-one", cx);
}

#[gpui::test]
fn a_toolbar_button_targets_its_composite_when_focus_is_elsewhere(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-toolbar", cx);
    let view = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let tab = workbench.focused_pane().unwrap().active_tab();
        workbench.composite_view(tab).unwrap().clone()
    });
    focus_document_tab(&window, cx);
    let button = tab_state(&window, cx).toolbar[0].clone();
    cx.update(|cx| {
        crate::app::product::run_toolbar_command(
            native_window,
            view,
            knot_protocol::Command {
                name: button.command.as_str().into(),
                arguments: button.arguments,
            },
            cx,
        );
    });
    cx.run_until_parked();
    run(&window, native_window, "composite.toolbar-expect-one", cx);
}

#[gpui::test]
fn an_update_replaces_the_toolbar_and_other_updates_keep_it(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-toolbar", cx);
    run(&window, native_window, "composite.update", cx);
    assert_eq!(tab_state(&window, cx).toolbar.len(), 2);
    run(&window, native_window, "composite.toolbar-toggle", cx);
    let toolbar = tab_state(&window, cx).toolbar;
    assert_eq!(toolbar.len(), 1);
    assert!(toolbar[0].toggled);
    run(&window, native_window, "composite.toolbar-clear", cx);
    assert!(tab_state(&window, cx).toolbar.is_empty());
}

/// The focused composite's views in layout order, by identity.
fn leaf_ids(window: &Entity<KnotWindow>, cx: &TestAppContext) -> Vec<gpui::EntityId> {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let tab = workbench.focused_pane().unwrap().active_tab();
        workbench
            .composite_view(tab)
            .unwrap()
            .read(cx)
            .leaves()
            .iter()
            .map(crate::app::composite_view::Leaf::entity_id)
            .collect()
    })
}

/// The group views beneath the focused composite's root, by identity.
fn group_ids(info: &knot_protocol::WindowInfo) -> Vec<String> {
    fn walk(node: &knot_protocol::ViewNodeInfo, ids: &mut Vec<String>) {
        for child in &node.children {
            if child.kind == "composite" {
                ids.push(format!("{:?}", child.target));
            }
            walk(child, ids);
        }
    }
    let mut ids = Vec::new();
    walk(composite_root(info), &mut ids);
    ids
}

#[gpui::test]
fn an_update_keeps_the_views_of_kept_children_and_replaces_the_rest(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-keyed", cx);
    let [a, b, c] = leaf_ids(&window, cx)[..] else {
        panic!("three views");
    };
    let crate::app::composite_view::Leaf::Editor(view) = leaf_of(&window, 1, cx) else {
        panic!("the second view is an editor");
    };
    let weak_b = view.downgrade();
    drop(view);
    let groups = group_ids(&info(&window, cx));
    assert_eq!(groups.len(), 1);

    run(&window, native_window, "composite.relayout", cx);
    let now = leaf_ids(&window, cx);
    assert_eq!(now.len(), 3);
    // `c` and `a` kept their views in their new places; `d` is new.
    assert_eq!((now[0], now[1]), (c, a));
    assert!(!now.contains(&b) && now[2] != b);
    // `b` is gone.
    assert!(weak_b.upgrade().is_none());
    // The kept group is the same view, now holding two editors.
    assert_eq!(group_ids(&info(&window, cx)), groups);
    let info = info(&window, cx);
    let root = composite_root(&info);
    assert_eq!(
        root.children
            .iter()
            .map(|c| c.kind.as_str())
            .collect::<Vec<_>>(),
        ["editor", "composite"]
    );
    assert_eq!(root.children[1].children.len(), 2);
    // The buffers are the kept ones.
    let crate::app::composite_view::Leaf::Editor(new) = leaf_of(&window, 2, cx) else {
        panic!("the third view is an editor");
    };
    assert_eq!(cx.read(|cx| new.read(cx).model().read(cx).text()), "d\n");
}

#[gpui::test]
fn an_update_keeps_the_focus_where_it_was(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-keyed", cx);
    // `c` had focus on open, and the new layout's `d` asks for it.
    assert_eq!(command_text(&window, cx), "c\n");
    run(&window, native_window, "composite.relayout", cx);
    assert_eq!(command_text(&window, cx), "c\n");
    let info = info(&window, cx);
    assert_eq!(
        info.focused_view.as_ref(),
        Some(&composite_root(&info).children[0].target)
    );

    // Focus moved by the user is the focus kept.
    focus_leaf(&window, native_window, 1, cx);
    assert_eq!(command_text(&window, cx), "a\n");
    run(
        &window,
        native_window,
        "composite.relayout-drop-focused-default",
        cx,
    );
    assert_eq!(command_text(&window, cx), "a\n");
}

#[gpui::test]
fn removing_the_focused_child_moves_focus_to_the_layouts_initial_one(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-keyed", cx);
    assert_eq!(command_text(&window, cx), "c\n");
    run_elsewhere(
        &window,
        native_window,
        "composite.relayout-drop-focused",
        cx,
    );
    assert_eq!(command_text(&window, cx), "d\n");
    let info = info(&window, cx);
    assert_eq!(
        info.focused_view.as_ref(),
        Some(&composite_root(&info).children[1].target)
    );

    // Without a focused member the first view in layout order has it.
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-keyed", cx);
    run_elsewhere(
        &window,
        native_window,
        "composite.relayout-drop-focused-default",
        cx,
    );
    assert_eq!(command_text(&window, cx), "d\n");
}

#[gpui::test]
fn a_key_that_shows_another_buffer_and_unkeyed_members_are_new_views(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-keyed", cx);
    let before = leaf_ids(&window, cx);
    run_elsewhere(&window, native_window, "composite.relayout-replace", cx);
    let after = leaf_ids(&window, cx);
    assert_eq!(after.len(), 2);
    assert!(after.iter().all(|id| !before.contains(id)));
}

#[gpui::test]
fn an_invalid_update_changes_nothing(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-keyed", cx);
    let before = leaf_ids(&window, cx);
    run(&window, native_window, "composite.relayout-invalid", cx);
    assert_eq!(leaf_ids(&window, cx), before);
}

#[gpui::test]
fn tabs_keep_the_member_they_show_when_the_members_move(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-keyed-tabs", cx);
    let [x, y] = leaf_ids(&window, cx)[..] else {
        panic!("two views");
    };
    select_tab_in(&window, native_window, 0, 1, cx);
    assert_eq!(command_text(&window, cx), "y\n");
    run(&window, native_window, "composite.relayout-tabs", cx);
    assert_eq!(leaf_ids(&window, cx), [y, x]);
    // `y` is still the shown member and has focus.
    assert_eq!(command_text(&window, cx), "y\n");
    let info = info(&window, cx);
    let tabs = &composite_root(&info).children[0];
    assert_eq!(info.focused_view.as_ref(), Some(&tabs.children[0].target));
}

#[gpui::test]
fn contributed_views_are_kept_dropped_and_unbound_by_an_update(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-keyed-views", cx);
    let tree = leaf_of(&window, 0, cx);
    let crate::app::composite_view::Leaf::Text(log) = leaf_of(&window, 1, cx) else {
        panic!("the second view is a text frame");
    };
    let weak_log = log.downgrade();
    drop(log);
    run(&window, native_window, "composite.relayout-views", cx);
    let now = leaf_ids(&window, cx);
    assert_eq!(now.len(), 2);
    assert_eq!(now[1], tree.entity_id());
    assert!(weak_log.upgrade().is_none());
}

mod documents {
    //! Editors on documents make the composite a view of them.
    use super::*;
    use crate::app::commands::dispatch::CLOSE_TAB_COMMAND;
    use crate::app::composite_view::{
        Arrangement, CompositeTab, ContentSpec, GroupSpec, LayoutSize, MemberSpec,
    };
    use crate::app::documents::DocumentId;

    fn dirty(document: DocumentId, cx: &mut TestAppContext) {
        let model = cx.read(|cx| {
            KnotWindow::documents(cx)
                .read(cx)
                .get(document)
                .unwrap()
                .model()
                .clone()
        });
        model.update(cx, |model, cx| {
            model.replace(0..0, "text").unwrap();
            cx.notify();
        });
    }

    /// A window whose first tab shows document `a`, and a composite of
    /// editors on `a`, `a` again, and `b`, focused beside it.
    fn window_with_composite(
        cx: &mut TestAppContext,
    ) -> (
        Entity<KnotWindow>,
        gpui::AnyWindowHandle,
        DocumentId,
        DocumentId,
    ) {
        let documents = install_globals(cx);
        let a = cx.update(|cx| create_untitled_document(&documents, cx));
        let b = cx.update(|cx| create_untitled_document(&documents, cx));
        let model = |document: DocumentId, cx: &TestAppContext| {
            cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone())
        };
        let (a_model, b_model) = (model(a, cx), model(b, cx));
        let (window, native_window) = product_window(a, a_model.clone(), cx);
        let editor = |document, buffer: &Entity<crate::app::model::Buffer>| MemberSpec {
            key: None,
            size: LayoutSize::default(),
            focused: false,
            title: None,
            content: ContentSpec::Editor {
                buffer: buffer.clone(),
                document: Some(document),
            },
        };
        window.update(cx, |window, cx| {
            window.workbench.update(cx, |workbench, cx| {
                workbench.open_composite_in_center(
                    CompositeTab::default(),
                    GroupSpec {
                        arrangement: Arrangement::Row,
                        members: vec![
                            editor(a, &a_model),
                            editor(a, &a_model),
                            editor(b, &b_model),
                        ],
                    },
                    "window",
                    cx,
                );
            });
            window.focus_active_surface(cx);
        });
        cx.run_until_parked();
        (window, native_window, a, b)
    }

    fn marker(window: &Entity<KnotWindow>, cx: &TestAppContext) -> bool {
        cx.read(|cx| {
            let workbench = window.read(cx).workbench.read(cx);
            let tab = workbench
                .tabs()
                .find(|tab| matches!(tab.surface_id(), TabSurfaceId::Composite(_)))
                .unwrap();
            crate::app::product::render::composite_asks_on_close(workbench, tab, cx)
        })
    }

    #[gpui::test]
    fn a_composite_is_one_view_of_each_document_however_many_editors_it_holds(
        cx: &mut TestAppContext,
    ) {
        let (window, _, a, b) = window_with_composite(cx);
        cx.read(|cx| {
            let workbench = window.read(cx).workbench.read(cx);
            assert_eq!(workbench.view_count(a), 2);
            assert_eq!(workbench.view_count(b), 1);
        });
    }

    #[gpui::test]
    fn the_marker_shows_exactly_when_closing_the_tab_would_ask(cx: &mut TestAppContext) {
        let (window, native_window, a, b) = window_with_composite(cx);
        assert!(!marker(&window, cx));

        // `a` is dirty but its own tab is another view; `b` has only the
        // composite.
        dirty(a, cx);
        assert!(!marker(&window, cx));
        dirty(b, cx);
        assert!(marker(&window, cx));

        // Closing the document's tab leaves the composite its last view.
        focus_document_tab(&window, cx);
        let execution = dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx);
        cx.run_until_parked();
        assert!(!cx.has_pending_prompt());
        drop(execution);
        assert!(marker(&window, cx));
    }

    #[gpui::test]
    async fn closing_a_composite_asks_about_each_dirty_document_it_shows_last(
        cx: &mut TestAppContext,
    ) {
        let (window, native_window, a, b) = window_with_composite(cx);
        dirty(a, cx);
        dirty(b, cx);
        // The document's tab closes without asking: the composite shows it.
        focus_document_tab(&window, cx);
        let execution = dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx);
        assert_eq!(
            execution.completion.await.unwrap(),
            CommandOutcome::Completed
        );
        assert!(!cx.has_pending_prompt());

        // Closing the composite asks once per document, not per editor.
        let execution = dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx);
        cx.run_until_parked();
        assert!(cx.has_pending_prompt());
        cx.simulate_prompt_answer("Cancel");
        assert_eq!(
            execution.completion.await.unwrap(),
            CommandOutcome::Cancelled
        );
        assert_eq!(composite_tabs(&window, cx), 1);

        let execution = dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx);
        cx.run_until_parked();
        cx.simulate_prompt_answer("Don't Save");
        cx.run_until_parked();
        cx.simulate_prompt_answer("Don't Save");
        assert_eq!(
            execution.completion.await.unwrap(),
            CommandOutcome::Completed
        );
        assert!(!cx.has_pending_prompt());
        assert_eq!(composite_tabs(&window, cx), 0);
        cx.read(|cx| {
            let documents = KnotWindow::documents(cx).read(cx);
            assert!(documents.get(a).is_none() && documents.get(b).is_none());
        });
    }

    #[gpui::test]
    async fn closing_a_composite_keeps_a_document_another_tab_shows(cx: &mut TestAppContext) {
        let (window, native_window, a, b) = window_with_composite(cx);
        dirty(a, cx);
        let execution = dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx);
        cx.run_until_parked();
        // `b` is clean, and `a` still has its tab: nothing asks.
        assert!(!cx.has_pending_prompt());
        assert_eq!(
            execution.completion.await.unwrap(),
            CommandOutcome::Completed
        );
        cx.read(|cx| {
            let documents = KnotWindow::documents(cx).read(cx);
            assert!(documents.get(a).is_some());
            assert!(documents.get(b).is_none());
        });
    }

    /// `update_composite` with a layout of editors on `documents`, which
    /// need not be the composite's.
    fn relayout(
        window: &Entity<KnotWindow>,
        documents: &[DocumentId],
        cx: &mut TestAppContext,
    ) -> Result<(), crate::app::extensions::host::handlers::OpenError> {
        let target = window.update(cx, |window, cx| {
            let (pane, tab) = window
                .workbench
                .read(cx)
                .tabs()
                .find_map(|tab| {
                    matches!(tab.surface_id(), TabSurfaceId::Composite(_)).then(|| {
                        (
                            window.workbench.read(cx).focused_pane_id().unwrap(),
                            tab.id(),
                        )
                    })
                })
                .unwrap();
            crate::app::product::OpenTarget::at(cx.weak_entity(), &window.workbench, pane, tab, cx)
                .public_target()
        });
        let layout = GroupSpec {
            arrangement: Arrangement::Row,
            members: documents
                .iter()
                .map(|&document| {
                    let buffer = cx.read(|cx| {
                        KnotWindow::documents(cx)
                            .read(cx)
                            .get(document)
                            .unwrap()
                            .model()
                            .clone()
                    });
                    MemberSpec {
                        key: None,
                        size: LayoutSize::default(),
                        focused: false,
                        title: None,
                        content: ContentSpec::Editor {
                            buffer,
                            document: Some(document),
                        },
                    }
                })
                .collect(),
        };
        let result = cx.update(|cx| {
            crate::app::product::operations::update_composite(
                &target,
                CompositeTab::default(),
                Some(layout),
                cx,
            )
        });
        cx.run_until_parked();
        result.map(|_| ())
    }

    #[gpui::test]
    fn an_update_that_drops_the_final_view_of_a_document_with_changes_is_refused(
        cx: &mut TestAppContext,
    ) {
        use crate::app::extensions::host::handlers::OpenError;
        let (window, _, a, b) = window_with_composite(cx);
        dirty(b, cx);
        assert!(matches!(
            relayout(&window, &[a], cx),
            Err(OpenError::DocumentUnsaved)
        ));
        // Nothing changed: the composite still shows `b`.
        cx.read(|cx| {
            let workbench = window.read(cx).workbench.read(cx);
            assert_eq!(workbench.view_count(b), 1);
            assert!(KnotWindow::documents(cx).read(cx).get(b).is_some());
        });
    }

    #[gpui::test]
    fn an_update_closes_a_clean_document_it_drops_the_final_view_of_and_keeps_shared_ones(
        cx: &mut TestAppContext,
    ) {
        let (window, _, a, b) = window_with_composite(cx);
        // `a` also has its own tab, so dropping it from the layout keeps it.
        relayout(&window, &[b], cx).unwrap();
        cx.read(|cx| {
            let workbench = window.read(cx).workbench.read(cx);
            assert_eq!(workbench.view_count(a), 1);
            assert_eq!(workbench.view_count(b), 1);
        });
        relayout(&window, &[a], cx).unwrap();
        cx.read(|cx| {
            let documents = KnotWindow::documents(cx).read(cx);
            assert!(documents.get(a).is_some());
            assert!(documents.get(b).is_none());
            assert_eq!(window.read(cx).workbench.read(cx).view_count(a), 2);
        });
    }
}

/// The linked editors' scroll lines.
fn linked_scrolls(window: &Entity<KnotWindow>, cx: &TestAppContext) -> [f32; 2] {
    [0, 1].map(|index| {
        let crate::app::composite_view::Leaf::Editor(view) = leaf_of(window, index, cx) else {
            panic!("an editor");
        };
        cx.read(|cx| view.read(cx).editor().read(cx).scroll_line())
    })
}

fn scroll_linked(window: &Entity<KnotWindow>, index: usize, line: f32, cx: &mut TestAppContext) {
    let crate::app::composite_view::Leaf::Editor(view) = leaf_of(window, index, cx) else {
        panic!("an editor");
    };
    let editor = cx.read(|cx| view.read(cx).editor().clone());
    editor.update(cx, |editor, cx| editor.scroll_to_line(line, cx));
    cx.run_until_parked();
}

#[gpui::test]
fn linked_editors_scroll_together_through_the_package_map(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-linked", cx);
    // Ten lines are inserted at line 10 of the second editor, at any depth.
    scroll_linked(&window, 0, 50., cx);
    assert_eq!(linked_scrolls(&window, cx), [50., 60.]);
    scroll_linked(&window, 1, 30., cx);
    assert_eq!(linked_scrolls(&window, cx), [20., 30.]);
    scroll_linked(&window, 1, 4., cx);
    assert_eq!(linked_scrolls(&window, cx), [4., 4.]);
    scroll_linked(&window, 1, 15., cx);
    assert_eq!(linked_scrolls(&window, cx), [10., 15.]);
}

#[gpui::test]
fn a_new_map_brings_the_second_editor_in_line_with_the_first(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-linked", cx);
    scroll_linked(&window, 0, 50., cx);
    run(&window, native_window, "composite.link-map", cx);
    assert_eq!(linked_scrolls(&window, cx), [50., 50.]);
    // Other updates keep the link.
    run(
        &window,
        native_window,
        "composite.link-kept-by-other-updates",
        cx,
    );
    scroll_linked(&window, 1, 30., cx);
    assert_eq!(linked_scrolls(&window, cx), [30., 30.]);
}

#[gpui::test]
fn removing_the_link_lets_the_editors_scroll_apart(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-linked", cx);
    run(&window, native_window, "composite.unlink", cx);
    scroll_linked(&window, 0, 50., cx);
    assert_eq!(linked_scrolls(&window, cx), [50., 0.]);
}

#[gpui::test]
fn a_link_that_does_not_join_two_editors_is_refused_and_changes_nothing(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-linked", cx);
    run(&window, native_window, "composite.invalid-links", cx);
    scroll_linked(&window, 0, 50., cx);
    assert_eq!(linked_scrolls(&window, cx), [50., 60.]);
}

/// The editors each connector of the shown composite joins, by entity.
fn connected_editors(
    window: &Entity<KnotWindow>,
    cx: &TestAppContext,
) -> Vec<Option<[gpui::EntityId; 2]>> {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let tab = workbench.focused_pane().unwrap().active_tab();
        workbench
            .composite_view(tab)
            .unwrap()
            .read(cx)
            .connected_editors(cx)
    })
}

#[gpui::test]
fn a_connector_joins_two_editors_and_is_not_a_view(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-connected", cx);
    let editors = [0, 1].map(|index| leaf_of(&window, index, cx).entity_id());
    assert_eq!(connected_editors(&window, cx), vec![Some(editors)]);

    // The tree holds the two editors and no node for the strip.
    let info = info(&window, cx);
    let tab = info
        .panes
        .iter()
        .flat_map(|pane| &pane.tabs)
        .find(|tab| tab.kind == SurfaceKind::Composite)
        .unwrap();
    assert_eq!(tab.root.children.len(), 2);

    // A new layout joins the same editors again with the new links.
    run(&window, native_window, "composite.reconnect", cx);
    assert_eq!(connected_editors(&window, cx), vec![Some(editors)]);
    assert_eq!(
        [0, 1].map(|index| leaf_of(&window, index, cx).entity_id()),
        editors
    );
}

#[gpui::test]
fn a_connector_that_does_not_join_two_editors_by_key_is_refused(cx: &mut TestAppContext) {
    let (window, native_window, _, _) = window_with_extension(cx);
    run(&window, native_window, "composite.open-connected", cx);
    run(&window, native_window, "composite.invalid-connectors", cx);
    assert_eq!(connected_editors(&window, cx).len(), 1);
}

#[test]
fn a_band_spans_both_editors_and_is_skipped_when_both_ends_are_off_screen() {
    use super::super::super::composite_view::band;
    let (above, below) = ((px(-60.), px(-40.)), (px(500.), px(520.)));
    let (a, b) = ((px(10.), px(30.)), (px(20.), px(80.)));
    let x = (px(100.), px(132.));
    let visible = (px(0.), px(400.));
    let corners = band(a, b, x, visible).unwrap();
    assert_eq!(
        corners,
        [
            gpui::point(px(100.), px(10.)),
            gpui::point(px(132.), px(20.)),
            gpui::point(px(132.), px(80.)),
            gpui::point(px(100.), px(30.)),
        ]
    );
    // One end on screen is enough; both ends on one side is not.
    assert!(band(a, above, x, visible).is_some());
    assert!(band(above, above, x, visible).is_none());
    assert!(band(below, below, x, visible).is_none());
    assert!(band(above, below, x, visible).is_some());
}
