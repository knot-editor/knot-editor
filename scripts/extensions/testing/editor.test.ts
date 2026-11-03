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
