//! Icon definitions, packs, and overrides through the public extension API.
use super::*;
use crate::app::icons::{apply_at_startup, icon_definition};
use knot_model::icons::{IconColor, IconDefinition, IconSource};
use knot_protocol::ExtensionKey;

const SOURCE: &str = r##"
import { commands, icons, settings } from "knot";
async function failure(operation) {
  try { await operation(); } catch (error) { return error; }
  throw new Error("expected a failure");
}
const svg = (fill) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" fill="${fill}"/></svg>`;
for (const invalid of [
  () => icons.define("x.bad", {}),
  () => icons.define("x.bad", { svg: "not an svg" }),
  () => icons.define("x.bad", { svg: svg("red"), lucide: "file" }),
  () => icons.define("x.bad", { lucide: "no-such-glyph" }),
  () => icons.define("x.bad", { lucide: "file", color: "original" }),
  () => icons.define("x.bad", { lucide: "file", size: 2 }),
  () => icons.define("not a name", { lucide: "file" }),
  () => icons.override("x.bad", { lucide: "no-such-glyph" }),
  () => icons.registerPack({ name: "Bad", icons: { "x.bad": { svg: "<svg" } } }),
  () => icons.registerPack({ name: "", icons: {} }),
]) {
  const error = await failure(invalid);
  if (!(error instanceof TypeError)) throw error;
}
await icons.define("x.icon", { lucide: "star" });
for (const name of ["file", "x.icon"]) {
  const duplicate = await failure(() => icons.define(name, { lucide: "box" }));
  if (duplicate.code !== "DuplicateIcon" || duplicate.details.name !== name) throw duplicate;
}
let pack = null;
let override = null;
await commands.register("pack.add", async () => {
  pack = await icons.registerPack({ name: "Red", icons: { "x.icon": { svg: svg("#f00"), color: "original" } } });
  await icons.registerPack({ name: "Blue", icons: {} });
  if ((await icons.listPacks()).join() !== "Blue,Red") throw new Error("packs");
});
await commands.register("pack.select", async ({ arguments: [name] }) => {
  await settings.set("icons.pack", name);
});
await commands.register("pack.drop", async () => { await pack.dispose(); });
await commands.register("override.add", async () => {
  override = await icons.override("x.icon", { svg: svg("#00f"), color: "error" });
});
await commands.register("override.drop", async () => { await override.dispose(); });
"##;

fn star() -> IconDefinition {
    IconDefinition::lucide("star")
}

fn current(cx: &mut TestAppContext) -> IconDefinition {
    cx.read(|cx| icon_definition("x.icon", cx))
}

#[gpui::test]
fn definitions_packs_and_overrides_layer_and_teardown_reveals_the_previous_level(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (shell, window) = product_window(document, model, cx);
    let key = ExtensionKey::new(ExtensionId::new(962), ExtensionLifecycleId::new(1));
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
    cx.update(apply_at_startup);

    let run = |name: &'static str, cx: &mut TestAppContext| {
        let execution = dispatch_product_command(&shell, window, name, cx);
        assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
        cx.run_until_parked();
    };
    let select = |pack: &str, cx: &mut TestAppContext| {
        let execution = dispatch_product_command_with(
            &shell,
            window,
            "pack.select",
            CommandArgumentValue::Array(vec![CommandArgumentValue::String(pack.into())]),
            cx,
        );
        assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
        cx.run_until_parked();
    };

    // A package default is visible at once, and an unselected pack does not apply.
    assert_eq!(current(cx), star());
    run("pack.add", cx);
    assert_eq!(current(cx), star());
    select("Red", cx);
    let IconSource::Svg(red) = current(cx).source else {
        panic!("the pack's svg applies");
    };
    assert!(red.contains("#f00"));
    assert_eq!(current(cx).color, IconColor::Original);

    // An override wins over the pack, and disposing it reveals the pack.
    run("override.add", cx);
    let overridden = current(cx);
    assert!(matches!(&overridden.source, IconSource::Svg(svg) if svg.contains("#00f")));
    assert_eq!(overridden.color, IconColor::Key("error".into()));
    run("override.drop", cx);
    assert_eq!(current(cx).color, IconColor::Original);

    // Disposing the pack reveals the default; teardown drops everything.
    run("pack.drop", cx);
    assert_eq!(current(cx), star());
    run("pack.add", cx);
    run("override.add", cx);
    host.update(cx, |host, cx| host.unload(key, cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(
        current(cx),
        cx.read(|cx| icon_definition("fallback", cx)),
        "the defined name goes with its lifecycle"
    );
    cx.read(|cx| {
        assert!(
            cx.global::<crate::app::icons::ApplicationIcons>()
                .0
                .pack_names()
                .is_empty()
        );
    });
}
