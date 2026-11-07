import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { commands, workbench } from "knot";
import type { CommandContext } from "knot";
import * as host from "knot/testing";

beforeEach(() => host.reset());

async function capture(): Promise<CommandContext> {
  let captured: CommandContext | undefined;
  const registration = await commands.register("test.capture", (context) => {
    captured = context;
  });
  await host.run("test.capture");
  await registration.dispose();
  return captured!;
}

test("line lookups split at newlines and leave terminators out", async () => {
  const document = host.addDocument({ text: "é1\r\nab\n\n😀" });
  host.openWindow({ documents: [document] });
  const buffer = (await capture()).buffer!;
  assert.equal(await buffer.lineCount(), 4);
  assert.deepEqual(await buffer.lineRange(0), { startByteOffset: 0, endByteOffset: 3 });
  assert.deepEqual(await buffer.lineRange(1), { startByteOffset: 5, endByteOffset: 7 });
  assert.deepEqual(await buffer.lineRange(2), { startByteOffset: 8, endByteOffset: 8 });
  assert.deepEqual(await buffer.lineRange(3), { startByteOffset: 9, endByteOffset: 13 });
  await assert.rejects(buffer.lineRange(4), { code: "InvalidRange" });
  assert.equal(await buffer.lineAt(0), 0);
  assert.equal(await buffer.lineAt(5), 1);
  assert.equal(await buffer.lineAt(13), 3);
  await assert.rejects(buffer.lineAt(1), { code: "InvalidRange" });
  const line = await buffer.snapshot(await buffer.lineRange(3));
  assert.equal(line.text, "😀");
  assert.equal(line.byteOffsetAtUtf16(2), 4);
});

test("the editor reads and moves the selection and restores view states", async () => {
  const document = host.addDocument({ text: "one\ntwo\n" });
  host.openWindow({ documents: [document] });
  host.setEditorView({ anchor: 1, caret: 3, top: 4 });
  const editor = (await capture()).editor!;
  assert.deepEqual(await editor.selection(), { anchor: 1, caret: 3 });
  const saved = await editor.viewState();
  await editor.setSelection({ anchor: 5, caret: 5 }, { reveal: "center" });
  assert.deepEqual(host.editorView(), { anchor: 5, caret: 5, top: 4, scrollX: 0 });
  assert.equal(host.editorReveal(), "center");
  await assert.rejects(editor.setSelection({ anchor: 0, caret: 9 }), { code: "InvalidRange" });
  await editor.restoreViewState(saved);
  assert.deepEqual(host.editorView(), saved);
});

test("commands without a document editor get none, and closed editors reject", async () => {
  const window = host.openWindow();
  assert.equal((await capture()).editor, null);
  const document = host.addDocument({ text: "x" });
  host.present(window, document);
  const editor = (await capture()).editor!;
  host.closeWindow(window);
  await assert.rejects(editor.selection(), { code: "InvalidTarget" });
});

test("a handler reports unavailable through commands.unavailable", async () => {
  host.openWindow();
  await commands.register("test.unavailable", () => commands.unavailable());
  assert.equal(await host.run("test.unavailable"), "unavailable");
  assert.deepEqual(await commands.invoke("test.unavailable"), { kind: "unavailable" });
});

test("an editor handle outlives commands and rejects after its tab closes", async () => {
  const document = host.addDocument({ text: "one\ntwo\n" });
  const window = host.openWindow({ documents: [document, document] });
  const [first, second] = [host.tabTarget(window, 0), host.tabTarget(window, 1)];
  const editor = await workbench.editor(first);
  await editor.setKeyContext("normal", true);
  await editor.setSelection({ anchor: 2, caret: 2 });
  assert.deepEqual(await editor.selection(), { anchor: 2, caret: 2 });
  let closed = 0;
  editor.onDidClose(() => {
    closed += 1;
  });
  await host.closeTab(second);
  assert.equal(closed, 0);
  await host.closeTab(first);
  assert.equal(closed, 1);
  await assert.rejects(editor.selection(), { code: "InvalidTarget" });
  await assert.rejects(workbench.editor(first), { code: "InvalidTarget" });
});

