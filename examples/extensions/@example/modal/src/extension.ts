// A tiny modal editor built on key contexts. Enter Modal Mode puts the editor
// into the `modal` context, where h/j/k/l move the caret, other typed text is
// swallowed, and Escape leaves. Everything else, including typing outside the
// context, stays native. `c` changes the word at the caret: it deletes the
// word and enters the `inserting` context, where typing is native, and Escape
// returns to modal mode. The deletion and everything typed meanwhile are one
// undo step.
import { commands, keymaps } from "knot";
import type { Buffer, Disposable, Editor, JsonValue } from "knot";
import { follow } from "./follow.js";
import { column, type Direction, wordBounds } from "./motion.js";

const modal = { view: "editor", context: "modal" } as const;
const inserting = { view: "editor", context: "inserting" } as const;

export async function start() {
  await commands.register("example.modal.enter", async ({ editor }) => {
    await editor?.setKeyContext("modal", true);
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
    await editor?.setKeyContext("modal", false);
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
    await editor.setKeyContext("modal", false);
    await editor.setKeyContext("inserting", true);
  }, { palette: false });
  await commands.register("example.modal.finish", async ({ editor }) => {
    const ended = group;
    group = null;
    await ended?.dispose();
    await editor?.setKeyContext("inserting", false);
    await editor?.setKeyContext("modal", true);
  }, { palette: false });

  const keys: [string, Direction][] = [["h", "left"], ["j", "down"], ["k", "up"], ["l", "right"]];
  for (const [key, direction] of keys) {
    keymaps.bind(key, { name: "example.modal.move", arguments: { direction } }, modal);
  }
  keymaps.bind("c", "example.modal.change", modal);
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

function directionArgument(value: JsonValue): Direction | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const direction = (value as { readonly [key: string]: JsonValue }).direction;
  return direction === "left" || direction === "right" || direction === "up" || direction === "down"
    ? direction
    : null;
}
