// A tiny modal editor built on key contexts. Enter Modal Mode puts the editor
// into the `modal` context, where h/j/k/l move the caret, other typed text is
// swallowed, and Escape leaves. Everything else, including typing outside the
// context, stays native.
import { commands, keymaps } from "knot";
import type { Buffer, Editor, JsonValue } from "knot";
import { column, type Direction } from "./motion.js";

const modal = { view: "editor", context: "modal" } as const;

export async function start() {
  await commands.register("example.modal.enter", async ({ editor }) => {
    await editor?.setKeyContext("modal", true);
  }, { title: "Example: Enter Modal Mode" });
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

  const keys: [string, Direction][] = [["h", "left"], ["j", "down"], ["k", "up"], ["l", "right"]];
  for (const [key, direction] of keys) {
    keymaps.bind(key, { name: "example.modal.move", arguments: { direction } }, modal);
  }
  keymaps.bind("escape", "example.modal.leave", modal);
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

function directionArgument(value: JsonValue): Direction | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const direction = (value as { readonly [key: string]: JsonValue }).direction;
  return direction === "left" || direction === "right" || direction === "up" || direction === "down"
    ? direction
    : null;
}
