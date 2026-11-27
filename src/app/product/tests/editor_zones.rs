//! Editor zones through registered extension commands: lines, update in
//! place, the ways a zone closes, and its place in the view tree.
use super::*;
use crate::app::editor::Editor;
use knot_protocol::ExtensionKey;

/// A package whose commands, named after `LABEL`, open and close zones named
/// "hunk" and "end" through a long-lived editor handle.
const SOURCE: &str = r#"
import { buffers, commands, workbench } from "knot";
const check = (condition, message) => { if (!condition) throw new Error(message); };
const expectError = async (operation, test, message) => {
  try { await operation(); } catch (error) { if (test(error)) return; throw error; }
  throw new Error(message);
};
let handle = null;
const editor = async () => {
  if (!handle) {
    const { snapshot } = await workbench.observe(() => {});
    handle = await workbench.editor(snapshot.windows[0].panes[0].tabs.find((tab) => tab.target === snapshot.windows[0].panes[0].selected).root.target);
  }
  return handle;
};
let closes = 0;
await commands.register("LABEL.watch", async () => {
  await (await editor()).onDidCloseZone(async ({ zone }) => {
    closes += 1;
    await workbench.writeClipboardText(`LABEL:${zone}:${closes}`);
  });
});
let made = null;
const layout = async (second) => {
  made ??= await Promise.all(["a\n", "b\n", "c\n"].map((text) => buffers.create({ text })));
  return {
    toolbar: [{ label: "Tool", icon: "file", command: "LABEL.tool" }],
    layout: { type: "column", children: [
      { type: "editor", key: "a", buffer: made[0] },
      second ? { type: "editor", buffer: made[2] } : { type: "editor", key: "b", buffer: made[1] },
    ] },
  };
};
await commands.register("LABEL.open", async () => {
  await (await editor()).openZone("hunk", { line: 1, ...await layout(false) });
});
await commands.register("LABEL.open-focus", async () => {
  await (await editor()).openZone("hunk", { line: 1, focus: true, ...await layout(false) });
});
await commands.register("LABEL.open-end", async () => {
  await (await editor()).openZone("end", { line: 1000, ...await layout(false) });
});
await commands.register("LABEL.update", async () => {
  await (await editor()).openZone("hunk", { line: 2, ...await layout(true) });
});
await commands.register("LABEL.close", async () => { await (await editor()).closeZone("hunk"); });
await commands.register("LABEL.tool", async ({ editor: captured }) => { await captured.closeZone("hunk"); });
await commands.register("LABEL.tree", async () => {
  const { snapshot } = await workbench.observe(() => {});
  const pane = snapshot.windows[0].panes[0];
  const root = pane.tabs.find((tab) => tab.target === pane.selected).root;
  check(root.kind === "editor" && root.children.length === 1, "the editor has one child");
  const [zone] = root.children;
  check(zone.kind === "zone" && zone.children.length === 2, "the zone has its layout's members");
  check(zone.children.every((member) => member.kind === "editor"), "the members are editors");
});
await commands.register("LABEL.validate", async ({ editor: captured }) => {
  const given = await layout(false);
  await expectError(() => captured.openZone("", { line: 0, ...given }), (e) => e instanceof TypeError, "empty id");
  await expectError(() => captured.openZone("x", { line: -1, ...given }), (e) => e instanceof TypeError, "negative line");
  await expectError(() => captured.openZone("x", { line: 1.5, ...given }), (e) => e instanceof TypeError, "fractional line");
  await expectError(() => captured.openZone("x", { line: 0, ...given, focus: 1 }), (e) => e instanceof TypeError, "focus is a boolean");
  await expectError(() => captured.openZone("x", { line: 0, layout: { type: "tabs", children: [] } }),
    (e) => e instanceof TypeError, "layout must be a row or column");
  await captured.closeZone("missing");
});
"#;

fn clipboard_text(cx: &TestAppContext) -> Option<String> {
    cx.read(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

fn load(label: &str, id: u64, cx: &mut TestAppContext) -> ExtensionKey {
    let host = cx.read(|cx| crate::app::extensions::host::entity(cx).unwrap());
    let key = ExtensionKey::new(ExtensionId::new(id), ExtensionLifecycleId::new(1));
    let source = SOURCE.replace("LABEL", label);
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, &source).unwrap();
        cx.spawn(async move |_, _| execution.await.unwrap())
            .detach();
    });
    let command = format!("{label}.validate");
    wait_for_api_condition(cx, |cx| {
        cx.global::<ApplicationProductCommands>()
            .0
            .read(cx)
            .catalog()
            .borrow()
            .resolve(&command)
            .is_ok()
    });
    key
}

struct Fixture {
    window: Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    editor: Entity<Editor>,
}

