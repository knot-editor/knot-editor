import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { commands, windows } from "knot";
import * as host from "knot/testing";
import { COMMAND, SEARCH_EVERYWHERE_COMMAND, SEARCH_ICON, start } from "../src/extension.js";

const ROOT = "file:///work/app";

beforeEach(async () => {
  host.reset();
  host.seedFile(`${ROOT}/src/main.ts`, "main");
  host.seedFile(`${ROOT}/src/ui/button.ts`, "button");
  host.seedFile(`${ROOT}/README.md`, "readme");
  host.openWindow({ roots: [ROOT] });
  await start();
});

test("each query lists the matching files with name and directory", async () => {
  host.query("butt");
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(COMMAND);

  const [shown] = host.prompts();
  assert.equal((shown?.options as { initialTab: string }).initialTab, "files");
  const [all, matched] = host.pickerRows();
  assert.equal(all?.length, 3);
  assert.deepEqual(matched?.map((row) => [row.label, row.description]), [["button.ts", "src/ui"]]);
  assert.deepEqual(matched?.[0]?.labelMatches, [[0, 4]]);
  assert.deepEqual(matched?.[0]?.icon, { file: "button.ts" });
});

test("choosing a row opens that file", async () => {
  host.answer("showPicker", { kind: "selected", id: `file::${ROOT}/src/main.ts` });
  await host.run(COMMAND);

  assert.deepEqual(
    host.documentIds().map((id) => host.document(id).resourceUri),
    [`${ROOT}/src/main.ts`],
  );
});

test("dismissing opens nothing", async () => {
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(COMMAND);

  assert.equal(host.documentIds().length, 0);
});

test("a window without roots lists nothing", async () => {
  host.reset();
  host.openWindow();
  await start();
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(COMMAND);

  assert.deepEqual(host.pickerRows(), [[]]);
});

test("several roots are ranked as one, each hit's root name starting its description", async () => {
  host.seedFile("file:///work/lib/util.ts", "util");
  host.seedFile("file:///work/lib/button.ts", "button");
  host.setRoots(host.windowTargets()[0]!, [ROOT, "file:///work/lib"]);
  host.query("butt");
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(COMMAND);

  const [all, matched] = host.pickerRows();
  assert.deepEqual(all?.map((row) => row.description), ["app", "app/src", "app/src/ui", "lib", "lib"]);
  assert.deepEqual(matched?.map((row) => [row.label, row.description]), [
    ["button.ts", "app/src/ui"],
    ["button.ts", "lib"],
  ]);
});

test("a toolbar search button runs Go to File", async () => {
  const [window] = host.windowTargets();
  assert.deepEqual(
    host.toolbarItems(window!).map((item) => [item.id, item.text, item.icon, item.side, item.command?.name]),
    [["go-to-file", "", SEARCH_ICON, "right", COMMAND]],
  );
});

test("Search Everywhere asks for each tab and lists files and actions", async () => {
  await commands.register("demo.save", async () => {}, {
    title: "Save Everything",
    description: "Save all",
  });
  host.selectTab("files", "actions");
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(SEARCH_EVERYWHERE_COMMAND);

  const [shown] = host.prompts();
  const options = shown?.options as { title: string; tabs: { id: string }[] };
  assert.deepEqual(options.tabs.map((tab) => tab.id), ["all", "buffers", "files", "actions"]);
  const [all, files, actions] = host.pickerRows();
  assert.equal(all?.length, 3, "an empty All tab lists files only");
  assert.deepEqual(files?.map((row) => row.label), all?.map((row) => row.label));
  assert.ok(actions?.some((row) => row.id === "command::demo.save" && row.label === "Save Everything"));
  assert.ok(actions?.every((row) => row.id.startsWith("command:")));
});

test("typing lists matching files, then actions, in the All tab", async () => {
  await commands.register("demo.main", async () => {}, { title: "Main Action" });
  host.query("main");
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(SEARCH_EVERYWHERE_COMMAND);

  const typed = host.pickerRows()[1];
  assert.deepEqual(typed?.map((row) => row.label), ["main.ts", "Main Action"]);
  assert.deepEqual(typed?.[1]?.labelMatches, [[0, 4]]);
  assert.equal(typed?.[1]?.icon, undefined);
});

test("choosing an action runs it and choosing a file opens it", async () => {
  let ran = 0;
  await commands.register("demo.run", async () => { ran += 1; }, { title: "Run Me" });
  host.answer("showPicker", { kind: "selected", id: "command::demo.run" });
  await host.run(SEARCH_EVERYWHERE_COMMAND);
  assert.equal(ran, 1);

  host.answer("showPicker", { kind: "selected", id: `file::${ROOT}/README.md` });
  await host.run(SEARCH_EVERYWHERE_COMMAND);
  assert.deepEqual(
    host.documentIds().map((id) => host.document(id).resourceUri),
    [`${ROOT}/README.md`],
  );
});

test("actions are ordered by title, recently run ones first, and recency breaks ties", async () => {
  for (const [name, title] of [["demo.a", "Alpha"], ["demo.b", "Save"], ["demo.c", "Gamma"], ["demo.d", "Save"]] as const) {
    await commands.register(name, async () => {}, { title });
  }
  host.answer("showPicker", { kind: "selected", id: "command::demo.d" });
  await host.run(SEARCH_EVERYWHERE_COMMAND);

  const [empty, typed] = (await search("actions", "save")).slice(-2);
  const demo = new Set(["Alpha", "Save", "Gamma"]);
  assert.deepEqual(empty?.filter((label) => demo.has(label)), ["Save", "Alpha", "Gamma", "Save"]);
  assert.deepEqual(typed?.filter((label) => demo.has(label)), ["Save", "Save"]);
  assert.deepEqual(
    host.pickerRows().slice(-1)[0]?.filter((row) => row.label === "Save").map((row) => row.id),
    ["command::demo.d", "command::demo.b"],
  );
});

