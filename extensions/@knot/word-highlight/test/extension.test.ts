import assert from "node:assert/strict";
import { afterEach, beforeEach, mock, test } from "node:test";
import { settings, windows, workbench } from "knot";
import * as host from "knot/testing";
import { SELECTION_COLOR, WORD_COLOR } from "../src/highlight.js";
import { start } from "../src/extension.js";
import { DELAY, SELECTION_ENABLED, WORD_ENABLED } from "../src/settings.js";

// Package tests get Node's own timers, which mock.timers drives. Its mocked
// timeouts ignore refresh(), so debouncing is covered by the product tests.
beforeEach(() => {
  host.reset();
  mock.timers.enable({ apis: ["setTimeout"] });
});
afterEach(() => mock.timers.reset());

const TEXT = "foo bar foo foobar\nfoo  x Foo\n";

const settle = async () => {
  for (let turn = 0; turn < 4; turn += 1) await new Promise((resolve) => setImmediate(resolve));
};

async function setup() {
  const document = host.addDocument({ text: TEXT });
  const window = host.openWindow({ documents: [document] });
  await start();
  const editor = await workbench.editor(host.tabTarget(window, 0));
  return { window, editor };
}

/** Highlights as [start, color] pairs of byte offsets. */
function painted(window: ReturnType<typeof host.openWindow>) {
  return host
    .viewDecorations(window)
    .map((item) => [item.range.startByteOffset, item.decoration?.color]);
}

async function rest(milliseconds = 75) {
  mock.timers.tick(milliseconds);
  await settle();
}

test("the word at the caret is highlighted everywhere as a whole word after the delay", async () => {
  const { window } = await setup();
  await rest(74);
  assert.deepEqual(painted(window), []);
  await rest(1);
  assert.deepEqual(painted(window), [[0, WORD_COLOR], [8, WORD_COLOR], [19, WORD_COLOR]]);
});

test("a selection highlights its other occurrences, case-insensitively and not as whole words", async () => {
  const { window, editor } = await setup();
  await editor.setSelection({ anchor: 0, caret: 3 });
  await rest();
  assert.deepEqual(painted(window), [
    [8, SELECTION_COLOR],
    [12, SELECTION_COLOR],
    [19, SELECTION_COLOR],
    [26, SELECTION_COLOR],
  ]);
});

test("a caret off any word, a multi-line selection, and a whitespace selection highlight nothing", async () => {
  const { window, editor } = await setup();
  await rest();
  assert.equal(painted(window).length, 3);
  await editor.setSelection({ anchor: 3, caret: 3 });
  await rest();
  assert.equal(painted(window).length, 3, "the end of a word still counts as the word");
  await editor.setSelection({ anchor: 23, caret: 23 });
  await rest();
  assert.deepEqual(painted(window), [], "inside a run of spaces");
  await editor.setSelection({ anchor: 0, caret: 3 });
  await rest();
  assert.equal(painted(window).length, 4);
  await editor.setSelection({ anchor: 0, caret: 20 });
  await rest();
  assert.deepEqual(painted(window), [], "over two lines");
  await editor.setSelection({ anchor: 22, caret: 24 });
  await rest();
  assert.deepEqual(painted(window), [], "whitespace only");
});

test("an edit refreshes the highlights", async () => {
  const { window } = await setup();
  await rest();
  const tab = (await windows.get(window))!.panes[0]!.tabs[0]!;
  const buffer = tab.buffer!;
  const { revision } = await buffer.snapshot({ startByteOffset: 0, endByteOffset: 0 });
  await buffer.applyEdits([{ range: { startByteOffset: 8, endByteOffset: 11 }, text: "baz" }], { ifRevision: revision });
  await rest();
  assert.deepEqual(painted(window), [[0, WORD_COLOR], [19, WORD_COLOR]]);
});

test("each kind can be turned off", async () => {
  const { window, editor } = await setup();
  await settings.set(WORD_ENABLED, false);
  await rest();
  assert.deepEqual(painted(window), []);
  await editor.setSelection({ anchor: 0, caret: 3 });
  await rest();
  assert.equal(painted(window).length, 4);
  await settings.set(SELECTION_ENABLED, false);
  await rest();
  assert.deepEqual(painted(window), []);
});

test("the delay is a setting", async () => {
  await settings.set(DELAY, 200);
  const { window } = await setup();
  await rest(199);
  assert.deepEqual(painted(window), []);
  await rest(1);
  assert.equal(painted(window).length, 3);
});
