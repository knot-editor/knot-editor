import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { commands, windows, workbench } from "knot";
import * as host from "knot/testing";
import { start } from "../src/extension.js";
import { column, wordBounds } from "../src/motion.js";

beforeEach(async () => {
  host.reset();
  host.openWindow({ documents: [host.addDocument({ text: "one\nab😀\n" })] });
  await start();
});

const caret = () => host.editorView().caret;
const move = (direction: string) => host.run("example.modal.move", { arguments: { direction } });

test("the modal context binds motions, reveals, marks, paste, Escape, and typed text", () => {
  const modal = { view: "editor", context: "modal" };
  assert.deepEqual(host.bindings(), [
    { key: "h", command: { name: "example.modal.move", arguments: { direction: "left" } }, ...modal },
    { key: "j", command: { name: "example.modal.move", arguments: { direction: "down" } }, ...modal },
    { key: "k", command: { name: "example.modal.move", arguments: { direction: "up" } }, ...modal },
    { key: "l", command: { name: "example.modal.move", arguments: { direction: "right" } }, ...modal },
    { key: "c", command: "example.modal.change", ...modal },
    { key: "z t", command: { name: "example.modal.reveal", arguments: { edge: "top" } }, ...modal },
    { key: "z b", command: { name: "example.modal.reveal", arguments: { edge: "bottom" } }, ...modal },
    { key: "m", command: "example.modal.mark", ...modal },
    { key: "'", command: "example.modal.jump", ...modal },
    { key: "p", command: "example.modal.paste", ...modal },
    { key: "escape", command: "example.modal.leave", ...modal },
    { key: "escape", command: "example.modal.finish", view: "editor", context: "inserting" },
    { key: null, command: "example.modal.ignore", ...modal },
  ]);
});

test("entering and leaving toggle the editor's context", async () => {
  await host.run("example.modal.enter");
  assert.deepEqual(host.keyContexts(), ["modal"]);
  await host.run("example.modal.leave");
  assert.deepEqual(host.keyContexts(), []);
});

test("the caret is a block in modal mode and the native bar otherwise", async () => {
  assert.equal(host.cursorStyle(), "bar");
  await host.run("example.modal.enter");
  assert.equal(host.cursorStyle(), "block");
  await host.run("example.modal.change");
  assert.equal(host.cursorStyle(), "bar", "inserting types with the native caret");
  await host.run("example.modal.finish");
  assert.equal(host.cursorStyle(), "block");
  await host.run("example.modal.leave");
  assert.equal(host.cursorStyle(), "bar");
});

test("reveals put the caret's line at an edge of the window", async () => {
  const text = Array.from({ length: 30 }, (_, line) => `line ${line}\n`).join("");
  host.reset();
  const window = host.openWindow({ documents: [host.addDocument({ text })] });
  host.setViewportLines(10, window);
  await start();
  const editor = await workbench.editor(host.tabTarget(window, 0));
  const line = (n: number) => text.split("\n").slice(0, n).join("\n").length + 1;
  await editor.setSelection({ anchor: line(12), caret: line(12) });
  await host.run("example.modal.reveal", { arguments: { edge: "top" } });
  assert.deepEqual(await editor.visibleLines(), { first: 12, last: 21 });
  await host.run("example.modal.reveal", { arguments: { edge: "bottom" } });
  assert.deepEqual(await editor.visibleLines(), { first: 3, last: 12 });
  await assert.rejects(host.run("example.modal.reveal", { arguments: { edge: "middle" } }));
});

test("jump returns to the marked text after edits move it", async () => {
  const id = host.documentIds()[0]!;
  const window = (await windows.list())[0]!.target;
  const editor = await workbench.editor(host.tabTarget(window, 0));
  await editor.setSelection({ anchor: 4, caret: 4 });
  await host.run("example.modal.mark");
  await editor.setSelection({ anchor: 0, caret: 0 });
  await commands.register("test.prefix", async ({ buffer }) => {
    const { revision } = await buffer!.snapshot();
    await buffer!.applyEdits([{ range: { startByteOffset: 0, endByteOffset: 0 }, text: ">> " }], { ifRevision: revision });
  });
  await host.run("test.prefix");
  assert.equal(host.document(id).text, ">> one\nab😀\n");
  await host.run("example.modal.jump");
  assert.equal(caret(), 7, "the same character, three bytes later");
});

