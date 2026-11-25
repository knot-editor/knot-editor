// Development playground for the D229 composite-view plan: commands that open
// composite views so their behavior can be checked by hand. Run it with
// scripts/dev/playground.sh, which points Knot at this directory as its
// personal config. Removed with the plan.
import { buffers, commands, documents, icons, workbench } from "knot";

let kind;
const composites = async () =>
  (kind ??= await workbench.registerComposite({ id: "playground.composite" }));

// Open two files (or untitled documents) first. A row of editors on the first
// document twice and on the second once; edit them to see the unsaved marker
// and the closing prompts: one per dirty document the tab is the last view of.
await commands.register("playground.composite-documents", async () => {
  const [first, second] = await documents.list();
  if (first === undefined) return;
  const members = [{ type: "editor", document: first.id }, { type: "editor", document: first.id }];
  members.push(
    second === undefined
      ? { type: "editor", buffer: await buffers.create({ text: "no second document open\n" }) }
      : { type: "editor", document: second.id },
  );
  await (await composites()).open({ title: "Composite on documents", icon: "file", layout: { type: "row", children: members } });
}, { title: "Playground: Open Composite on Documents" });

// Editors on extension buffers: a column of an editor and a row of two.
await commands.register("playground.composite-buffers", async () => {
  const [a, b, c] = await Promise.all(["one\n", "two\n", "three\n"].map((text) => buffers.create({ text })));
  await (await composites()).open({
    title: "Composite on buffers",
    layout: {
      type: "column",
      children: [
        { type: "editor", buffer: a },
        { type: "row", children: [{ type: "editor", buffer: b }, { type: "editor", buffer: c }] },
      ],
    },
  });
}, { title: "Playground: Open Composite on Buffers" });

// A composite whose layout "Playground: Next Layout" cycles through. Type in
// each editor, move the caret, then cycle: keyed editors keep their text
// position, scroll, undo history, and focus when they move or regroup; the
// third layout drops `c`, so focus moves to the layout's initial editor when
// `c` had it (the command then ends as InvalidTarget, as one that closes its
// own view does).
let cycling;
let step = 0;
const layouts = (a, b, c) => {
  const editor = (key, buffer, rest = {}) => ({ type: "editor", key, buffer, ...rest });
  return [
    { type: "column", children: [editor("a", a), { type: "row", key: "pair", children: [editor("b", b), editor("c", c)] }] },
    { type: "row", children: [editor("c", c), { type: "column", key: "pair", children: [editor("a", a), editor("b", b)] }] },
    { type: "row", children: [editor("b", b), editor("a", a, { focused: true })] },
  ];
};
await commands.register("playground.composite-keyed", async () => {
  const buffersOf = await Promise.all(["alpha\n", "bravo\n", "charlie\n"].map((text) => buffers.create({ text })));
  step = 0;
  cycling = { buffers: buffersOf, instance: await (await composites()).open({
    title: "Keyed composite",
    layout: layouts(...buffersOf)[0],
  }) };
}, { title: "Playground: Open Keyed Composite" });
await commands.register("playground.composite-next-layout", async () => {
  if (cycling === undefined) return;
  step = (step + 1) % 3;
  await cycling.instance.update({ layout: layouts(...cycling.buffers)[step] });
}, { title: "Playground: Next Layout" });

// A toolbar: "Count" counts presses in the tab title and shows pressed while
// on, "Toggle" flips the first button's pressed state, and "Off" is disabled.
// Click a button with focus in another tab or tool window: the command still
// runs with this composite as its target (the title counts the presses).
await icons.define("playground.count", { lucide: "plus" });
await icons.define("playground.toggle", { lucide: "toggle-left" });
await icons.define("playground.off", { lucide: "lock" });
await commands.register("playground.composite-toolbar", async () => {
  const buffer = await buffers.create({ text: "toolbar\n" });
  let presses = 0;
  let on = false;
  const buttons = () => [
    { label: "Count", icon: "playground.count", command: "playground.toolbar-count", toggled: on },
    { label: "Toggle", icon: "playground.toggle", command: "playground.toolbar-toggle" },
    { label: "Off", icon: "playground.off", command: "playground.toolbar-count", disabled: true },
  ];
  const instance = await (await composites()).open({
    title: "Toolbar",
    layout: { type: "row", children: [{ type: "editor", buffer }] },
    toolbar: buttons(),
  });
  const refresh = () => instance.update({ title: `Toolbar (${presses})`, toolbar: buttons() });
  toolbar = {
    count: async (context) => {
      presses += 1;
      await refresh();
    },
    toggle: async () => {
      on = !on;
      await refresh();
    },
  };
}, { title: "Playground: Open Composite with Toolbar" });
let toolbar;
await commands.register("playground.toolbar-count", (context) => toolbar?.count(context));
await commands.register("playground.toolbar-toggle", () => toolbar?.toggle());

// Block rows: shows three lines of a Rust buffer above the focused editor's
// second line and one above its last, banded as removed lines. Try it in a
// document, in a composite's editor, and in a diff side; wrap, edit above the
// rows, scroll, and click a row (the caret goes to the line below). "Edit
// Source" changes the source's text: the rows follow it.
let source;
await commands.register("playground.block-rows", async ({ editor }) => {
  if (editor === undefined) return;
  source ??= await buffers.create({ text: "fn removed() {\n    let x = 1; // gone\n}\n", language: "rust" });
  await editor.setBlocks([
    { line: 1, source, lines: { start: 0, end: 3 }, color: "diff.removed.background" },
    { line: 9999, source, lines: { start: 1, end: 2 }, color: "diff.removed.background" },
  ]);
}, { title: "Playground: Show Block Rows" });
await commands.register("playground.block-rows-edit", async () => {
  if (source === undefined) return;
  const { revision } = await source.snapshot();
  await source.applyEdits([{ range: { startByteOffset: 0, endByteOffset: 0 }, text: "// edited\n" }], { ifRevision: revision });
}, { title: "Playground: Edit Block Rows Source" });
await commands.register("playground.block-rows-clear", async ({ editor }) => {
  await editor?.setBlocks([]);
}, { title: "Playground: Clear Block Rows" });

console.info("playground loaded");
