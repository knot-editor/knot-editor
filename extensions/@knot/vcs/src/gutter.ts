// Turning the line hunks of a diff into gutter contributions.
import type { Buffer, DiffHunk, EditorContribution } from "knot";

export const ADDED = "diff.added";
export const MODIFIED = "diff.modified";
export const REMOVED = "diff.removed";

/** The command a click on a change mark runs. */
export const SHOW_HUNK = "vcs.showHunk";

/**
 * One contribution per hunk: a bar over the new lines of an insertion
 * (`diff.added`) or a replacement (`diff.modified`), and a wedge at the
 * boundary where lines were removed (`diff.removed`). A click on a mark
 * runs `vcs.showHunk`. Reads line ranges of the buffer's current text, so the caller replaces at the revision it
 * diffed and retries when that no longer holds.
 */
/** The gutter color of a hunk's mark, dimmer when the hunk is staged. */
export function colorOf({ old, new: now }: DiffHunk, staged = false): string {
  const color = now.end === now.start ? REMOVED : old.end === old.start ? ADDED : MODIFIED;
  return staged ? `${color}.staged` : color;
}

type Lines = DiffHunk["new"];

/** Whether two ranges of new lines share a line, or a removal's boundary lies within or beside the other. */
function overlap(left: Lines, right: Lines): boolean {
  if (left.start === left.end || right.start === right.end) return left.start <= right.end && right.start <= left.end;
  return left.start < right.end && right.start < left.end;
}

/**
 * For each hunk of the baseline-against-buffer diff, whether it is staged:
 * the staged-against-buffer diff, `unstaged`, has no hunk overlapping its new
 * lines, so the buffer equals the staged text there. A partly staged hunk
 * counts as unstaged.
 */
export function stagedFlags(hunks: readonly DiffHunk[], unstaged: readonly DiffHunk[]): boolean[] {
  return hunks.map((hunk) => !unstaged.some((other) => overlap(hunk.new, other.new)));
}

export async function contributionsOf(
  buffer: Buffer,
  hunks: readonly DiffHunk[],
  staged: readonly boolean[] = [],
): Promise<EditorContribution[]> {
  if (hunks.length === 0) return [];
  const last = (await buffer.lineCount()) - 1;
  const items: EditorContribution[] = [];
  for (const [index, { old, new: now }] of hunks.entries()) {
    const color = colorOf({ old, new: now }, staged[index] === true);
    if (now.end === now.start) {
      // A removal after the last line of a text without a final newline sits
      // at the end of the text, where the wedge is drawn below that line.
      const { startByteOffset } = now.start > last
        ? { startByteOffset: (await buffer.lineRange(last)).endByteOffset }
        : await buffer.lineRange(now.start);
      items.push({
        range: { startByteOffset, endByteOffset: startByteOffset },
        gutter: { color, style: "deletion" },
        command: SHOW_HUNK,
      });
      continue;
    }
    const first = await buffer.lineRange(Math.min(now.start, last));
    const final = await buffer.lineRange(Math.min(now.end - 1, last));
    items.push({
      range: { startByteOffset: first.startByteOffset, endByteOffset: final.endByteOffset },
      gutter: { color, style: "bar" },
      command: SHOW_HUNK,
    });
  }
  return items;
}
