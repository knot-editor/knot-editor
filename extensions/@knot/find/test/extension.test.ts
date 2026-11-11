import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { windows, workbench } from "knot";
import type { DocumentId, WindowTarget } from "knot";
import * as host from "knot/testing";
import { CLOSE, CURRENT_COLOR, FIND, MATCH_COLOR, NEXT, PREVIOUS, start } from "../src/extension.js";

const TEXT = "foo bar\nFoo baz foo\nfoofoo\n";
const offset = (text: string) => new TextEncoder().encode(text).length;

async function setup(text = TEXT) {
  host.reset();
  const document = host.addDocument({ text });
  const window = host.openWindow({ documents: [document] });
  await start();
  const editor = await workbench.editor(host.tabTarget(window, 0));
  const tab = (await windows.get(window))!.panes[0]!.tabs[0]!;
  return { document, window, editor, buffer: tab.buffer! };
}

/** The editor's selection as the text it covers. */
async function selected(editor: Awaited<ReturnType<typeof setup>>["editor"], text: string) {
  const { anchor, caret } = await editor.selection();
  const bytes = new TextEncoder().encode(text);
  return new TextDecoder().decode(bytes.slice(Math.min(anchor, caret), Math.max(anchor, caret)));
}

function message(window: WindowTarget) {
  return host.editorBars(window).bars[0]?.message;
}

/** Highlights as [start, current] pairs of byte offsets. */
function highlights(document: DocumentId) {
  return host.contributions(document).map((item) => [
    item.range.startByteOffset,
    item.decoration?.color === CURRENT_COLOR,
  ]);
}

async function search(window: WindowTarget, query: string) {
  await host.run(FIND, { window });
  await host.typeInBar("find", "query", query, window);
}

beforeEach(() => host.reset());

test("find binds its keys in the bar", async () => {
  await start();
  const bound = host.bindings().map((binding) => [binding.key, binding.command, binding.view, binding.context]);
  assert.deepEqual(bound, [
    ["enter", NEXT, "bar", "find"],
    ["shift-enter", PREVIOUS, "bar", "find"],
    ["escape", CLOSE, "bar", "find"],
  ]);
});

test("typing highlights every match and selects the first from the caret", async () => {
  const { document, window, editor } = await setup();
  await search(window, "foo");

  assert.equal(message(window), "1 of 5");
  assert.equal(await selected(editor, TEXT), "foo");
  assert.deepEqual(highlights(document), [
    [0, true],
    [offset("foo bar\n"), false],
    [offset("foo bar\nFoo baz "), false],
    [offset("foo bar\nFoo baz foo\n"), false],
    [offset("foo bar\nFoo baz foo\nfoo"), false],
  ]);
  assert.equal(host.contributions(document)[1]!.decoration?.color, MATCH_COLOR);
  assert.deepEqual(host.editorBars(window).focus, { bar: "find", field: "query" });

  await host.typeInBar("find", "query", "baz", window);
  assert.equal(message(window), "1 of 1");
  assert.equal(await selected(editor, TEXT), "baz");

  await host.typeInBar("find", "query", "missing", window);
  assert.equal(message(window), "No results");
  assert.deepEqual(host.contributions(document), []);

  await host.typeInBar("find", "query", "", window);
  assert.equal(message(window), null);
});

test("next and previous step through the matches and wrap", async () => {
  const { window, editor, document } = await setup();
  await search(window, "foo");
  const at = async () => (await editor.selection()).anchor;

  await host.run(NEXT, { window });
  assert.equal(await at(), offset("foo bar\n"));
  assert.equal(message(window), "2 of 5");
  assert.deepEqual(highlights(document).filter(([, current]) => current), [[offset("foo bar\n"), true]]);

  for (let step = 0; step < 4; step += 1) await host.run(NEXT, { window });
  assert.equal(await at(), 0);
  assert.equal(message(window), "1 of 5");

  await host.run(PREVIOUS, { window });
  assert.equal(await at(), offset("foo bar\nFoo baz foo\nfoo"));
  assert.equal(message(window), "5 of 5");
  await host.run(PREVIOUS, { window });
  assert.equal(await at(), offset("foo bar\nFoo baz foo\n"));
});

