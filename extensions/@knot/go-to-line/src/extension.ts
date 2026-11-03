// Go to Line: type `N`, `N:C`, or `N,C` to move the caret there, previewed
// while typing. Enter keeps the position; dismissing restores the view.
import { commands, workbench } from "knot";
import type { Buffer, CommandContext, Editor } from "knot";
import { columnAt, columnOffset, parseLocation } from "./location.js";
import type { Location } from "./location.js";

export const COMMAND = "editor.go-to-line";

/** A resolved target: its byte offset and the 1-based line and column it
 * clamped to. */
type Target = { offset: number; line: number; column: number };

async function resolve(buffer: Buffer, location: Location, lineCount: number): Promise<Target> {
  const line = Math.min(location.line, lineCount);
  const range = await buffer.lineRange(line - 1);
  if (location.column === null) return { offset: range.startByteOffset, line, column: 1 };
  const text = await buffer.snapshot(range);
  const offset = columnOffset(text, location.column);
  return { offset: range.startByteOffset + offset, line, column: columnAt(text, offset) };
}

async function caretPosition(buffer: Buffer, caret: number) {
  const line = await buffer.lineAt(caret);
  const range = await buffer.lineRange(line);
  const text = await buffer.snapshot(range);
  const offset = Math.min(caret, range.endByteOffset) - range.startByteOffset;
  return { line: line + 1, column: columnAt(text, offset) };
}

function describe(target: Target, location: Location) {
  return location.column === null
    ? `Go to line ${target.line}`
    : `Go to line ${target.line}, character ${target.column}`;
}

async function goToLine(editor: Editor, buffer: Buffer) {
  const original = await editor.viewState();
  const lineCount = await buffer.lineCount();
  const current = await caretPosition(buffer, original.caret);
  const usage = `Current Line: ${current.line} of ${lineCount} (column ${current.column})`;

  // Moves the caret to a typed location, or restores the view for anything
  // else, and returns the message to show.
  const preview = async (value: string) => {
    const location = parseLocation(value);
    if (location === null) {
      await editor.restoreViewState(original);
      return usage;
    }
    const target = await resolve(buffer, location, lineCount);
    await editor.setSelection({ anchor: target.offset, caret: target.offset }, { reveal: "center" });
    return describe(target, location);
  };

  const result = await workbench.showInput({ title: "Go to Line", onChange: preview });
  if (result.kind === "dismissed") {
    await editor.restoreViewState(original);
    return;
  }
  // The accepted text may be newer than the last preview.
  await preview(result.value);
}

export async function start() {
  await commands.register(
    COMMAND,
    async ({ editor, buffer }: CommandContext) => {
      if (editor === null || buffer === null) commands.unavailable();
      await goToLine(editor, buffer);
    },
    { title: "Go to Line…", description: "Move the cursor to a line and column." },
  );
}