test("an editor handle sets and clears its cursor style", async () => {
  const document = host.addDocument({ text: "x" });
  const window = host.openWindow({ documents: [document] });
  const target = host.tabTarget(window, 0);
  const editor = await workbench.editor(target);
  assert.equal(host.cursorStyle(window), "bar");
  await editor.setCursorStyle("block");
  assert.equal(host.cursorStyle(window), "block");
  await editor.setCursorStyle(null);
  assert.equal(host.cursorStyle(window), "bar");
  // @ts-expect-error not a cursor style
  await assert.rejects(editor.setCursorStyle("beam"), TypeError);
  await host.closeTab(target);
  await assert.rejects(editor.setCursorStyle("block"), { code: "InvalidTarget" });
});

test("an editor handle reports the viewport, maps rows, and scrolls without moving the caret", async () => {
  const text = Array.from({ length: 30 }, (_, line) => `line ${line}\n`).join("");
  const document = host.addDocument({ text });
  const window = host.openWindow({ documents: [document] });
  host.setViewportLines(10, window);
  const editor = await workbench.editor(host.tabTarget(window, 0));
  assert.deepEqual(await editor.viewport(), { first: 0, rows: 10 });
  await editor.scrollBy(4);
  assert.deepEqual(await editor.viewport(), { first: 4, rows: 10 });
  assert.deepEqual(await editor.selection(), { anchor: 0, caret: 0 });
  await editor.scrollBy(-100);
  assert.deepEqual(await editor.viewport(), { first: 0, rows: 10 });
  await editor.scrollBy(100);
  assert.deepEqual(await editor.viewport(), { first: 21, rows: 10 });
  await assert.rejects(editor.scrollBy(1.5), TypeError);
  // The fake does not wrap, so a row is a line.
  assert.deepEqual(await editor.rowAt(4), { line: 4, column: 0 });
  assert.deepEqual(await editor.rowAt(1000), { line: 30, column: 0 });
  assert.equal(await editor.rowOf(7, 3), 7);
  await assert.rejects(editor.rowOf(7, 99), { code: "InvalidRange" });
  await assert.rejects(editor.rowAt(-1), TypeError);
});

test("an editor handle folds and unfolds line ranges", async () => {
  const document = host.addDocument({ text: Array.from({ length: 10 }, (_, line) => `line ${line}\n`).join("") });
  const window = host.openWindow({ documents: [document] });
  const editor = await workbench.editor(host.tabTarget(window, 0));
  await editor.fold({ start: 4, end: 6 });
  await editor.fold({ start: 1, end: 8 });
  await editor.fold({ start: 4, end: 6 });
  assert.deepEqual(await editor.folds(), [
    { start: 1, end: 8 },
    { start: 4, end: 6 },
  ]);
  await editor.unfold({ start: 1, end: 8 });
  assert.deepEqual(await editor.folds(), [{ start: 4, end: 6 }]);
  await assert.rejects(editor.fold({ start: 3, end: 99 }), { code: "InvalidRange" });
  await assert.rejects(editor.fold({ start: 3, end: 3 }), TypeError);
});