test("matching ignores case, and a find that cannot match steps nowhere", async () => {
  const { window, editor } = await setup();
  await search(window, "FOO");
  assert.equal(await selected(editor, TEXT), "foo");
  await search(window, "none");
  const before = await editor.selection();
  await host.run(NEXT, { window });
  assert.deepEqual(await editor.selection(), before);
});

test("more than 1000 matches highlight the first 1000 and stepping still reaches the rest", async () => {
  const text = "x\n".repeat(1500);
  const { window, editor, document } = await setup(text);
  await search(window, "x");
  assert.equal(message(window), "1000+");
  assert.equal(host.contributions(document).length, 1000);

  await editor.setSelection({ anchor: offset("x\n".repeat(1200)), caret: offset("x\n".repeat(1200)) });
  await host.run(NEXT, { window });
  assert.equal((await editor.selection()).anchor, offset("x\n".repeat(1200)));
  await host.run(NEXT, { window });
  assert.equal((await editor.selection()).anchor, offset("x\n".repeat(1201)));

  await host.run(PREVIOUS, { window });
  await host.run(PREVIOUS, { window });
  assert.equal((await editor.selection()).anchor, offset("x\n".repeat(1199)));

  await editor.setSelection({ anchor: 0, caret: 0 });
  await host.run(PREVIOUS, { window });
  assert.equal((await editor.selection()).anchor, offset("x\n".repeat(1499)));
});

test("edits search again and the current match follows the selection", async () => {
  const { window, document, buffer, editor } = await setup();
  await search(window, "baz");
  assert.equal(message(window), "1 of 1");

  const { revision } = await buffer.snapshot();
  await buffer.applyEdits([{ range: { startByteOffset: 0, endByteOffset: 0 }, text: "baz " }], { ifRevision: revision });
  assert.equal(message(window), "2 results");
  assert.equal(host.contributions(document).length, 2);

  const text = host.document(document).text;
  const start = offset(text.slice(0, text.indexOf("baz", 1)));
  await editor.setSelection({ anchor: start, caret: start + 3 });
  const { revision: next } = await buffer.snapshot();
  await buffer.applyEdits([{ range: { startByteOffset: 0, endByteOffset: 0 }, text: "x" }], { ifRevision: next });
  assert.equal(message(window), "2 results");
});

test("closing removes the bar and the highlights and keeps the current match selected", async () => {
  const { window, document, editor } = await setup();
  await search(window, "baz");
  await host.run(CLOSE, { window });

  assert.deepEqual(host.editorBars(window).bars, []);
  assert.deepEqual(host.contributions(document), []);
  assert.equal(await selected(editor, TEXT), "baz");
});

test("next and previous with the bar closed use the last query and highlight nothing", async () => {
  const { window, document, editor } = await setup();
  await search(window, "foo");
  await host.run(CLOSE, { window });
  await editor.setSelection({ anchor: 0, caret: 0 });

  await host.run(NEXT, { window });
  assert.equal((await editor.selection()).anchor, 0);
  await host.run(NEXT, { window });
  assert.equal((await editor.selection()).anchor, offset("foo bar\n"));
  assert.deepEqual(host.editorBars(window).bars, []);
  assert.deepEqual(host.contributions(document), []);
  await host.run(PREVIOUS, { window });
  assert.equal((await editor.selection()).anchor, 0);
});

test("find fills the field from a single-line selection, else from the last query", async () => {
  const { window, editor } = await setup();
  await search(window, "baz");
  await host.run(CLOSE, { window });

  await host.run(FIND, { window });
  assert.deepEqual(await (await workbench.editor(host.tabTarget(window, 0))).barFields("find"), { query: "baz" });

  await host.run(CLOSE, { window });
  const start = offset("foo bar\nFoo ");
  await editor.setSelection({ anchor: start, caret: start + 6 });
  await host.run(FIND, { window });
  assert.deepEqual(await editor.barFields("find"), { query: "baz fo" });

  await host.run(CLOSE, { window });
  await editor.setSelection({ anchor: 0, caret: offset("foo bar\nFoo") });
  await host.run(FIND, { window });
  assert.deepEqual(await editor.barFields("find"), { query: "baz fo" });
});

test("a command without an editor is unavailable", async () => {
  await setup();
  const window = host.openWindow();
  assert.equal(await host.run(FIND, { window }), "unavailable");
  assert.equal(await host.run(NEXT, { window }), "unavailable");
});