impl Fixture {
    fn new(cx: &mut TestAppContext) -> Self {
        let documents = install_globals(cx);
        cx.update(super::super::bind_product_keys);
        let document = cx.update(|cx| create_untitled_document(&documents, cx));
        let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
        model.update(cx, |model, _| {
            model.replace(0..0, "one\ntwo\nthree\nfour\n").unwrap()
        });
        cx.update(super::super::super::extensions::host::install);
        let (window, native_window) = product_window(document, model, cx);
        cx.run_until_parked();
        let editor = super::editor_api::active_editor(&window, cx);
        Self {
            native_window,
            window,
            editor,
        }
    }

    fn run(&self, name: &'static str, cx: &mut TestAppContext) {
        let execution = dispatch_product_command(&self.window, self.native_window, name, cx);
        assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
        cx.run_until_parked();
    }

    fn refresh(&self, cx: &mut TestAppContext) {
        cx.update_window(self.native_window, |_, window, _| window.refresh())
            .unwrap();
        cx.run_until_parked();
    }

    /// Each zone's owner, ID, and line, in opening order.
    fn zones(&self, cx: &TestAppContext) -> Vec<(ExtensionKey, String, usize)> {
        cx.read(|cx| {
            self.editor
                .read(cx)
                .zones()
                .zones()
                .iter()
                .map(|zone| (zone.owner, zone.id.clone(), zone.spec.line))
                .collect()
        })
    }

    fn leaves(&self, index: usize, cx: &TestAppContext) -> Vec<crate::app::view_layout::Leaf> {
        cx.read(|cx| {
            self.editor.read(cx).zone_contents()[index]
                .read(cx)
                .leaves()
        })
    }
}

#[gpui::test]
fn zones_from_two_lifecycles_update_in_place_and_end_with_their_lifecycle(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    let first = load("first", 921, cx);
    let second = load("second", 922, cx);
    fixture.run("first.open", cx);
    fixture.run("second.open", cx);
    fixture.run("first.open-end", cx);
    let end = fixture.zones(cx)[2].2;
    assert_eq!(
        fixture.zones(cx)[..2],
        [
            (first, "hunk".to_owned(), 1),
            (second, "hunk".to_owned(), 1)
        ]
    );
    // A line past the last clamps to the end: below the last line.
    assert!(end >= 4);

    // An update keeps the zone's place and the views of its keyed members.
    let before = fixture.leaves(0, cx);
    fixture.run("first.update", cx);
    let after = fixture.leaves(0, cx);
    assert_eq!(fixture.zones(cx)[0], (first, "hunk".to_owned(), 2));
    assert_eq!(after[0].entity_id(), before[0].entity_id());
    assert_ne!(after[1].entity_id(), before[1].entity_id());

    fixture.run("first.validate", cx);
    assert_eq!(fixture.zones(cx).len(), 3);
    fixture.run("first.close", cx);
    assert_eq!(
        fixture
            .zones(cx)
            .iter()
            .map(|zone| (zone.0, zone.1.as_str()))
            .collect::<Vec<_>>(),
        [(second, "hunk"), (first, "end")]
    );

    let host = cx.read(|cx| crate::app::extensions::host::entity(cx).unwrap());
    host.update(cx, |host, cx| host.unload(first, cx).unwrap());
    assert_eq!(fixture.zones(cx), [(second, "hunk".to_owned(), 1)]);
}

#[gpui::test]
fn escape_in_a_focused_zone_closes_it_and_tells_its_owner(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    load("esc", 923, cx);
    fixture.run("esc.watch", cx);
    fixture.run("esc.open-focus", cx);
    fixture.run("esc.open-end", cx);
    fixture.refresh(cx);

    let content = cx.read(|cx| fixture.editor.read(cx).zone_contents()[0].clone());
    content.update(cx, |content, cx| content.focus_leaf(0, cx));
    cx.simulate_keystrokes(fixture.native_window, "escape");
    cx.run_until_parked();
    assert_eq!(
        fixture
            .zones(cx)
            .iter()
            .map(|zone| zone.1.as_str())
            .collect::<Vec<_>>(),
        ["end"]
    );
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard().and_then(|item| item.text()) == Some("esc:hunk:1".to_owned())
    });

    // Closing through the API and from a toolbar button's command report
    // nothing.
    fixture.run("esc.open", cx);
    fixture.run("esc.close", cx);
    fixture.run("esc.open", cx);
    fixture.refresh(cx);
    super::composite::click_toolbar_button(fixture.native_window, "popover-toolbar-0", cx);
    cx.run_until_parked();
    assert_eq!(fixture.zones(cx).len(), 1);
    assert_eq!(clipboard_text(cx).as_deref(), Some("esc:hunk:1"));
}

#[gpui::test]
fn a_zone_is_a_child_of_its_editor_in_the_view_tree(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    load("tree", 924, cx);
    fixture.run("tree.open", cx);
    fixture.run("tree.tree", cx);
}
