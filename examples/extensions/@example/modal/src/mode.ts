// The modes of the example and how each looks: normal movement draws a block
// caret, inserting leaves the native bar.
import type { Editor } from "knot";

export type Mode = "modal" | "inserting" | null;

/** Put `editor` in `mode`, or in no mode with `null`. */
export async function setMode(editor: Editor, mode: Mode) {
  await editor.setKeyContext("modal", mode === "modal");
  await editor.setKeyContext("inserting", mode === "inserting");
  await editor.setCursorStyle(mode === "modal" ? "block" : null);
}
