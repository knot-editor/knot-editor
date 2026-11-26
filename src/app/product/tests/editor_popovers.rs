//! Editor popovers through registered extension commands: stacking and
//! updates across lifecycles, anchors following edits, and the ways a popover
//! closes.
use super::*;
use crate::app::editor::Editor;
use knot_protocol::ExtensionKey;
use std::ops::Range;

/// A package whose commands, named after `LABEL`, open and close popovers
/// named "hunk" and "hover" through a long-lived editor handle.
const SOURCE: &str = r#"
import { buffers, commands, workbench } from "knot";
const expectError = async (operation, test, message) => {
  try { await operation(); } catch (error) { if (test(error)) return; throw error; }
  throw new Error(message);
};
let handle = null;
let buffer = null;
const editor = async () => {
  if (!handle) {
    const { snapshot } = await workbench.observe(() => {});
    handle = await workbench.editor(snapshot.windows[0].panes[0].tabs.find((tab) => tab.target === snapshot.windows[0].panes[0].selected).root.target);
  }
  return handle;
};
const content = async (height) => {
  buffer ??= await buffers.create({ text: "changed\n" });
  return { type: "column", children: [{ type: "editor", buffer, size: { pixels: height } }] };
};
await commands.register("LABEL.open", async () => {
  await (await editor()).openPopover("hunk", { anchor: { start: 1, end: 2 }, layout: await content(60) });
});
await commands.register("LABEL.update", async () => {
  await (await editor()).openPopover("hunk", { anchor: { start: 0, end: 3 }, layout: await content(90) });
});
await commands.register("LABEL.hover", async () => {
  await (await editor()).openPopover("hover", { anchor: { start: 2, end: 3 }, layout: await content(40) });
});
await commands.register("LABEL.close", async () => { await (await editor()).closePopover("hunk"); });
await commands.register("LABEL.validate", async ({ editor: captured }) => {
  const layout = await content(50);
  const anchor = { start: 0, end: 1 };
  await expectError(() => captured.openPopover("", { anchor, layout }), (e) => e instanceof TypeError, "empty id");
  await expectError(() => captured.openPopover("x", { anchor: { start: 1, end: 1 }, layout }),
    (e) => e instanceof TypeError, "empty anchor");
  await expectError(() => captured.openPopover("x", { anchor: { start: -1, end: 1 }, layout }),
    (e) => e instanceof TypeError, "negative anchor");
  await expectError(() => captured.openPopover("x", { anchor, layout: { type: "tabs", children: [] } }),
    (e) => e instanceof TypeError, "layout must be a row or column");
  await expectError(() => captured.openPopover("x", { anchor, layout: { type: "column", children: [{ type: "editor", buffer, size: { pixels: -1 } }] } }),
    (e) => e instanceof TypeError, "size must be positive");
  await captured.closePopover("missing");
});
"#;

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
    model: Entity<Buffer>,
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
        let (window, native_window) = product_window(document, model.clone(), cx);
        cx.run_until_parked();
        let editor = super::editor_api::active_editor(&window, cx);
        Self {
            native_window,
            window,
            editor,
            model,
        }
    }

    fn run(&self, name: &'static str, cx: &mut TestAppContext) {
        let execution = dispatch_product_command(&self.window, self.native_window, name, cx);
        assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
        cx.run_until_parked();
    }

    /// Each popover's owner, ID, anchored lines, and height, bottom to top.
    fn popovers(&self, cx: &TestAppContext) -> Vec<(ExtensionKey, String, Range<u32>, f32)> {
        cx.read(|cx| {
            let editor = self.editor.read(cx);
            let model = self.model.read(cx);
            editor
                .popovers()
                .popovers()
                .iter()
                .map(|popover| {
                    (
                        popover.owner,
                        popover.id.clone(),
                        editor.popovers().anchor_lines(popover, model).unwrap(),
                        popover.spec.height,
                    )
                })
                .collect()
        })
    }

    fn edit(&self, range: Range<usize>, text: &str, cx: &mut TestAppContext) {
        self.model.update(cx, |model, cx| {
            model.replace(range, text).unwrap();
            cx.notify();
        });
        cx.run_until_parked();
    }
}

#[gpui::test]
fn popovers_from_two_lifecycles_stack_update_and_end_with_their_lifecycle(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    let first = load("first", 901, cx);
    let second = load("second", 902, cx);
    fixture.run("first.open", cx);
    fixture.run("second.open", cx);
    fixture.run("first.hover", cx);
    fixture.run("first.update", cx);
    assert_eq!(
        fixture.popovers(cx),
        [
            (first, "hunk".to_owned(), 0..3, 90.),
            (second, "hunk".to_owned(), 1..2, 60.),
            (first, "hover".to_owned(), 2..3, 40.),
        ]
    );

    fixture.run("first.validate", cx);
    assert_eq!(fixture.popovers(cx).len(), 3);
    fixture.run("first.close", cx);
    assert_eq!(
        fixture
            .popovers(cx)
            .iter()
            .map(|popover| (popover.0, popover.1.as_str()))
            .collect::<Vec<_>>(),
        [(second, "hunk"), (first, "hover")]
    );

    let host = cx.read(|cx| crate::app::extensions::host::entity(cx).unwrap());
    host.update(cx, |host, cx| host.unload(first, cx).unwrap());
    assert_eq!(
        fixture.popovers(cx),
        [(second, "hunk".to_owned(), 1..2, 60.)]
    );
}

#[gpui::test]
fn the_anchor_follows_edits_and_deleting_it_closes_the_popover(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    let owner = load("anchor", 903, cx);
    fixture.run("anchor.open", cx);
    fixture.edit(0..0, "zero\n", cx);
    assert_eq!(
        fixture.popovers(cx),
        [(owner, "hunk".to_owned(), 2..3, 60.)]
    );

    // Deleting the anchored line, "two", closes the popover.
    fixture.edit(9..13, "", cx);
    assert!(fixture.popovers(cx).is_empty());
}

#[gpui::test]
fn the_popover_paints_over_the_text_below_its_anchor(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    load("paint", 904, cx);
    fixture.run("paint.open", cx);
    cx.update_window(fixture.native_window, |_, window, _| window.refresh())
        .unwrap();
    cx.run_until_parked();
    let view = cx.read(|cx| {
        fixture
            .window
            .read(cx)
            .workbench
            .read(cx)
            .focused_editor()
            .unwrap()
            .clone()
    });
    let popovers = cx.read(|cx| fixture.editor.read(cx).popover_view_entities());
    assert_eq!(popovers.len(), 1);
    let (below_anchor, popover, text) = cx.read(|cx| {
        (
            view.read(cx).line_top(2, cx),
            popovers[0].read(cx).interaction_bounds(),
            view.read(cx).text_bounds(),
        )
    });
    // The popover starts under the anchor's line and floats over the lines
    // below it, within the text area.
    assert!(
        popover.top() >= below_anchor,
        "{popover:?} {below_anchor:?}"
    );
    assert!(popover.size.height > px(0.));
    assert!(popover.right() <= text.right() && popover.left() >= text.left());
}
