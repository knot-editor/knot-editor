// Turning the line hunks of a diff into what the tab shows: bands and gutter
// wedges on each side, the strip between them, and the scroll map.
import type {
  Buffer,
  CompositeConnectorLink,
  CompositeScrollLink,
  DiffHunk,
  EditorBlockRows,
  ViewDecoration,
} from "knot";

export type Side = "original" | "modified";

const ADDED = "diff.added";
const REMOVED = "diff.removed";
const ADDED_BACKGROUND = "diff.added.background";
const REMOVED_BACKGROUND = "diff.removed.background";
const MODIFIED_BACKGROUND = "diff.modified.background";

/** A color key's dimmer form for a hunk whose change the staged text already holds. */
const dimmed = (color: string, staged: boolean | undefined) => (staged === true ? `${color}.staged` : color);

/** Whether two ranges of lines share a line, or an empty one lies within or beside the other. */
function overlap(left: DiffHunk["new"], right: DiffHunk["new"]): boolean {
  if (left.start === left.end || right.start === right.end) return left.start <= right.end && right.start <= left.end;
  return left.start < right.end && right.start < left.end;
}

/**
 * For each of `hunks`, the original's against the modified text, whether it
 * is staged: `unstaged`, the hunks of the staged text against the modified
 * text, has none overlapping its modified lines. A partly staged hunk counts
 * as unstaged.
 */
export function stagedFlags(hunks: readonly DiffHunk[], unstaged: readonly DiffHunk[]): boolean[] {
  return hunks.map((hunk) => !unstaged.some((other) => overlap(hunk.new, other.new)));
}

/** The strip's links: one band per hunk, colored by what the hunk does. */
export function connectorLinks(hunks: readonly DiffHunk[], staged: readonly boolean[] = []): CompositeConnectorLink[] {
  return hunks.map(({ old, new: now }, index) => ({
    first: old,
    second: now,
    color: dimmed(
      old.start === old.end ? ADDED_BACKGROUND : now.start === now.end ? REMOVED_BACKGROUND : MODIFIED_BACKGROUND,
      staged[index],
    ),
  }));
}

/** The inline layout's rows: each hunk's original lines above the modified line they precede. */
export function blocksOf(
  hunks: readonly DiffHunk[],
  original: Buffer,
  staged: readonly boolean[] = [],
): EditorBlockRows[] {
  return hunks.flatMap(({ old, new: now }, index) =>
    old.start === old.end
      ? []
      : [{ line: now.start, source: original, lines: old, color: dimmed(REMOVED_BACKGROUND, staged[index]) }],
  );
}

/** The scroll link of the original and modified editors, keyed as the layout is. */
export function scrollLink(hunks: readonly DiffHunk[]): CompositeScrollLink {
  return { first: "original", second: "modified", map: hunks.map(({ old, new: now }) => ({ first: old, second: now })) };
}

/**
 * The bands of one side: a line band over each hunk's lines, and a wedge at
 * the boundary where the side has none. Reads line ranges of the buffer's
 * current text, so the caller sets them at the revision it diffed.
 */
export async function decorationsOf(
  buffer: Buffer,
  hunks: readonly DiffHunk[],
  side: Side,
  staged: readonly boolean[] = [],
): Promise<ViewDecoration[]> {
  if (hunks.length === 0) return [];
  const last = (await buffer.lineCount()) - 1;
  const items: ViewDecoration[] = [];
  for (const [index, { old, new: now }] of hunks.entries()) {
    const [lines, other] = side === "original" ? [old, now] : [now, old];
    if (lines.start === lines.end) {
      // After the last line of a text without a final newline, the wedge sits at the end of the text.
      const offset =
        lines.start > last
          ? (await buffer.lineRange(last)).endByteOffset
          : (await buffer.lineRange(lines.start)).startByteOffset;
      items.push({
        range: { startByteOffset: offset, endByteOffset: offset },
        gutter: { color: dimmed(side === "original" ? ADDED : REMOVED, staged[index]), style: "deletion" },
      });
      continue;
    }
    const first = await buffer.lineRange(Math.min(lines.start, last));
    const final = await buffer.lineRange(Math.min(lines.end - 1, last));
    const color =
      other.start !== other.end ? MODIFIED_BACKGROUND : side === "original" ? REMOVED_BACKGROUND : ADDED_BACKGROUND;
    items.push({
      range: { startByteOffset: first.startByteOffset, endByteOffset: final.endByteOffset },
      decoration: { color: dimmed(color, staged[index]), style: "line" },
    });
  }
  return items;
}

/**
 * The line a move from `caretLine` lands on: the first line of the nearest
 * hunk after (or before) it on `side`, wrapping at the ends, or null without
 * hunks. A removal's line is the one that followed the removed lines.
 */
export function targetLine(
  hunks: readonly DiffHunk[],
  side: Side,
  caretLine: number,
  direction: "next" | "previous",
): number | null {
  if (hunks.length === 0) return null;
  const starts = hunks.map((hunk) => (side === "original" ? hunk.old.start : hunk.new.start));
  if (direction === "next") return starts.find((start) => start > caretLine) ?? starts[0]!;
  const before = starts.filter((start) => start < caretLine);
  return before[before.length - 1] ?? starts[starts.length - 1]!;
}
