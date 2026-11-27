import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { buffers, commands, diff, workbench } from "knot";
import type { CommandContext, FindOptions } from "knot";
import * as host from "knot/testing";

beforeEach(() => host.reset());

async function capture(): Promise<CommandContext> {
  let captured: CommandContext | undefined;
  const registration = await commands.register("test.capture", (context) => {
    captured = context;
  });
  await host.run("test.capture");
  await registration[Symbol.asyncDispose]();
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

test("wordAt returns the word, separator, or space segment in byte offsets", async () => {
  // Bytes: "foo" 0..3, "." 3..4, "bar" 4..7, two spaces 7..9, "é" 9..11, an empty line at 12.
  const document = host.addDocument({ text: "foo.bar  é\n\nx" });
  host.openWindow({ documents: [document] });
  const buffer = (await capture()).buffer!;
  const at = async (offset: number) => {
    const word = await buffer.wordAt(offset);
    return word === null ? null : [word.range.startByteOffset, word.range.endByteOffset, word.kind];
  };
  assert.deepEqual(await at(0), [0, 3, "word"]);
  assert.deepEqual(await at(3), [0, 3, "word"]);
  assert.deepEqual(await at(4), [4, 7, "word"]);
  assert.deepEqual(await at(8), [7, 9, "space"]);
  assert.deepEqual(await at(11), [9, 11, "word"]);
  assert.equal(await at(12), null);
  await assert.rejects(buffer.wordAt(10), { code: "InvalidRange" });
  await assert.rejects(buffer.wordAt(99), { code: "InvalidRange" });
});

test("find matches literally in byte offsets with case, word, range, and limit options", async () => {
  // Bytes: é 0..2, É 3..5, 😀 6..10, é 11..13, "cat" at 19, 26 (inside "concat"), and 30.
  const document = host.addDocument({ text: "é É 😀 é xxxx cat concat cat" });
  host.openWindow({ documents: [document] });
  const buffer = (await capture()).buffer!;
  const ranges = async (query: string, options?: FindOptions) =>
    (await buffer.find(query, options)).matches.map((m) => [m.startByteOffset, m.endByteOffset]);
  const folded = await buffer.find("é");
  assert.equal(folded.revision, 0);
  assert.equal(folded.more, false);
  assert.deepEqual(await ranges("é"), [[0, 2], [3, 5], [11, 13]]);
  assert.deepEqual(await ranges("é", { caseSensitive: true }), [[0, 2], [11, 13]]);
  assert.deepEqual(await ranges("cat", { wholeWord: true }), [[19, 22], [30, 33]]);
  assert.deepEqual(await ranges("é", { range: { startByteOffset: 3, endByteOffset: 13 } }), [[3, 5], [11, 13]]);
  assert.deepEqual(await ranges(""), []);
  const limited = await buffer.find("é", { limit: 2 });
  assert.equal(limited.matches.length, 2);
  assert.equal(limited.more, true);
  await assert.rejects(buffer.find("é", { range: { startByteOffset: 1, endByteOffset: 5 } }), { code: "InvalidRange" });
  assert.deepEqual(await ranges("c(a)t", { regex: true, wholeWord: true }), [[19, 22], [30, 33]]);
  assert.deepEqual(await ranges("^", { regex: true, range: { startByteOffset: 0, endByteOffset: 0 } }), [[0, 0]]);
  await assert.rejects(buffer.find("(", { regex: true }), { code: "InvalidQuery" });
});

test("replace commits matches in one batch with expansion, limit, and revision checks", async () => {
  const document = host.addDocument({ text: "foo Bar foo\nbaz" });
  host.openWindow({ documents: [document] });
  const buffer = (await capture()).buffer!;
  const text = async () => (await buffer.snapshot()).text;
  let changes = 0;
  await buffer.onDidChange(() => {
    changes += 1;
  });
  const none = await buffer.replace("zzz", "y");
  assert.deepEqual(none, { revision: 0, count: 0, more: false });
  await assert.rejects(buffer.replace("foo", "x", { ifRevision: 4 }), { code: "RevisionConflict" });
  await assert.rejects(buffer.replace("(", "x", { regex: true }), { code: "InvalidQuery" });
  assert.deepEqual(await buffer.replace("foo", "$1", { limit: 1 }), { revision: 1, count: 1, more: true });
  assert.equal(await text(), "$1 Bar foo\nbaz");
  const expanded = await buffer.replace("(?<word>\\w+) (\\w+)", "${word}_$2\\n$$", { regex: true });
  assert.equal(expanded.count, 1);
  assert.equal(await text(), "$1_Bar\n$ foo\nbaz");
  const lines = await buffer.replace("^", "// ", { regex: true });
  assert.equal(lines.count, 3);
  assert.equal(await text(), "// $1_Bar\n// $ foo\n// baz");
  assert.equal(changes, 3);
});

test("find folds case one character at a time", async () => {
  const document = host.addDocument({ text: "Σ σ ς ß ss SS İ ı i" });
  host.openWindow({ documents: [document] });
  const buffer = (await capture()).buffer!;
  const count = async (query: string) => (await buffer.find(query)).matches.length;
  assert.equal(await count("σ"), 3);
  assert.equal(await count("ß"), 1);
  assert.equal(await count("ss"), 2);
  assert.equal(await count("İ"), 1);
  assert.equal(await count("ı"), 1);
  assert.equal(await count("i"), 1);
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
  const editor = await workbench.editor(host.viewTarget(window, 0));
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
  await assert.rejects(workbench.editor(first as never), { code: "InvalidTarget" });
});

test("an editor handle sets and clears its cursor style", async () => {
  const document = host.addDocument({ text: "x" });
  const window = host.openWindow({ documents: [document] });
  const target = host.tabTarget(window, 0);
  const editor = await workbench.editor(host.viewTarget(window, 0));
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

test("an editor handle shows lines of a source buffer as block rows", async () => {
  const window = host.openWindow({ documents: [host.addDocument({ text: "a\nb" })] });
  const editor = await workbench.editor(host.viewTarget(window, 0));
  const source = await buffers.create({ text: "one\ntwo\nthree" });
  await editor.setBlocks([{ line: 1, source, lines: { start: 1, end: 5 }, color: "diff.removed.background" }]);
  assert.deepEqual(host.blockRows(window), [
    { line: 1, lines: ["two", "three"], color: "diff.removed.background" },
  ]);
  await source.applyEdits([{ range: { startByteOffset: 4, endByteOffset: 7 }, text: "2" }], { ifRevision: 0 });
  assert.deepEqual(host.blockRows(window)[0]!.lines, ["2", "three"]);
  await assert.rejects(editor.setBlocks([{ line: 0, source: {} as never, lines: { start: 0, end: 1 } }]), TypeError);
  await assert.rejects(editor.setBlocks([{ line: 0, source, lines: { start: 2, end: 1 } }]), RangeError);
  await editor.setBlocks([]);
  assert.deepEqual(host.blockRows(window), []);
});

test("an editor handle opens bars, focuses fields, and hears typing", async () => {
  const document = host.addDocument({ text: "x" });
  const window = host.openWindow({ documents: [document] });
  const editor = await workbench.editor(host.viewTarget(window, 0));
  const edits: unknown[] = [];
  await editor.onDidEditBar((edit) => {
    edits.push(edit);
  });
  await editor.openBar("find", { fields: [{ id: "query", placeholder: "Find" }] });
  await editor.openBar("other", { message: "below" });
  await editor.focusBarField("find", "query", { text: "seed" });
  assert.deepEqual(host.editorBars(window).focus, { bar: "find", field: "query" });
  assert.deepEqual(edits, []);
  await host.typeInBar("find", "query", "typed");
  assert.deepEqual(edits, [{ bar: "find", fields: { query: "typed" } }]);
  await editor.openBar("find", { fields: [{ id: "query" }], message: "1 of 2" });
  assert.deepEqual(await editor.barFields("find"), { query: "typed" });
  assert.deepEqual(host.editorBars(window).bars.map((bar) => [bar.id, bar.message]), [["find", "1 of 2"], ["other", "below"]]);
  await assert.rejects(editor.openBar("x", { fields: [{ id: "a" }, { id: "a" }] }), RangeError);
  await assert.rejects(editor.focusBarField("other", "query"), { code: "BarNotFound" });

  // Clicking a control runs its command in the bar's editor.
  const pressed: unknown[] = [];
  await commands.register("bar.press", async ({ editor: captured, arguments: args }) => {
    pressed.push([args, captured !== null]);
  });
  const icon = "case-sensitive";
  const command = { name: "bar.press", arguments: "case" };
  await editor.openBar("controls", { toggles: [{ id: "case", icon, command, active: true }], buttons: [{ id: "next", icon, command }] });
  assert.deepEqual(host.editorBars(window).bars[2]!.toggles.map((toggle) => toggle.active), [true]);
  await host.clickBarControl("controls", "case");
  await host.clickBarControl("controls", "next");
  assert.deepEqual(pressed, [["case", true], ["case", true]]);
  await assert.rejects(editor.openBar("x", { toggles: [{ id: "a", icon, command }], buttons: [{ id: "a", icon, command }] }), RangeError);
  await editor.closeBar("controls");
  await editor.closeBar("find");
  assert.equal(host.editorBars(window).focus, null);
  await assert.rejects(editor.barFields("find"), { code: "BarNotFound" });
});

test("an editor handle reports the viewport, maps rows, and scrolls without moving the caret", async () => {
  const text = Array.from({ length: 30 }, (_, line) => `line ${line}\n`).join("");
  const document = host.addDocument({ text });
  const window = host.openWindow({ documents: [document] });
  host.setViewportLines(10, window);
  const editor = await workbench.editor(host.viewTarget(window, 0));
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
  const editor = await workbench.editor(host.viewTarget(window, 0));
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
  const editor = await workbench.editor(host.viewTarget(window, 0));
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
  await mark[Symbol.asyncDispose]();
  assert.equal(await mark.position(), null);
});

test("a disposed handle stops listening and acting", async () => {
  const document = host.addDocument({ text: "x" });
  const window = host.openWindow({ documents: [document] });
  const target = host.tabTarget(window, 0);
  const editor = await workbench.editor(host.viewTarget(window, 0));
  let closed = 0;
  editor.onDidClose(() => {
    closed += 1;
  });
  await editor[Symbol.asyncDispose]();
  await host.closeTab(target);
  assert.equal(closed, 0);
  await assert.rejects(editor.selection(), { code: "InvalidTarget" });
});

test("a handle reports selection changes until the listener or handle is disposed", async () => {
  const document = host.addDocument({ text: "one\ntwo\n" });
  const window = host.openWindow({ documents: [document] });
  const editor = await workbench.editor(host.viewTarget(window, 0));
  const seen: number[] = [];
  const subscription = await editor.onDidChangeSelection(async ({ caret }) => {
    seen.push(caret);
  });
  await editor.setSelection({ anchor: 2, caret: 2 });
  await editor.setSelection({ anchor: 4, caret: 4 });
  assert.deepEqual(seen, [2, 4]);
  await subscription[Symbol.asyncDispose]();
  await editor.setSelection({ anchor: 5, caret: 5 });
  assert.deepEqual(seen, [2, 4]);
  await editor[Symbol.asyncDispose]();
  await assert.rejects(editor.onDidChangeSelection(() => {}), { code: "InvalidTarget" });
});

test("a selection read and a notification carry the caret's line and its neighbors when asked", async () => {
  const document = host.addDocument({ text: "one\r\ntwo\nthree" });
  const window = host.openWindow({ documents: [document] });
  const editor = await workbench.editor(host.viewTarget(window, 0));
  const shown = (items: readonly { line: number; startByteOffset: number; text: string }[]) =>
    items.map((item) => `${item.line}@${item.startByteOffset}:${item.text}`);
  const heard: string[][] = [];
  await editor.onDidChangeSelection(({ lines }) => void heard.push(shown(lines)), { lines: 0 });
  await editor.onDidChangeSelection(() => {}, { lines: 1 });
  await editor.setSelection({ anchor: 0, caret: 0 });
  // The handle serves the widest window, so the narrow listener gets it too.
  assert.deepEqual(heard, [["0@0:one", "1@5:two"]]);
  assert.deepEqual(await editor.selection(), { anchor: 0, caret: 0 });
  const read = await editor.selection({ lines: 1 });
  assert.equal(read.line, 0);
  assert.deepEqual(shown(read.lines), ["0@0:one", "1@5:two"]);
  const buffer = (await capture()).buffer!;
  assert.deepEqual(shown(await buffer.lines(1, 2)), ["1@5:two", "2@9:three"]);
  await assert.rejects(buffer.lines(2, 1), { code: "InvalidRange" });
  await assert.rejects(buffer.lines(0, 3), { code: "InvalidRange" });
});

test("an undo group is open until disposed, and a second one is rejected", async () => {
  const document = host.addDocument({ text: "x" });
  const window = host.openWindow({ documents: [document] });
  const editor = await workbench.editor(host.viewTarget(window, 0));
  const group = await editor.beginUndoGroup();
  assert.equal(host.undoGroupOpen(window), true);
  await assert.rejects(editor.beginUndoGroup(), { code: "OperationInProgress" });
  await group[Symbol.asyncDispose]();
  assert.equal(host.undoGroupOpen(window), false);
});

test("diff compares strings and buffers by line and reports buffer revisions", async () => {
  const document = host.addDocument({ text: "a\nb\nc\n" });
  host.openWindow({ documents: [document] });
  const buffer = (await capture()).buffer!;
  assert.deepEqual(await diff("a\nc\n", buffer), {
    hunks: [{ old: { start: 1, end: 1 }, new: { start: 1, end: 2 } }],
    newRevision: 0,
  });
  assert.deepEqual(await diff(buffer, "a\nB\nc\n"), {
    hunks: [{ old: { start: 1, end: 2 }, new: { start: 1, end: 2 } }],
    oldRevision: 0,
  });
  assert.deepEqual(await diff("", ""), { hunks: [] });
  assert.deepEqual((await diff(buffer, buffer)).hunks, []);
});

test("diff ignores whitespace only when asked and rejects invalid arguments", async () => {
  assert.equal((await diff("a b\r\n", "ab\n")).hunks.length, 1);
  assert.deepEqual(await diff("a b\r\n", "ab\n", { ignoreWhitespace: true }), { hunks: [] });
  await assert.rejects(diff(1 as never, ""), TypeError);
  await assert.rejects(diff("", {} as never), TypeError);
  await assert.rejects(diff("", "", { ignoreWhitespace: "yes" as never }), TypeError);
});

test("clicking a contribution runs its command with its range", async () => {
  const document = host.addDocument({ text: "abcdef" });
  host.openWindow({ documents: [document] });
  const seen: unknown[] = [];
  await commands.register("mark.click", async ({ editor, buffer, arguments: args }) => {
    seen.push([args, editor !== null, buffer !== null]);
  });
  const { revision } = await (await capture()).buffer!.snapshot();
  await (await capture()).buffer!.contributions.replace(
    [
      { range: { startByteOffset: 1, endByteOffset: 3 }, gutter: { color: "diff.added", style: "bar" }, command: "mark.click" },
      { range: { startByteOffset: 4, endByteOffset: 4 }, gutter: { color: "diff.removed", style: "deletion" }, command: { name: "mark.click", arguments: { hunk: 2 } } },
      { range: { startByteOffset: 0, endByteOffset: 1 } },
    ],
    { ifRevision: revision },
  );
  await host.clickContribution(document, 0);
  await host.clickContribution(document, 1);
  assert.deepEqual(seen, [
    [{ range: { startByteOffset: 1, endByteOffset: 3 } }, true, true],
    [{ hunk: 2, range: { startByteOffset: 4, endByteOffset: 4 } }, true, true],
  ]);
  await assert.rejects(host.clickContribution(document, 2), /no command/);
});

test("an editor handle opens popovers and hears why Knot closed them", async () => {
  const document = host.addDocument({ text: "a\nb\nc" });
  const window = host.openWindow({ documents: [document] });
  const editor = await workbench.editor(host.viewTarget(window, 0));
  const closes: unknown[] = [];
  await editor.onDidClosePopover((close) => {
    closes.push(close);
  });
  const popover = { anchor: { start: 0, end: 1 }, focus: true, layout: { type: "column", children: [] } } as never;
  await editor.openPopover("hunk", popover);
  await editor.openPopover("hover", popover);
  await editor.closePopover("hover");
  assert.deepEqual(closes, []);
  await host.dismissPopover("hunk", "escape");
  await host.dismissPopover("hunk", "escape");
  assert.deepEqual(closes, [{ popover: "hunk", reason: "escape" }]);
});

test("an editor handle opens zones and hears when the user dismissed them", async () => {
  const document = host.addDocument({ text: "a\nb\nc" });
  const window = host.openWindow({ documents: [document] });
  const editor = await workbench.editor(host.viewTarget(window, 0));
  const closes: unknown[] = [];
  await editor.onDidCloseZone((close) => {
    closes.push(close);
  });
  const zone = { line: 1, focus: true, layout: { type: "column", children: [] } } as never;
  await editor.openZone("hunk", zone);
  await editor.openZone("end", { ...(zone as object), line: 3 } as never);
  await editor.closeZone("end");
  assert.deepEqual(closes, []);
  await host.dismissZone("hunk");
  await host.dismissZone("hunk");
  assert.deepEqual(closes, [{ zone: "hunk" }]);
  await assert.rejects(editor.openZone("", zone), TypeError);
  await assert.rejects(editor.openZone("x", { ...(zone as object), line: -1 } as never), TypeError);
});
