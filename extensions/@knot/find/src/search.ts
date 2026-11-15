// Matching and navigation over `buffer.find`, which does the matching
// natively. Offsets are UTF-8 bytes, as everywhere in the editor API.
import type { Buffer, ByteRange, TextEdit } from "knot";

/** The most matches highlighted at once. */
export const HIGHLIGHT_LIMIT = 1000;
/** The most `buffer.find` returns, and so the most one step back can read. */
const READ_LIMIT = 100_000;

export type Options = { caseSensitive: boolean; wholeWord: boolean; regex: boolean };

export type Highlights = { revision: number; matches: readonly ByteRange[]; more: boolean };

/** The matches to highlight: the first `HIGHLIGHT_LIMIT` of the buffer, or of
 * `scope`. */
export function highlights(
  buffer: Buffer,
  query: string,
  options: Options,
  scope: ByteRange | null,
): Promise<Highlights> {
  return buffer.find(query, { ...options, ...(scope && { range: scope }), limit: HIGHLIGHT_LIMIT });
}

export async function endOf(buffer: Buffer): Promise<number> {
  const last = await buffer.lineRange((await buffer.lineCount()) - 1);
  return last.endByteOffset;
}

/** The first match starting at or after `from`, wrapping to the first match of
 * the buffer, or of `scope`; `null` without any. */
export async function matchFrom(
  buffer: Buffer,
  query: string,
  options: Options,
  from: number,
  scope: ByteRange | null,
): Promise<ByteRange | null> {
  const bounds = scope ?? { startByteOffset: 0, endByteOffset: await endOf(buffer) };
  const start = Math.max(bounds.startByteOffset, Math.min(from, bounds.endByteOffset));
  const after = await buffer.find(query, {
    ...options,
    range: { startByteOffset: start, endByteOffset: bounds.endByteOffset },
    limit: 1,
  });
  if (after.matches.length > 0) return after.matches[0]!;
  const first = await buffer.find(query, { ...options, range: bounds, limit: 1 });
  return first.matches[0] ?? null;
}

/** The last match ending at or before `before`, wrapping to the last match of
 * the buffer, or of `scope`; `null` without any. Reading back past the
 * highlight limit costs what the matches before the caret do. */
export async function matchBefore(
  buffer: Buffer,
  query: string,
  options: Options,
  before: number,
  scope: ByteRange | null,
): Promise<ByteRange | null> {
  const bounds = scope ?? { startByteOffset: 0, endByteOffset: await endOf(buffer) };
  const end = Math.max(bounds.startByteOffset, Math.min(before, bounds.endByteOffset));
  const preceding = await buffer.find(query, {
    ...options,
    range: { startByteOffset: bounds.startByteOffset, endByteOffset: end },
    limit: READ_LIMIT,
  });
  if (preceding.matches.length > 0) return preceding.matches.at(-1)!;
  const all = await buffer.find(query, { ...options, range: bounds, limit: READ_LIMIT });
  return all.matches.at(-1) ?? null;
}

/** `scope` after `edits`, which are in the scope's own coordinates, or `null`
 * when they leave nothing of it. Text inserted at either end joins the scope. */
export function rebase(scope: ByteRange, edits: readonly TextEdit[]): ByteRange | null {
  const start = moveThrough(scope.startByteOffset, edits, false);
  const end = moveThrough(scope.endByteOffset, edits, true);
  return end > start ? { startByteOffset: start, endByteOffset: end } : null;
}

/** Where the offset `at` lands after `edits`. An edit around it leaves a start
 * before the replacement and an end after it. */
function moveThrough(at: number, edits: readonly TextEdit[], isEnd: boolean): number {
  let shift = 0;
  for (const { range, text } of edits) {
    const { startByteOffset: from, endByteOffset: to } = range;
    const change = utf8Length(text) - (to - from);
    if (to <= at) {
      shift += change;
    } else if (from < at) {
      return from + shift + (isEnd ? utf8Length(text) : 0);
    }
  }
  return at + shift;
}

function utf8Length(text: string): number {
  let bytes = 0;
  for (const char of text) {
    const code = char.codePointAt(0)!;
    bytes += code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
  }
  return bytes;
}

/** The bar's message line for a search whose current match is `current`. */
export function summary(query: string, found: Highlights, current: ByteRange | null): string | null {
  if (query === "") return null;
  if (found.more) return `${HIGHLIGHT_LIMIT}+`;
  if (found.matches.length === 0) return "No results";
  const index = current === null ? -1 : found.matches.findIndex((match) => sameRange(match, current));
  const count = found.matches.length;
  return index >= 0 ? `${index + 1} of ${count}` : `${count} ${count === 1 ? "result" : "results"}`;
}

export function sameRange(a: ByteRange, b: ByteRange): boolean {
  return a.startByteOffset === b.startByteOffset && a.endByteOffset === b.endByteOffset;
}

/** The parser's message when `error` is an invalid regular expression, else
 * `null`. */
export function invalidPattern(error: unknown): string | null {
  const host = error as { code?: string; message?: string; details?: { message?: string } };
  return host.code === "InvalidQuery" ? (host.details?.message ?? host.message ?? "Invalid pattern") : null;
}
