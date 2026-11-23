//! Resource decorations pushed through the public extension API.
use super::*;
use crate::app::resource::ResourceUri;
use crate::app::resource_decorations::{ApplicationResourceDecorations, Badge};
use crate::test_support::{TestWindow, wait_until};
use knot_protocol::ExtensionKey;

const SOURCE: &str = r##"
import { commands, decorations } from "knot";
async function failure(operation) {
  try { await operation(); } catch (error) { return error; }
  throw new Error("expected a failure");
}
const source = await decorations.createSource();
for (const invalid of [
  () => source.set(null),
  () => source.set([]),
  () => source.set({ "file:///repo/a": "red" }),
  () => source.set({ "file:///repo/a": { colour: "error" } }),
  () => source.set({ "file:///repo/a": { color: 3 } }),
  () => source.set({ "file:///repo/a": { tooltip: 3 } }),
  () => source.set({ "file:///repo/a": { propagate: "yes" } }),
  () => source.set({ "file:///repo/a": { badge: "M" } }),
  () => source.set({ "file:///repo/a": { badge: {} } }),
  () => source.set({ "file:///repo/a": { badge: { text: "M", icon: "x" } } }),
  () => source.set({ "file:///repo/a": { badge: { text: "" } } }),
  () => source.set({ "file:///repo/a": { badge: { text: "MMM" } } }),
  () => source.set({ "file:///repo/a": { badge: { text: 1 } } }),
  () => source.set({ "file:///repo/a": { badge: { icon: "" } } }),
]) {
  const error = await failure(invalid);
  if (!(error instanceof TypeError)) throw error;
}
const malformed = await failure(() => source.set({ "not a uri": { color: "error" } }));
if (malformed.code !== "InvalidUri") throw malformed;
await commands.register("decorate", async () => {
  await source.set({
    "file:///repo/src/main.rs": {
      color: "warning", badge: { text: "M" }, tooltip: "Modified", propagate: true,
    },
    "file:///repo/a.rs": { badge: { icon: "star" } },
    "file:///repo/src/../b.rs": { color: "error" },
  });
});
await commands.register("partial", async () => {
  await source.set({ "file:///repo/a.rs": null, "file:///repo/c.rs": { color: "info" } });
});
await commands.register("clear", async () => { await source.clear(); });
await commands.register("dispose", async () => {
  await source[Symbol.asyncDispose]();
  const error = await failure(() => source.set({}));
  if (error.code !== "OwnershipDisposed") throw error;
});
"##;

fn lookup(
    text: &str,
    cx: &mut TestAppContext,
) -> Option<crate::app::resource_decorations::ResolvedDecoration> {
    let uri = ResourceUri::parse(text).unwrap();
    cx.read(|cx| {
        cx.global::<ApplicationResourceDecorations>()
            .0
            .lookup(&uri)
            .cloned()
    })
}