test("an action matches on its name though the name is not shown", async () => {
  await commands.register("demo.frobnicate", async () => {}, { title: "Tidy Up" });
  const [, matched] = await search("actions", "frobn");
  assert.deepEqual(matched, ["Tidy Up"]);
});

test("an action shows the binding of the view the picker opened from, and none for the pickers", async () => {
  await commands.register("demo.copy", async () => {}, { title: "Copy It" });
  await commands.register("workbench.show-command-palette", async () => {}, { title: "Show Command Palette" });
  await activate(`${ROOT}/src/main.ts`);
  host.bindKey("demo.copy", "⌘C");
  host.selectTab("actions");
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(SEARCH_EVERYWHERE_COMMAND);

  const actions = host.pickerRows()[1];
  assert.equal(actions?.find((row) => row.label === "Copy It")?.hint, "⌘C");
  assert.deepEqual(
    actions?.filter((row) => /Search Everywhere|Go to File|Command Palette/.test(row.label)),
    [],
  );
});

/** Opens the file in a tab and reports it as the active one. */
async function activate(uri: string | null, text = "") {
  const [window] = host.windowTargets();
  const id = host.addDocument({ ...(uri === null ? {} : { uri }), text });
  host.present(window!, id);
  await host.emit({
    type: "activeTargetChanged",
    activeWindow: window!,
    activeTarget: host.tabTarget(window!, host.documentsIn(window!).length - 1),
  });
  return id;
}

async function search(tab: string | undefined, ...queries: string[]) {
  host.query(...queries);
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(SEARCH_EVERYWHERE_COMMAND, { arguments: tab === undefined ? null : { tab } });
  return host.pickerRows().map((rows) => rows.map((row) => row.label));
}

test("Go to File and Search Everywhere start on their own tabs", async () => {
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(SEARCH_EVERYWHERE_COMMAND);
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(SEARCH_EVERYWHERE_COMMAND, { arguments: { tab: "actions" } });
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(SEARCH_EVERYWHERE_COMMAND, { arguments: { tab: "nowhere" } });
  const initial = host.prompts().map((prompt) => (prompt.options as { initialTab?: string }).initialTab);
  assert.deepEqual(initial, [undefined, "actions", undefined]);
});

test("the Buffers tab lists open documents, the current one last", async () => {
  await activate(`${ROOT}/src/main.ts`);
  await activate(null);
  await activate(`${ROOT}/README.md`);
  await activate(`${ROOT}/src/ui/button.ts`);
  host.selectTab("buffers");

  const [, buffers] = await search(undefined);
  assert.deepEqual(buffers, ["README.md", "Untitled", "main.ts", "button.ts"]);
});

test("Buffers match on the title", async () => {
  await activate(`${ROOT}/src/main.ts`);
  await activate(`${ROOT}/README.md`);
  host.selectTab("buffers");

  const [, , matched] = await search(undefined, "main");
  assert.deepEqual(matched, ["main.ts"]);
});

test("choosing a buffer shows that document", async () => {
  const main = await activate(`${ROOT}/src/main.ts`);
  await activate(`${ROOT}/README.md`);
  host.answer("showPicker", { kind: "selected", id: `buffer::${main}` });
  await host.run(SEARCH_EVERYWHERE_COMMAND, { arguments: { tab: "buffers" } });

  const info = await windows.get(host.windowTargets()[0]!);
  const pane = info!.panes[0]!;
  assert.equal(pane.tabs.find((tab) => tab.target === pane.selected)?.document, main);
});

test("All lists buffers, then files, once each", async () => {
  await activate(`${ROOT}/src/main.ts`, "x");
  await activate(`${ROOT}/README.md`);

  const [all] = await search("all");
  assert.deepEqual(all, ["main.ts", "README.md", "button.ts"]);
});

test("a bare :N lists only the line row, and text:N matches on the text", async () => {
  await activate(`${ROOT}/src/main.ts`, "a\nb\nc\n");
  const [empty, colon, bare, withText, noLine] = await search("files", ":", ":2", "butt:3", "butt:");
  assert.deepEqual(empty?.includes("Go to line 2"), false);
  assert.deepEqual(colon, ["Go to line…"]);
  assert.deepEqual(bare, ["Go to line 2"]);
  assert.deepEqual(withText, ["button.ts"]);
  assert.deepEqual(noLine, ["button.ts"]);
});

test("choosing the line row goes to that line of the current editor", async () => {
  await activate(`${ROOT}/src/main.ts`, "ab\ncd\nef\n");
  host.answer("showPicker", { kind: "selected", id: "line:2:" });
  await host.run(SEARCH_EVERYWHERE_COMMAND);
  assert.equal(host.editorView().caret, 3);
  assert.equal(host.editorReveal(), "center");
});

test("choosing a buffer or file with a line goes there, clamped to the text", async () => {
  const main = await activate(`${ROOT}/src/main.ts`, "ab\ncd\nef");
  await activate(`${ROOT}/README.md`);
  host.answer("showPicker", { kind: "selected", id: `buffer:3:${main}` });
  await host.run(SEARCH_EVERYWHERE_COMMAND);
  assert.equal(host.editorView().caret, 6);

  host.answer("showPicker", { kind: "selected", id: `file:99:${ROOT}/src/main.ts` });
  await host.run(SEARCH_EVERYWHERE_COMMAND);
  assert.equal(host.editorView().caret, 6);
});
