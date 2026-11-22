// Turning the line hunks of a diff into gutter contributions.
import type { Buffer, DiffHunk, EditorContribution } from "knot";

export const ADDED = "diff.added";
export const MODIFIED = "diff.modified";
export const REMOVED = "diff.removed";

/**
 * One contribution per hunk: a bar over the new lines of an insertion
 * (`diff.added`) or a replacement (`diff.modified`), and a wedge at the
 * boundary where lines were removed (`diff.removed`). Reads line ranges of
 * the buffer's current text, so the caller replaces at the revision it
 * diffed and retries when that no longer holds.
 */
export async function contributionsOf(buffer: Buffer, hunks: readonly DiffHunk[]): Promise<EditorContribution[]> {
  if (hunks.length === 0) return [];
  const last = (await buffer.lineCount()) - 1;
  const items: EditorContribution[] = [];
  for (const { old, new: now } of hunks) {
    if (now.end === now.start) {
      // A removal after the last line of a text without a final newline sits
      // at the end of the text, where the wedge is drawn below that line.
      const { startByteOffset } = now.start > last
        ? { startByteOffset: (await buffer.lineRange(last)).endByteOffset }
        : await buffer.lineRange(now.start);
      items.push({
        range: { startByteOffset, endByteOffset: startByteOffset },
        gutter: { color: REMOVED, style: "deletion" },
      });
      continue;
    }
    const first = await buffer.lineRange(Math.min(now.start, last));
    const final = await buffer.lineRange(Math.min(now.end - 1, last));
    items.push({
      range: { startByteOffset: first.startByteOffset, endByteOffset: final.endByteOffset },
      gutter: { color: old.end === old.start ? ADDED : MODIFIED, style: "bar" },
    });
  }
  return items;
}
