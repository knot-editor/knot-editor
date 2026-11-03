import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import * as host from "knot/testing";
import { COMMAND, start } from "../src/extension.js";

const TEXT = "first\n\tsecond é😀 line\nthird\n";
const ORIGINAL = { anchor: 2, caret: 4, top: 6, scrollX: 3 };
const offset = (text: string) => new TextEncoder().encode(text).length;
const LINE_2 = offset("first\n");
const LINE_3 = offset("first\n\tsecond é😀 line\n");

beforeEach(async () => {
  host.reset();
  host.openWindow({ documents: [host.addDocument({ text: TEXT })] });
  host.setEditorView(ORIGINAL);
  await start();
});

function prompt() {
  const [shown] = host.prompts();
  return shown?.options as { title: string; value?: string };
}

test("typing previews each location, centered, and Enter keeps the last", async () => {
  host.type("2", "2:9", "2,10");
  host.answer("showInput", { kind: "accepted", value: "2,10" });
  await host.run(COMMAND);

  assert.equal(prompt().title, "Go to Line");
  assert.deepEqual(host.inputMessages(), [
    "Current Line: 1 of 4 (column 5)",
    "Go to line 2",
    "Go to line 2, character 9",
    "Go to line 2, character 10",
  ]);
  const caret = LINE_2 + offset("\tsecond é");
  assert.deepEqual(host.editorView(), { ...ORIGINAL, anchor: caret, caret });
  assert.equal(host.editorReveal(), "center");
});

test("lines and columns past the end clamp", async () => {
  host.type("3:99", "99:2");
  host.answer("showInput", { kind: "accepted", value: "99:2" });
  await host.run(COMMAND);

  assert.deepEqual(host.inputMessages().slice(1), ["Go to line 3, character 6", "Go to line 4, character 1"]);
  const end = offset(TEXT);
  assert.deepEqual(host.editorView(), { ...ORIGINAL, anchor: end, caret: end });
});

test("dismissing restores the view exactly", async () => {
  host.type("3");
  host.answer("showInput", { kind: "dismissed" });
  await host.run(COMMAND);

  assert.deepEqual(host.inputMessages().at(-1), "Go to line 3");
  assert.deepEqual(host.editorView(), ORIGINAL);
});

test("invalid input restores the view, shows the usage, and Enter only dismisses", async () => {
  host.type("3", "3x");
  host.answer("showInput", { kind: "accepted", value: "3x" });
  await host.run(COMMAND);

  assert.deepEqual(host.inputMessages().slice(1), [
    "Go to line 3",
    "Current Line: 1 of 4 (column 5)",
  ]);
  assert.deepEqual(host.editorView(), ORIGINAL);
});

test("Enter on a location newer than the last preview still goes there", async () => {
  host.answer("showInput", { kind: "accepted", value: "3" });
  await host.run(COMMAND);

  assert.deepEqual(host.editorView(), { ...ORIGINAL, anchor: LINE_3, caret: LINE_3 });
});

test("the current position counts columns in scalar values", async () => {
  const caret = LINE_2 + offset("\tsecond é😀");
  host.setEditorView({ anchor: caret, caret });
  host.answer("showInput", { kind: "dismissed" });
  await host.run(COMMAND);

  assert.equal(host.inputMessages()[0], "Current Line: 2 of 4 (column 11)");
});

test("a command without an editor is unavailable", async () => {
  const window = host.openWindow();
  assert.equal(await host.run(COMMAND, { window }), "unavailable");
  assert.equal(host.prompts().length, 0);
});
