// A tiny modal editor built on key contexts. Enter Modal Mode puts the editor
// into the `modal` context, where h/j/k/l move the caret, other typed text is
// swallowed, and Escape leaves. Everything else, including typing outside the
// context, stays native. `c` changes the word at the caret: it deletes the
// word and enters the `inserting` context, where typing is native, and Escape
// returns to modal mode. The deletion and everything typed meanwhile are one
// undo step. The caret is a block in modal mode. `zt` and `zb` put the caret's
// line at the top or bottom of the window, `m` marks the caret's place and `'`
// returns to it however the text changed, and `p` pastes the clipboard.
import { commands, keymaps, workbench } from "knot";
import type { Buffer, Disposable, Editor, JsonValue, Mark } from "knot";
import { follow } from "./follow.js";
import { setMode } from "./mode.js";
import { column, type Direction, utf8Length, wordBounds } from "./motion.js";

const modal = { view: "editor", context: "modal" } as const;
const inserting = { view: "editor", context: "inserting" } as const;

export async function start() {
  await commands.register("example.modal.enter", async ({ editor }) => {
    if (editor !== null) await setMode(editor, "modal");
  }, { title: "Example: Enter Modal Mode" });
  let following: Disposable | null = null;
  await commands.register("example.modal.follow", async () => {
    if (following === null) {
      following = await follow();
    } else {
      const stopped = following;
      following = null;
      await stopped.dispose();
    }
  }, { title: "Example: Toggle Modal Mode in Every Editor" });
  await commands.register("example.modal.leave", async ({ editor }) => {
    if (editor !== null) await setMode(editor, null);
  }, { palette: false });
  await commands.register("example.modal.move", async ({ editor, buffer, arguments: args }) => {
    const direction = directionArgument(args);
    if (direction === null) return commands.invalidArguments("expected { direction }");
    if (editor !== null && buffer !== null) await move(editor, buffer, direction);
  }, { palette: false });
  // Typed text arrives here as `{ text }` and is dropped, so modal keys that
  // nothing binds never edit the buffer.
  await commands.register("example.modal.ignore", () => {}, { palette: false });

  let group: Disposable | null = null;
  await commands.register("example.modal.change", async ({ editor, buffer }) => {
    if (editor === null || buffer === null || group !== null) return;
    group = await editor.beginUndoGroup();
    await changeWord(editor, buffer);
    await setMode(editor, "inserting");
  }, { palette: false });
  await commands.register("example.modal.finish", async ({ editor }) => {
    const ended = group;
    group = null;
    await ended?.dispose();
    if (editor !== null) await setMode(editor, "modal");
  }, { palette: false });

  await commands.register("example.modal.reveal", async ({ editor, arguments: args }) => {
    const edge = edgeArgument(args);
    if (edge === null) return commands.invalidArguments("expected { edge }");
    if (editor === null) return;
    await editor.setSelection(await editor.selection(), { reveal: edge });
  }, { palette: false });

  // One mark, in the buffer it was set in: a mark follows edits, so jumping
  // back lands on the same text.
  let marked: { buffer: Buffer; mark: Mark } | null = null;
  await commands.register("example.modal.mark", async ({ editor, buffer }) => {
    if (editor === null || buffer === null) return;
    const previous = marked;
    const { caret } = await editor.selection();
    marked = { buffer, mark: await buffer.createMark(caret) };
    await previous?.mark.dispose();
  }, { palette: false });
  await commands.register("example.modal.jump", async ({ editor, buffer }) => {
    if (editor === null || marked === null || marked.buffer !== buffer) return;
    const offset = await marked.mark.position();
    if (offset !== null) await editor.setSelection({ anchor: offset, caret: offset }, { reveal: "center" });
  }, { palette: false });

  await commands.register("example.modal.paste", async ({ editor, buffer }) => {
    const text = await workbench.readClipboardText();
    if (editor === null || buffer === null || text === null || text === "") return;
    const { caret } = await editor.selection();
    const { revision } = await buffer.snapshot({ startByteOffset: caret, endByteOffset: caret });
    await buffer.applyEdits([{ range: { startByteOffset: caret, endByteOffset: caret }, text }], {
      ifRevision: revision,
    });
    const end = caret + utf8Length(text);
    await editor.setSelection({ anchor: end, caret: end });
  }, { palette: false });

  const keys: [string, Direction][] = [["h", "left"], ["j", "down"], ["k", "up"], ["l", "right"]];
  for (const [key, direction] of keys) {
    keymaps.bind(key, { name: "example.modal.move", arguments: { direction } }, modal);
  }
  keymaps.bind("c", "example.modal.change", modal);
  keymaps.bind("z t", { name: "example.modal.reveal", arguments: { edge: "top" } }, modal);
  keymaps.bind("z b", { name: "example.modal.reveal", arguments: { edge: "bottom" } }, modal);
  keymaps.bind("m", "example.modal.mark", modal);
  keymaps.bind("'", "example.modal.jump", modal);
  keymaps.bind("p", "example.modal.paste", modal);
  keymaps.bind("escape", "example.modal.leave", modal);
  keymaps.bind("escape", "example.modal.finish", inserting);
  keymaps.bindText("example.modal.ignore", modal);
}

async function move(editor: Editor, buffer: Buffer, direction: Direction) {
  const { caret } = await editor.selection();
  const line = await buffer.lineAt(caret);
  const current = await buffer.snapshot(await buffer.lineRange(line));
  const from = current.utf16OffsetAtByte(caret - current.range.startByteOffset);
  const target = direction === "up" ? line - 1 : direction === "down" ? line + 1 : line;
  if (target < 0 || target >= (await buffer.lineCount())) return;
  const next = target === line ? current : await buffer.snapshot(await buffer.lineRange(target));
  const offset = next.range.startByteOffset + next.byteOffsetAtUtf16(column(next.text, from, direction));
  await editor.setSelection({ anchor: offset, caret: offset });
}

async function changeWord(editor: Editor, buffer: Buffer) {
  const { caret } = await editor.selection();
  const range = await buffer.lineRange(await buffer.lineAt(caret));
  const line = await buffer.snapshot(range);
  const [start, end] = wordBounds(line.text, line.utf16OffsetAtByte(caret - range.startByteOffset));
  if (start === end) return;
  const from = range.startByteOffset + line.byteOffsetAtUtf16(start);
  const to = range.startByteOffset + line.byteOffsetAtUtf16(end);
  await buffer.applyEdits([{ range: { startByteOffset: from, endByteOffset: to }, text: "" }], {
    ifRevision: line.revision,
  });
  await editor.setSelection({ anchor: from, caret: from });
}

function edgeArgument(value: JsonValue): "top" | "bottom" | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const edge = (value as { readonly [key: string]: JsonValue }).edge;
  return edge === "top" || edge === "bottom" ? edge : null;
}

function directionArgument(value: JsonValue): Direction | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const direction = (value as { readonly [key: string]: JsonValue }).direction;
  return direction === "left" || direction === "right" || direction === "up" || direction === "down"
    ? direction
    : null;
}
