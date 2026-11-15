import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { windows, workbench } from "knot";
import type { DocumentId, WindowTarget } from "knot";
import * as host from "knot/testing";
import {
  CLOSE,
  CURRENT_COLOR,
  FIND,
  MATCH_COLOR,
  NEXT,
  PREVIOUS,
  REPLACE,
  REPLACE_ALL,
  REPLACE_CURRENT,
  SCOPE_COLOR,
  TOGGLE_CASE,
  TOGGLE_REGEX,
  TOGGLE_SCOPE,
  TOGGLE_WORD,
  start,
} from "../src/extension.js";

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
function highlights(window: WindowTarget) {
  return host.viewDecorations(window).filter((item) => item.decoration?.color !== SCOPE_COLOR).map((item) => [
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
    ["cmd-alt-c", TOGGLE_CASE, "bar", "find"],
    ["cmd-alt-w", TOGGLE_WORD, "bar", "find"],
    ["cmd-alt-r", TOGGLE_REGEX, "bar", "find"],
    ["cmd-alt-s", TOGGLE_SCOPE, "bar", "find"],
    ["escape", CLOSE, "bar", "find"],
    ["cmd-alt-c", TOGGLE_CASE, "bar", "replace"],
    ["cmd-alt-w", TOGGLE_WORD, "bar", "replace"],
    ["cmd-alt-r", TOGGLE_REGEX, "bar", "replace"],
    ["cmd-alt-s", TOGGLE_SCOPE, "bar", "replace"],
    ["escape", CLOSE, "bar", "replace"],
    ["enter", NEXT, "bar", "find"],
    ["shift-enter", PREVIOUS, "bar", "find"],
    ["enter", REPLACE_CURRENT, "bar", "replace"],
    ["cmd-enter", REPLACE_ALL, "bar", "replace"],
  ]);
});

