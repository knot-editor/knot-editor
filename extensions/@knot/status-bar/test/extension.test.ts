import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { commands, documents, languages, settings, windows, workbench } from "knot";
import * as host from "knot/testing";
import { INDENTATION_COMMAND, start } from "../src/extension.js";

const TEXT = "one\n\tsecond é\nthree\n";
const offset = (text: string) => new TextEncoder().encode(text).length;

beforeEach(async () => {
  host.reset();
  await languages.register({
    id: "rust", name: "Rust", grammar: "rust", queries: { highlights: "" }, fileExtensions: ["rs"],
  });
});

function texts(window: ReturnType<typeof host.openWindow>) {
  return host.statusItems(window).map((item) => item.text);
}

async function changed(window: ReturnType<typeof host.openWindow>) {
  const info = await windows.get(window);
  await host.emit({ type: "presentationsChanged", window: info! });
}

async function editorOf(window: ReturnType<typeof host.openWindow>) {
  return workbench.editor(host.tabTarget(window, 0));
}

test("a window shows its editor's position, indentation, and language, right to left by priority", async () => {
  const document = host.addDocument({ text: TEXT, language: "rust" });
  const window = host.openWindow({ documents: [document] });
  await start();
  assert.deepEqual(texts(window), ["Ln 1, Col 1", "Spaces: 4", "Rust"]);
  assert.deepEqual(
    host.statusItems(window).map((item) => [item.id, item.side, item.command?.name]),
    [
      ["position", "right", "editor.go-to-line"],
      ["indentation", "right", INDENTATION_COMMAND],
      ["language", "right", "editor.set-language-mode"],
    ],
  );
});

test("the position follows the selection, counting characters and a tab as one", async () => {
  const document = host.addDocument({ text: TEXT });
  const window = host.openWindow({ documents: [document] });
  await start();
  const editor = await editorOf(window);
  const caret = offset("one\n\tsecond é");
  await editor.setSelection({ anchor: caret, caret });
  assert.equal(texts(window)[0], "Ln 2, Col 10");
  await editor.setSelection({ anchor: 0, caret: offset("one\n\t") });
  assert.equal(texts(window)[0], "Ln 2, Col 2 (5 selected)");
  await editor.setSelection({ anchor: offset("one\n\tsecond é"), caret: 0 });
  assert.equal(texts(window)[0], "Ln 1, Col 1 (13 selected)");
});

test("the language shows plain text and falls back to an unregistered ID", async () => {
  const plain = host.addDocument({ text: "" });
  const first = host.openWindow({ documents: [plain] });
  const unknown = host.addDocument({ text: "", language: "zig" });
  const second = host.openWindow({ documents: [unknown] });
  await start();
  assert.equal(texts(first)[2], "Plain Text");
  assert.equal(texts(second)[2], "zig");
});

test("a document change updates the indentation and language of every window showing it", async () => {
  const document = host.addDocument({ text: TEXT, language: "rust" });
  const window = host.openWindow({ documents: [document] });
  await start();
  await settings.set("editor.insertSpaces", false, { document });
  await settings.set("editor.tabSize", 8, { document });
  await documents.setLanguage(document, "plaintext");
  await host.emit({ type: "documentChanged", document: (await documents.get(document))! });
  assert.deepEqual(texts(window).slice(1), ["Tab Size: 8", "Plain Text"]);
});

test("windows follow their own editor and show nothing without one", async () => {
  const first = host.addDocument({ text: "a\n" });
  const second = host.addDocument({ text: "b\nc\n" });
  const one = host.openWindow({ documents: [first] });
  const two = host.openWindow();
  await start();
  assert.deepEqual(texts(two), []);

  host.present(two, second);
  await changed(two);
  const editor = await editorOf(two);
  await editor.setSelection({ anchor: offset("b\n"), caret: offset("b\n") });
  assert.equal(texts(two)[0], "Ln 2, Col 1");
  assert.equal(texts(one)[0], "Ln 1, Col 1");

  await host.closeTab(host.tabTarget(two, 0));
  await changed(two);
  assert.deepEqual(texts(two), []);
  assert.equal(texts(one).length, 3);
});

test("a window opened later and a closed window are followed and released", async () => {
  await start();
  const document = host.addDocument({ text: "a\n" });
  const window = host.openWindow({ documents: [document] });
  await host.emit({ type: "windowOpened", window: (await windows.get(window))! });
  assert.deepEqual(texts(window), ["Ln 1, Col 1", "Spaces: 4", "Plain Text"]);
  const editor = await editorOf(window);
  host.closeWindow(window);
  await host.emit({ type: "windowClosed", window });
  await assert.rejects(editor.selection(), { code: "InvalidTarget" });
});

test("Change Indentation writes document-scoped settings", async () => {
  const document = host.addDocument({ text: TEXT });
  const other = host.addDocument({ text: TEXT });
  host.openWindow({ documents: [document] });
  host.openWindow({ documents: [other] });
  await start();

  host.answer("showPicker", { kind: "selected", id: "tabs" });
  assert.equal(await host.run(INDENTATION_COMMAND), "completed");
  assert.equal(await settings.get("editor.insertSpaces", { document }), false);
  assert.equal(await settings.get("editor.insertSpaces", { document: other }), true);

  host.answer("showPicker", { kind: "selected", id: "size" });
  host.type("0", "99", "x", "8");
  host.answer("showInput", { kind: "accepted", value: "8" });
  await host.run(INDENTATION_COMMAND);
  const hint = "Enter a number from 1 to 32";
  assert.deepEqual(host.inputMessages(), [hint, hint, hint, hint, null]);
  assert.equal(await settings.get("editor.tabSize", { document }), 8);
  assert.equal(await settings.get("editor.tabSize", { document: other }), 4);

  host.answer("showPicker", { kind: "selected", id: "size" });
  host.answer("showInput", { kind: "accepted", value: "0" });
  await host.run(INDENTATION_COMMAND);
  assert.equal(await settings.get("editor.tabSize", { document }), 8);

  host.answer("showPicker", { kind: "dismissed" });
  await host.run(INDENTATION_COMMAND);
  assert.equal(await settings.get("editor.insertSpaces", { document }), false);
});

test("Change Indentation converts through the native commands and needs an editor", async () => {
  const document = host.addDocument({ text: TEXT });
  const window = host.openWindow({ documents: [document] });
  await start();
  const converted: string[] = [];
  for (const name of ["editor.convert-indentation-to-spaces", "editor.convert-indentation-to-tabs"]) {
    await commands.register(name, () => {
      converted.push(name);
    });
  }
  host.answer("showPicker", { kind: "selected", id: "to-spaces" });
  await host.run(INDENTATION_COMMAND);
  host.answer("showPicker", { kind: "selected", id: "to-tabs" });
  await host.run(INDENTATION_COMMAND);
  assert.deepEqual(converted, [
    "editor.convert-indentation-to-spaces",
    "editor.convert-indentation-to-tabs",
  ]);

  const empty = host.openWindow();
  assert.equal(await host.run(INDENTATION_COMMAND, { window: empty }), "unavailable");
  void window;
});
