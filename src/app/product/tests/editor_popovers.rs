//! Editor popovers through registered extension commands: stacking and
//! updates across lifecycles, anchors following edits, and the ways a popover
//! closes.
use super::*;
use crate::app::editor::Editor;
use knot_model::editor::popovers::PopoverWidth;
use knot_protocol::ExtensionKey;
use std::ops::Range;

/// A package whose commands, named after `LABEL`, open and close popovers
/// named "hunk" and "hover" through a long-lived editor handle.
const SOURCE: &str = r#"
import { buffers, commands, documents, workbench } from "knot";
const check = (condition, message) => { if (!condition) throw new Error(message); };
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
const content = async (width) => {
  buffer ??= await buffers.create({ text: "changed\n" });
  return { width: { pixels: width }, layout: { type: "column", children: [{ type: "editor", buffer }] } };
};
await commands.register("LABEL.open", async () => {
  await (await editor()).openPopover("hunk", { anchor: { start: 1, end: 2 }, ...await content(60) });
});
await commands.register("LABEL.update", async () => {
  await (await editor()).openPopover("hunk", { anchor: { start: 0, end: 3 }, ...await content(90) });
});
await commands.register("LABEL.hover", async () => {
  await (await editor()).openPopover("hover", { anchor: { start: 2, end: 3 }, ...await content(40) });
});
// A popover of every kind of content the test can make: keyed editors on
// buffers and a document, and a toolbar button.
let rich = null;
const richLayout = async (second) => {
  rich ??= await Promise.all(["a\n", "b\n", "c\n"].map((text) => buffers.create({ text })));
  const [doc] = await documents.list();
  return {
    width: "editor",
    toolbar: [{ label: "Tool", icon: "file", command: "LABEL.tool", arguments: { n: 1 } }],
    layout: { type: "column", children: [
      { type: "editor", key: "a", buffer: rich[0] },
      second
        ? { type: "editor", buffer: rich[2] }
        : { type: "row", key: "pair", children: [
            { type: "editor", key: "b", buffer: rich[1] },
            { type: "editor", key: "doc", document: doc.id },
          ] },
    ] },
  };
};
await commands.register("LABEL.open-rich", async () => {
  await (await editor()).openPopover("rich", { anchor: { start: 1, end: 2 }, ...await richLayout(false) });
});
await commands.register("LABEL.update-rich", async () => {
  await (await editor()).openPopover("rich", { anchor: { start: 1, end: 2 }, ...await richLayout(true) });
});
let tool = null;
await commands.register("LABEL.tool", async ({ editor: captured, target, arguments: args }) => {
  tool = { editor: captured !== null, chain: target.viewChain.length, n: args.n };
  // The captured editor is the anchoring one, so this closes the popover.
  await captured.closePopover("rich");
});
await commands.register("LABEL.expect-tool", async () => {
  check(tool !== null && tool.editor && tool.n === 1, `the toolbar command ran: ${JSON.stringify(tool)}`);
});
await commands.register("LABEL.tree", async () => {
  const { snapshot } = await workbench.observe(() => {});
  const pane = snapshot.windows[0].panes[0];
  const root = pane.tabs.find((tab) => tab.target === pane.selected).root;
  check(root.kind === "editor" && root.children.length === 1, "the editor has one child");
  const [popover] = root.children;
  check(popover.kind === "popover" && popover.children.length === 2, "the popover has its layout's members");
  const [first, pair] = popover.children;
  check(first.kind === "editor" && first.document === null, "a buffer editor");
  check(pair.kind === "composite" && pair.children.length === 2, "the row is a group");
  check(pair.children[1].document !== null, "the document editor shows its document");
  const inner = await workbench.editor(first.target);
  check(inner !== null, "workbench.editor reaches the popover's editor");
});
await commands.register("LABEL.close", async () => { await (await editor()).closePopover("hunk"); });
await commands.register("LABEL.validate", async ({ editor: captured }) => {
  const given = await content(50);
  const anchor = { start: 0, end: 1 };
  await expectError(() => captured.openPopover("", { anchor, ...given }), (e) => e instanceof TypeError, "empty id");
  await expectError(() => captured.openPopover("x", { ...given, anchor: { start: 1, end: 1 } }),
    (e) => e instanceof TypeError, "empty anchor");
  await expectError(() => captured.openPopover("x", { ...given, anchor: { start: -1, end: 1 } }),
    (e) => e instanceof TypeError, "negative anchor");
  await expectError(() => captured.openPopover("x", { anchor, ...given, width: { pixels: 0 } }),
    (e) => e instanceof TypeError, "width must be positive");
  await expectError(() => captured.openPopover("x", { anchor, ...given, width: "wide" }),
    (e) => e instanceof TypeError, "width is a number of pixels or the editor");
  await expectError(() => captured.openPopover("x", { anchor, ...given, layout: { type: "column", children: [{ type: "editor", buffer, maxRows: 3 }] } }),
    (e) => e instanceof TypeError, "an editor has maxLines");
  await expectError(() => captured.openPopover("x", { anchor, ...given, layout: { type: "column", children: [{ type: "editor", buffer, maxLines: 0 }] } }),
    (e) => e instanceof TypeError, "maxLines is positive");
  await expectError(() => captured.openPopover("x", { anchor, layout: { type: "tabs", children: [] } }),
    (e) => e instanceof TypeError, "layout must be a row or column");
  await expectError(() => captured.openPopover("x", { anchor, layout: { type: "column", children: [{ type: "editor", buffer, size: { pixels: -1 } }] } }),
    (e) => e instanceof TypeError, "size must be positive");
  await expectError(() => captured.openPopover("x", { anchor, ...given, toolbar: [{ label: "x" }] }),
    (e) => e instanceof TypeError, "toolbar button needs an icon");
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

    /// Each popover's owner, ID, anchored lines, and width, bottom to top.
    fn popovers(
        &self,
        cx: &TestAppContext,
    ) -> Vec<(ExtensionKey, String, Range<u32>, PopoverWidth)> {
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
                        popover.spec.width,
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
            (first, "hunk".to_owned(), 0..3, PopoverWidth::Pixels(90.)),
            (second, "hunk".to_owned(), 1..2, PopoverWidth::Pixels(60.)),
            (first, "hover".to_owned(), 2..3, PopoverWidth::Pixels(40.)),
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
        [(second, "hunk".to_owned(), 1..2, PopoverWidth::Pixels(60.))]
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
        [(owner, "hunk".to_owned(), 2..3, PopoverWidth::Pixels(60.))]
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
    let contents = cx.read(|cx| fixture.editor.read(cx).popover_contents());
    assert_eq!(contents.len(), 1);
    let (below_anchor, popover, text) = cx.read(|cx| {
        let leaves = contents[0].read(cx).leaves();
        (
            view.read(cx).line_top(2, cx),
            leaves[0].editor().unwrap().read(cx).interaction_bounds(),
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

/// The views of the popover `index` of the fixture's editor, in layout order.
fn leaves(
    fixture: &Fixture,
    index: usize,
    cx: &TestAppContext,
) -> Vec<crate::app::view_layout::Leaf> {
    cx.read(|cx| {
        fixture.editor.read(cx).popover_contents()[index]
            .read(cx)
            .leaves()
    })
}

#[gpui::test]
fn a_popover_holds_a_layout_of_views_and_an_update_keeps_the_keyed_ones(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    load("rich", 905, cx);
    fixture.run("rich.open-rich", cx);
    let first = leaves(&fixture, 0, cx);
    assert_eq!(first.len(), 3);

    // `a` stays, the row with `b` and the document editor goes, and an
    // unkeyed editor arrives.
    fixture.run("rich.update-rich", cx);
    let second = leaves(&fixture, 0, cx);
    assert_eq!(second.len(), 2);
    assert_eq!(second[0].entity_id(), first[0].entity_id());
    assert!(
        first[1..]
            .iter()
            .all(|old| old.entity_id() != second[1].entity_id())
    );
    assert_eq!(fixture.popovers(cx).len(), 1);
}

#[gpui::test]
fn a_popover_is_a_child_of_its_editor_in_the_view_tree(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    load("tree", 906, cx);
    fixture.run("tree.open-rich", cx);
    fixture.run("tree.tree", cx);

    // While no view of the popover has focus, the editor does.
    let info = super::composite::info(&fixture.window, cx);
    let root = &info.panes[0].tabs[0].root;
    assert_eq!(info.focused_view.as_ref(), Some(&root.target));

    // Focus inside the popover makes its view the window's focused one.
    let first = leaves(&fixture, 0, cx)[0].clone();
    let content = cx.read(|cx| fixture.editor.read(cx).popover_contents()[0].clone());
    content.update(cx, |content, cx| content.focus_leaf(0, cx));
    let info = super::composite::info(&fixture.window, cx);
    let focused = info.focused_view.unwrap();
    assert_eq!(focused.view, first.entity_id().as_u64().to_string());
    assert_ne!(Some(&focused), Some(&info.panes[0].tabs[0].root.target));
}

#[gpui::test]
fn a_toolbar_button_runs_its_command_on_the_anchoring_editor(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    load("tool", 907, cx);
    fixture.run("tool.open-rich", cx);
    cx.update_window(fixture.native_window, |_, window, _| window.refresh())
        .unwrap();
    // Focus is inside the popover, yet the command's editor is the anchoring
    // one, so its closing the popover through that editor works.
    let content = cx.read(|cx| fixture.editor.read(cx).popover_contents()[0].clone());
    content.update(cx, |content, cx| content.focus_leaf(1, cx));
    super::composite::click_toolbar_button(fixture.native_window, "popover-toolbar-0", cx);
    cx.run_until_parked();
    assert!(fixture.popovers(cx).is_empty());
    fixture.run("tool.expect-tool", cx);
}

#[gpui::test]
fn an_editor_on_a_document_in_a_popover_counts_as_a_view_of_it(cx: &mut TestAppContext) {
    use crate::app::editor::PopoverContentSpec;
    use crate::app::view_layout::{Arrangement, ContentSpec, GroupSpec, LayoutSize, MemberSpec};

    let fixture = Fixture::new(cx);
    let documents = cx.read(|cx| {
        cx.global::<crate::app::documents::ApplicationDocuments>()
            .0
            .clone()
    });
    let other = cx.update(|cx| create_untitled_document(&documents, cx));
    let other_model = cx.read(|cx| documents.read(cx).get(other).unwrap().model().clone());
    let count =
        |cx: &TestAppContext| cx.read(|cx| cx.global::<ApplicationWindows>().view_count(other, cx));
    assert_eq!(count(cx), 0);

    let owner = ExtensionKey::new(ExtensionId::new(908), ExtensionLifecycleId::new(1));
    let window = crate::app::product::window_id(fixture.window.entity_id());
    let spec = PopoverContentSpec {
        spec: knot_model::editor::popovers::PopoverSpec {
            anchor: 0..1,
            width: PopoverWidth::Editor,
        },
        toolbar: Vec::new(),
        layout: GroupSpec {
            arrangement: Arrangement::Column,
            members: vec![MemberSpec {
                key: None,
                size: LayoutSize::default(),
                focused: false,
                title: None,
                limit: None,
                content: ContentSpec::Editor {
                    buffer: other_model,
                    document: Some(other),
                },
            }],
        },
    };
    fixture.editor.update(cx, |editor, cx| {
        editor.open_popover(owner, "doc", &window, spec, cx)
    });
    assert_eq!(count(cx), 1);
    fixture.editor.update(cx, |editor, cx| {
        editor.close_popover(owner, "doc", cx);
    });
    assert_eq!(count(cx), 0);
}
