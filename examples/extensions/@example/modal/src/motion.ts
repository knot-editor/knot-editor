export type Direction = "left" | "right" | "up" | "down";

/**
 * The UTF-16 column a motion from `from` lands on in the target line `text`:
 * one character left or right within the line, or the same column clamped to
 * the line for up and down. Never splits a surrogate pair.
 */
export function column(text: string, from: number, direction: Direction): number {
  if (direction === "left") {
    if (from === 0) return 0;
    return from - (isLowSurrogate(text, from - 1) && from >= 2 ? 2 : 1);
  }
  if (direction === "right") {
    if (from >= text.length) return text.length;
    return from + (isHighSurrogate(text, from) ? 2 : 1);
  }
  const clamped = Math.min(from, text.length);
  return isLowSurrogate(text, clamped) ? clamped - 1 : clamped;
}

const isHighSurrogate = (text: string, index: number) => {
  const unit = text.charCodeAt(index);
  return unit >= 0xd800 && unit <= 0xdbff;
};

const isLowSurrogate = (text: string, index: number) => {
  const unit = text.charCodeAt(index);
  return unit >= 0xdc00 && unit <= 0xdfff;
};

/**
 * The UTF-16 range of the word at `from` in `text`, or the rest of the run of
 * word characters after it; empty when none follows.
 */
export function wordBounds(text: string, from: number): [number, number] {
  const word = /[\p{L}\p{N}_]/u;
  const chars = [...text];
  let index = 0;
  let unit = 0;
  while (index < chars.length && unit + chars[index]!.length <= from) unit += chars[index++]!.length;
  let start = index;
  while (start > 0 && word.test(chars[start - 1]!)) start--;
  let end = index;
  while (end < chars.length && word.test(chars[end]!)) end++;
  if (end === start || (start === index && !word.test(chars[index] ?? ""))) return [from, from];
  const length = (count: number) => chars.slice(0, count).join("").length;
  return [length(start), length(end)];
}
