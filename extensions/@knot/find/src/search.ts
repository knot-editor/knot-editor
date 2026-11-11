// Matching and navigation over `buffer.find`, which does the matching
// natively. Offsets are UTF-8 bytes, as everywhere in the editor API.
import type { Buffer, ByteRange } from "knot";

/** The most matches highlighted at once. */
export const HIGHLIGHT_LIMIT = 1000;
/** The most `buffer.find` returns, and so the most one step back can read. */
const READ_LIMIT = 100_000;

export type Options = { caseSensitive: boolean; wholeWord: boolean };

export type Highlights = { revision: number; matches: readonly ByteRange[]; more: boolean };

/** The matches to highlight: the first `HIGHLIGHT_LIMIT` of the buffer. */
export function highlights(buffer: Buffer, query: string, options: Options): Promise<Highlights> {
  return buffer.find(query, { ...options, limit: HIGHLIGHT_LIMIT });
}

async function endOf(buffer: Buffer): Promise<number> {
  const last = await buffer.lineRange((await buffer.lineCount()) - 1);
  return last.endByteOffset;
}

/** The first match starting at or after `from`, wrapping to the first match of
 * the buffer; `null` without any. */
export async function matchFrom(
  buffer: Buffer,
  query: string,
  options: Options,
  from: number,
): Promise<ByteRange | null> {
  const end = await endOf(buffer);
  const after = await buffer.find(query, {
    ...options,
    range: { startByteOffset: Math.min(from, end), endByteOffset: end },
    limit: 1,
  });
  if (after.matches.length > 0) return after.matches[0]!;
  const first = await buffer.find(query, { ...options, limit: 1 });
  return first.matches[0] ?? null;
}

/** The last match ending at or before `before`, wrapping to the last match of
 * the buffer; `null` without any. Reading back past the highlight limit costs
 * what the matches before the caret do. */
export async function matchBefore(
  buffer: Buffer,
  query: string,
  options: Options,
  before: number,
): Promise<ByteRange | null> {
  const preceding = await buffer.find(query, {
    ...options,
    range: { startByteOffset: 0, endByteOffset: before },
    limit: READ_LIMIT,
  });
  if (preceding.matches.length > 0) return preceding.matches.at(-1)!;
  const all = await buffer.find(query, { ...options, limit: READ_LIMIT });
  return all.matches.at(-1) ?? null;
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
