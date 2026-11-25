// Development playground for the D229 composite-view plan: commands that open
// composite views so their behavior can be checked by hand. Run it with
// scripts/dev/playground.sh, which points Knot at this directory as its
// personal config. Removed with the plan.
import { buffers, commands, documents, workbench } from "knot";

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

console.info("playground loaded");