test("typing highlights every match and selects the first from the caret", async () => {
  const { document, window, editor } = await setup();
  await search(window, "foo");

  assert.equal(message(window), "1 of 5");
  assert.equal(await selected(editor, TEXT), "foo");
  assert.deepEqual(highlights(window), [
    [0, true],
    [offset("foo bar\n"), false],
    [offset("foo bar\nFoo baz "), false],
    [offset("foo bar\nFoo baz foo\n"), false],
    [offset("foo bar\nFoo baz foo\nfoo"), false],
  ]);
  assert.equal(host.viewDecorations(window)[1]!.decoration?.color, MATCH_COLOR);
  assert.deepEqual(host.editorBars(window).focus, { bar: "find", field: "query" });

  await host.typeInBar("find", "query", "baz", window);
  assert.equal(message(window), "1 of 1");
  assert.equal(await selected(editor, TEXT), "baz");

  await host.typeInBar("find", "query", "missing", window);
  assert.equal(message(window), "No results");
  assert.deepEqual(host.viewDecorations(window), []);

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
  assert.deepEqual(highlights(window).filter(([, current]) => current), [[offset("foo bar\n"), true]]);

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
  assert.equal(host.viewDecorations(window).length, 1000);

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
  assert.equal(host.viewDecorations(window).length, 2);

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
  assert.deepEqual(host.viewDecorations(window), []);
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
  assert.deepEqual(host.viewDecorations(window), []);
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

test("the toggles switch case and whole word and search again", async () => {
  const { window, editor } = await setup();
  await search(window, "foo");
  const bar = () => host.editorBars(window).bars[0]!;
  assert.deepEqual(bar().toggles.map((toggle) => [toggle.id, toggle.active]), [["case", false], ["word", false], ["regex", false], ["scope", false]]);
  assert.equal(message(window), "1 of 5");

  await host.clickBarControl("find", "case", window);
  assert.equal(bar().toggles[0]!.active, true);
  assert.equal(message(window), "1 of 4");
  await host.clickBarControl("find", "word", window);
  assert.equal(bar().toggles[1]!.active, true);
  assert.equal(message(window), "1 of 2");
  assert.equal(await selected(editor, TEXT), "foo");

  await host.run(TOGGLE_CASE, { window });
  await host.run(TOGGLE_WORD, { window });
  assert.deepEqual(bar().toggles.map((toggle) => toggle.active), [false, false, false, false]);
  assert.equal(message(window), "1 of 5");
});

test("the buttons step and close", async () => {
  const { window, document, editor } = await setup();
  await search(window, "foo");
  const bar = () => host.editorBars(window).bars[0]!;
  assert.deepEqual(bar().buttons.map((button) => button.id), ["previous", "next", "close"]);

  await host.clickBarControl("find", "next", window);
  assert.equal(message(window), "2 of 5");
  await host.clickBarControl("find", "previous", window);
  assert.equal(message(window), "1 of 5");
  await host.clickBarControl("find", "close", window);
  assert.deepEqual(host.editorBars(window).bars, []);
  assert.deepEqual(host.viewDecorations(window), []);
  assert.equal(await selected(editor, TEXT), "foo");
});

test("the regex toggle searches a pattern and shows an invalid one's message", async () => {
  const { window, editor } = await setup("foo1 foo22 bar\nfoo\n");
  await search(window, "foo\\d+");
  assert.equal(message(window), "No results");

  await host.clickBarControl("find", "regex", window);
  assert.equal(host.editorBars(window).bars[0]!.toggles[2]!.active, true);
  assert.equal(message(window), "1 of 2");
  assert.equal(host.viewDecorations(window).length, 2);

  await host.typeInBar("find", "query", "foo(", window);
  assert.match(message(window)!, /invalid regular expression/);
  assert.deepEqual(host.viewDecorations(window), []);
  await host.run(NEXT, { window });

  await host.typeInBar("find", "query", "^foo", window);
  assert.equal(message(window), "1 of 2");
  await host.run(TOGGLE_REGEX, { window });
  assert.equal(message(window), "No results");
});

test("replace opens a bar beneath find, and closing one closes both", async () => {
  const { window } = await setup();
  await host.run(REPLACE, { window });
  const ids = () => host.editorBars(window).bars.map((bar) => bar.id);
  assert.deepEqual(ids(), ["find", "replace"]);
  assert.deepEqual(host.editorBars(window).focus, { bar: "find", field: "query" });
  assert.deepEqual(host.editorBars(window).bars[1]!.buttons.map((button) => button.id), ["replace", "replace-all"]);

  // Find leaves the Replace bar alone.
  await host.run(FIND, { window });
  assert.deepEqual(ids(), ["find", "replace"]);

  await host.run(CLOSE, { window });
  assert.deepEqual(ids(), []);
  await host.run(FIND, { window });
  assert.deepEqual(ids(), ["find"]);
});

test("replace advances over matches, replaces the selected one, and wraps", async () => {
  const { window, document, editor } = await setup("a1 b a2 a3\n");
  await host.run(REPLACE, { window });
  await host.typeInBar("find", "query", "a\\d", window);
  await host.run(TOGGLE_REGEX, { window });
  await host.typeInBar("replace", "replacement", "#", window);
  assert.equal(message(window), "1 of 3");

  // The first match is selected: replace it and move to the next.
  await host.run(REPLACE_CURRENT, { window });
  const text = () => host.document(document).text;
  assert.equal(text(), "# b a2 a3\n");
  assert.equal(await selected(editor, text()), "a2");
  assert.equal(message(window), "1 of 2");

  // A selection that is not a match only advances.
  await editor.setSelection({ anchor: 0, caret: 1 });
  await host.run(REPLACE_CURRENT, { window });
  assert.equal(text(), "# b a2 a3\n");
  assert.equal(await selected(editor, text()), "a2");

  await host.clickBarControl("replace", "replace", window);
  await host.run(REPLACE_CURRENT, { window });
  assert.equal(text(), "# b # #\n");
  assert.equal(message(window), "No results");
});

test("replace all expands captures, reports the count, and clears when the text changes", async () => {
  const { window, document } = await setup("ann@1 bob@22 x@y\n");
  await host.run(REPLACE, { window });
  await host.run(TOGGLE_REGEX, { window });
  await host.typeInBar("find", "query", "(\\w+)@(\\d+)", window);
  await host.typeInBar("replace", "replacement", "$2:$1", window);

  await host.run(REPLACE_ALL, { window });
  assert.equal(host.document(document).text, "1:ann 22:bob x@y\n");
  assert.equal(host.editorBars(window).bars[1]!.message, "Replaced 2");
  assert.equal(message(window), "No results");

  await host.typeInBar("replace", "replacement", "z", window);
  assert.equal(host.editorBars(window).bars[1]!.message, null);
  await host.typeInBar("find", "query", "nothing", window);
  await host.clickBarControl("replace", "replace-all", window);
  assert.equal(host.document(document).text, "1:ann 22:bob x@y\n");
  assert.equal(host.editorBars(window).bars[1]!.message, "Replaced 0");

  // An invalid pattern replaces nothing and keeps the parser's message.
  await host.typeInBar("find", "query", "(", window);
  await host.run(REPLACE_ALL, { window });
  assert.equal(host.document(document).text, "1:ann 22:bob x@y\n");
  assert.match(message(window)!, /invalid regular expression/);
});

/** The scope's decoration as [start, end] byte offsets, or null. */
function scope(window: WindowTarget) {
  const item = host.viewDecorations(window).find((item) => item.decoration?.color === SCOPE_COLOR);
  return item === undefined ? null : [item.range.startByteOffset, item.range.endByteOffset];
}

const LINE_TWO = [offset("foo bar\n"), offset("foo bar\nFoo baz foo\n")];

test("find in selection limits highlights and stepping to the selection, wrapping inside it", async () => {
  const { window, editor } = await setup();
  await editor.setSelection({ anchor: LINE_TWO[0]!, caret: LINE_TWO[1]! });
  await host.run(FIND, { window });
  assert.deepEqual(host.editorBars(window).bars[0]!.toggles[3]!.active, true);
  await host.typeInBar("find", "query", "foo", window);
  assert.equal(message(window), "1 of 2");
  assert.deepEqual(scope(window), LINE_TWO);
  assert.deepEqual(highlights(window).map(([start]) => start), [LINE_TWO[0], offset("foo bar\nFoo baz ")]);

  await host.run(NEXT, { window });
  assert.equal(message(window), "2 of 2");
  await host.run(NEXT, { window });
  assert.equal(message(window), "1 of 2");
  await host.run(PREVIOUS, { window });
  assert.equal(message(window), "2 of 2");

  // Turning it off searches the whole buffer again.
  await host.clickBarControl("find", "scope", window);
  assert.equal(scope(window), null);
  assert.equal(message(window), "3 of 5");
  await host.run(CLOSE, { window });
});

test("find in selection with no selection scopes the caret's line, and closing drops it", async () => {
  const { window, editor } = await setup();
  await editor.setSelection({ anchor: offset("foo bar\nFoo"), caret: offset("foo bar\nFoo") });
  await host.run(FIND, { window });
  assert.equal(scope(window), null);
  await host.run(TOGGLE_SCOPE, { window });
  assert.deepEqual(scope(window), [LINE_TWO[0], LINE_TWO[1]! - 1]);
  await host.typeInBar("find", "query", "foo", window);
  assert.equal(message(window), "2 of 2");

  await host.run(CLOSE, { window });
  await host.run(FIND, { window });
  assert.equal(scope(window), null);
  assert.equal(host.editorBars(window).bars[0]!.toggles[3]!.active, false);
});

test("the scope follows edits and goes away when they delete it", async () => {
  const { window, editor, buffer, document } = await setup();
  await editor.setSelection({ anchor: LINE_TWO[0]!, caret: LINE_TWO[1]! });
  await host.run(FIND, { window });
  await host.typeInBar("find", "query", "foo", window);
  const revision = async () => (await buffer.find("", { limit: 1 })).revision;

  // Text before moves it; text inside and at its end joins it.
  await buffer.applyEdits([{ range: { startByteOffset: 0, endByteOffset: 0 }, text: ">>" }], {
    ifRevision: await revision(),
  });
  assert.deepEqual(scope(window), [LINE_TWO[0]! + 2, LINE_TWO[1]! + 2]);
  const end = LINE_TWO[1]! + 2;
  await buffer.applyEdits([{ range: { startByteOffset: end, endByteOffset: end }, text: "foo\n" }], {
    ifRevision: await revision(),
  });
  assert.deepEqual(scope(window), [LINE_TWO[0]! + 2, end + 4]);
  assert.equal(message(window), "3 results");

  // Deleting all of it lifts the limit.
  await buffer.applyEdits(
    [{ range: { startByteOffset: LINE_TWO[0]! + 2, endByteOffset: end + 4 }, text: "" }],
    { ifRevision: await revision() },
  );
  assert.equal(host.document(document).text, ">>foo bar\nfoofoo\n");
  assert.equal(scope(window), null);
  assert.equal(host.editorBars(window).bars[0]!.toggles[3]!.active, false);
  assert.equal(message(window), "3 results");
});

test("replace all and replace stay inside the scope, which follows the longer text", async () => {
  const { window, editor, document } = await setup();
  await editor.setSelection({ anchor: LINE_TWO[0]!, caret: LINE_TWO[1]! });
  await host.run(REPLACE, { window });
  await host.typeInBar("find", "query", "foo", window);
  await host.typeInBar("replace", "replacement", "quux", window);

  await host.run(REPLACE_CURRENT, { window });
  assert.equal(host.document(document).text, "foo bar\nquux baz foo\nfoofoo\n");
  assert.equal(message(window), "1 of 1");
  assert.deepEqual(scope(window), [LINE_TWO[0], LINE_TWO[1]! + 1]);

  await host.run(REPLACE_ALL, { window });
  assert.equal(host.document(document).text, "foo bar\nquux baz quux\nfoofoo\n");
  assert.equal(host.editorBars(window).bars[1]!.message, "Replaced 1");
  assert.deepEqual(scope(window), [LINE_TWO[0], LINE_TWO[1]! + 2]);
});

test("find in selection is a command that opens find when it is closed", async () => {
  const { window, editor } = await setup();
  await editor.setSelection({ anchor: LINE_TWO[0]!, caret: LINE_TWO[1]! - 1 });
  await host.run(TOGGLE_SCOPE, { window });
  assert.deepEqual(host.editorBars(window).bars.map((bar) => bar.id), ["find"]);
  assert.deepEqual(scope(window), [LINE_TWO[0], LINE_TWO[1]! - 1]);
});