test("reveal top and bottom put the caret's line at that edge of the viewport", async () => {
  const text = Array.from({ length: 30 }, (_, line) => `line ${line}\n`).join("");
  const document = host.addDocument({ text });
  const window = host.openWindow({ documents: [document] });
  host.setViewportLines(10, window);
  const editor = await workbench.editor(host.tabTarget(window, 0));
  const lineStart = (line: number) => text.split("\n").slice(0, line).join("\n").length + (line > 0 ? 1 : 0);
  const caret = lineStart(7);
  await editor.setSelection({ anchor: caret, caret }, { reveal: "bottom" });
  assert.deepEqual(await editor.viewport(), { first: 0, rows: 10 });
  const far = lineStart(15);
  await editor.setSelection({ anchor: far, caret: far }, { reveal: "bottom" });
  assert.deepEqual(await editor.viewport(), { first: 6, rows: 10 });
  await editor.setSelection({ anchor: far, caret: far }, { reveal: "top" });
  assert.deepEqual(await editor.viewport(), { first: 15, rows: 10 });
  await editor.setSelection({ anchor: far, caret: far }, { reveal: "always-center" });
  assert.deepEqual(await editor.viewport(), { first: 10, rows: 10 });
  const end = text.length;
  await editor.setSelection({ anchor: end, caret: end }, { reveal: "top" });
  assert.deepEqual(await editor.viewport(), { first: 21, rows: 10 });
});

test("marks follow edits and end when their text is deleted or they are disposed", async () => {
  const document = host.addDocument({ text: "hello world" });
  host.openWindow({ documents: [document] });
  const buffer = (await capture()).buffer!;
  const mark = await buffer.createMark(6);
  const edit = async (startByteOffset: number, endByteOffset: number, text: string) => {
    const { revision } = await buffer.snapshot();
    await buffer.applyEdits([{ range: { startByteOffset, endByteOffset }, text }], { ifRevision: revision });
  };
  assert.equal(await mark.position(), 6);
  await edit(0, 0, ">> ");
  assert.equal(await mark.position(), 9);
  await edit(9, 9, "big ");
  assert.equal(await mark.position(), 13);
  await edit(14, 14, "!");
  assert.equal(await mark.position(), 13);
  await edit(0, 3, "");
  assert.equal(await mark.position(), 10);
  await assert.rejects(buffer.createMark(1_000), { code: "InvalidRange" });
  const doomed = await buffer.createMark(5);
  await edit(3, 8, "");
  assert.equal(await doomed.position(), null);
  await mark.dispose();
  assert.equal(await mark.position(), null);
});

test("a disposed handle stops listening and acting", async () => {
  const document = host.addDocument({ text: "x" });
  const window = host.openWindow({ documents: [document] });
  const target = host.tabTarget(window, 0);
  const editor = await workbench.editor(target);
  let closed = 0;
  editor.onDidClose(() => {
    closed += 1;
  });
  await editor.dispose();
  await host.closeTab(target);
  assert.equal(closed, 0);
  await assert.rejects(editor.selection(), { code: "InvalidTarget" });
});

test("a handle reports selection changes until the listener or handle is disposed", async () => {
  const document = host.addDocument({ text: "one\ntwo\n" });
  const window = host.openWindow({ documents: [document] });
  const editor = await workbench.editor(host.tabTarget(window, 0));
  const seen: number[] = [];
  const subscription = await editor.onDidChangeSelection(async () => {
    seen.push((await editor.selection()).caret);
  });
  await editor.setSelection({ anchor: 2, caret: 2 });
  await editor.setSelection({ anchor: 4, caret: 4 });
  assert.deepEqual(seen, [2, 4]);
  await subscription.dispose();
  await editor.setSelection({ anchor: 5, caret: 5 });
  assert.deepEqual(seen, [2, 4]);
  await editor.dispose();
  await assert.rejects(editor.onDidChangeSelection(() => {}), { code: "InvalidTarget" });
});

test("an undo group is open until disposed, and a second one is rejected", async () => {
  const document = host.addDocument({ text: "x" });
  const window = host.openWindow({ documents: [document] });
  const editor = await workbench.editor(host.tabTarget(window, 0));
  const group = await editor.beginUndoGroup();
  assert.equal(host.undoGroupOpen(window), true);
  await assert.rejects(editor.beginUndoGroup(), { code: "OperationInProgress" });
  await group.dispose();
  assert.equal(host.undoGroupOpen(window), false);
});