test("paste inserts the clipboard at the caret and moves past it", async () => {
  await workbench.writeClipboardText("é😀");
  const editor = await workbench.editor(host.tabTarget((await windows.list())[0]!.target, 0));
  await editor.setSelection({ anchor: 3, caret: 3 });
  await host.run("example.modal.paste");
  assert.equal(host.document(host.documentIds()[0]!).text, "oneé😀\nab😀\n");
  assert.equal(caret(), 9, "past 2 + 4 pasted bytes");
});

test("motions move by character within a line and keep the column across lines", async () => {
  for (let i = 0; i < 4; i++) await move("right");
  assert.equal(caret(), 3, "stops at the end of the line");
  await move("down");
  assert.equal(caret(), 6, "column 3 would split the emoji, so before it");
  await move("right");
  assert.equal(caret(), 10, "over the emoji");
  await move("left");
  await move("left");
  assert.equal(caret(), 5);
  await move("up");
  await move("up");
  assert.equal(caret(), 1, "stays on the first line");
  await move("down");
  await move("down");
  await move("down");
  assert.equal(caret(), 11, "the empty last line");
  await assert.rejects(host.run("example.modal.move", { arguments: {} }));
});

test("swallowed text leaves the buffer and caret alone", async () => {
  await host.run("example.modal.ignore", { arguments: { text: "x" } });
  assert.equal(caret(), 0);
});

test("columns never split a surrogate pair", () => {
  assert.equal(column("a😀", 1, "right"), 3);
  assert.equal(column("a😀", 3, "left"), 1);
  assert.equal(column("a😀b", 2, "down"), 1);
  assert.equal(column("ab", 5, "up"), 2);
  assert.equal(column("", 0, "left"), 0);
});

test("following puts each activated editor in modal mode until text is selected", async () => {
  await windows.focus((await windows.list())[0]!.target);
  await host.run("example.modal.follow");
  assert.deepEqual(host.keyContexts(), ["modal"], "the active editor joins at once");
  const window = host.openWindow({ documents: [host.addDocument({ text: "second" })] });
  assert.deepEqual(host.keyContexts(window), [], "an editor not yet activated is untouched");
  await host.emit({ type: "activeTargetChanged", activeWindow: window, activeTarget: host.tabTarget(window, 0) });
  assert.deepEqual(host.keyContexts(window), ["modal"]);
  const editor = await workbench.editor(host.tabTarget(window, 0));
  await editor.setSelection({ anchor: 1, caret: 4 });
  assert.deepEqual(host.keyContexts(window), [], "selecting text leaves modal mode");
  await host.run("example.modal.follow");
  assert.deepEqual(host.keyContexts(), [], "toggling off leaves every editor");
});

test("words are the run of word characters at or after the caret", () => {
  assert.deepEqual(wordBounds("foo bar", 1), [0, 3]);
  assert.deepEqual(wordBounds("foo bar", 3), [0, 3], "at a word's end");
  assert.deepEqual(wordBounds("foo  bar", 4), [4, 4], "between words");
  assert.deepEqual(wordBounds("😀 né", 4), [3, 5]);
  assert.deepEqual(wordBounds("", 0), [0, 0]);
});

test("change deletes the word and holds one undo group until Escape", async () => {
  await host.run("example.modal.enter");
  await host.run("example.modal.change");
  assert.equal(host.undoGroupOpen(), true);
  assert.deepEqual(host.keyContexts(), ["inserting"]);
  assert.equal(host.document(host.documentIds()[0]!).text, "\nab😀\n");
  await host.run("example.modal.finish");
  assert.equal(host.undoGroupOpen(), false);
  assert.deepEqual(host.keyContexts(), ["modal"]);
});