fn run(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    command: &'static str,
    cx: &mut TestAppContext,
) {
    let execution = dispatch_product_command(window, native_window, command, cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    cx.run_until_parked();
}

#[gpui::test]
fn sources_validate_normalize_change_partially_and_end_with_their_lifecycle(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let key = ExtensionKey::new(ExtensionId::new(980), ExtensionLifecycleId::new(1));
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
    // Rejected calls changed nothing.
    assert_eq!(lookup("file:///repo/a", cx), None);

    run(&window, native_window, "decorate", cx);
    let main = lookup("file:///repo/src/main.rs", cx).unwrap();
    assert_eq!(main.color.as_deref(), Some("warning"));
    assert_eq!(main.badge, Some(Badge::Text("M".into())));
    assert_eq!(main.tooltip.as_deref(), Some("Modified"));
    let folder = lookup("file:///repo/src", cx).unwrap();
    assert_eq!(folder.color.as_deref(), Some("warning"));
    assert_eq!(folder.badge, None);
    assert_eq!(
        lookup("file:///repo/a.rs", cx).unwrap().badge,
        Some(Badge::Icon("star".into()))
    );
    // The URI is normalized, so the dot segment resolves.
    assert_eq!(
        lookup("file:///repo/b.rs", cx).unwrap().color.as_deref(),
        Some("error")
    );

    // A partial set changes only the URIs it names.
    run(&window, native_window, "partial", cx);
    assert_eq!(lookup("file:///repo/a.rs", cx), None);
    assert!(lookup("file:///repo/c.rs", cx).is_some());
    assert!(lookup("file:///repo/src/main.rs", cx).is_some());

    run(&window, native_window, "clear", cx);
    assert_eq!(lookup("file:///repo/src/main.rs", cx), None);
    assert_eq!(lookup("file:///repo", cx), None);

    run(&window, native_window, "decorate", cx);
    run(&window, native_window, "dispose", cx);
    assert_eq!(lookup("file:///repo/src/main.rs", cx), None);

    // Tearing down the lifecycle removes what its sources still hold.
    let second = ExtensionKey::new(ExtensionId::new(981), ExtensionLifecycleId::new(1));
    let loaded = Arc::new(Mutex::new(None));
    let result = loaded.clone();
    host.update(cx, |host, cx| {
        let execution = host
            .load_test_source(
                second,
                r#"
import { decorations } from "knot";
const source = await decorations.createSource();
await source.set({ "file:///repo/z.rs": { color: "error" } });
"#,
            )
            .unwrap();
        cx.spawn(async move |_, _| {
            *result.lock().unwrap() = Some(execution.await.map(|_| ()));
        })
        .detach();
    });
    wait_for_api_condition(cx, |_| loaded.lock().unwrap().is_some());
    loaded.lock().unwrap().take().unwrap().unwrap();
    assert!(lookup("file:///repo/z.rs", cx).is_some());
    host.update(cx, |host, cx| host.unload(second, cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(lookup("file:///repo/z.rs", cx), None);
}

const TREE: &str = r##"
import { commands, decorations, workbench } from "knot";
const source = await decorations.createSource();
await workbench.registerView({
  id: "example.decorated",
  title: "Decorated",
  content: { kind: "tree", provider: {
    getChildren(parentId) {
      return parentId === null ? [
        { id: "src", label: "src", collapsibleState: "expanded", resource: "file:///repo/src" },
        { id: "plain", label: "plain.rs", collapsibleState: "none",
          resource: "file:///repo/plain.rs", statusText: "own", badge: "info" },
      ] : parentId === "src" ? [
        { id: "main", label: "main.rs", collapsibleState: "none",
          resource: "file:///repo/src/main.rs" },
      ] : [];
    },
  } },
});
await commands.register("decorate", async () => {
  await source.set({
    "file:///repo/src/main.rs": {
      color: "warning", badge: { text: "M" }, tooltip: "Modified", propagate: true,
    },
    "file:///repo/plain.rs": { badge: { icon: "star" } },
  });
});
await commands.register("clear", async () => { await source.clear(); });
"##;

#[gpui::test]
fn tree_rows_with_a_resource_show_decorations_and_repaint_on_change(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let key = ExtensionKey::new(ExtensionId::new(982), ExtensionLifecycleId::new(1));
    let loaded = Arc::new(Mutex::new(None));
    let result = loaded.clone();
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, TREE).unwrap();
        cx.spawn(async move |_, _| {
            *result.lock().unwrap() = Some(execution.await.map(|_| ()));
        })
        .detach();
    });
    wait_for_api_condition(cx, |_| loaded.lock().unwrap().is_some());
    loaded.lock().unwrap().take().unwrap().unwrap();
    let tree = TestWindow::of(window.clone(), cx).tree("example.decorated", cx);
    wait_until(cx, |cx| tree.labels(cx).len() == 3);
    assert_eq!(tree.decoration("main", cx), None);
    assert!(!tree.paints_decoration_badge(1, cx));

    run(&window, native_window, "decorate", cx);
    assert_eq!(
        tree.decoration("main", cx),
        Some((
            Some("warning".into()),
            Some("text:M".into()),
            Some("Modified".into())
        ))
    );
    // The folder takes the propagated color alone.
    assert_eq!(
        tree.decoration("src", cx),
        Some((Some("warning".into()), None, None))
    );
    // A row's own status and badge stay beside the decoration.
    assert_eq!(
        tree.decoration("plain", cx),
        Some((None, Some("icon:star".into()), None))
    );
    assert_eq!(tree.status("plain", cx).as_deref(), Some("own"));
    assert!(tree.paints_decoration_badge(1, cx));
    assert!(!tree.paints_decoration_badge(0, cx));

    run(&window, native_window, "clear", cx);
    assert_eq!(tree.decoration("main", cx), None);
    assert_eq!(tree.decoration("src", cx), None);
    assert!(!tree.paints_decoration_badge(1, cx));
}
