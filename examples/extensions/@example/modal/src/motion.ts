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
