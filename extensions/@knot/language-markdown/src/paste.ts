// Pasting a URL over selected text makes the selection a link.
import type { PasteProvider, PasteRequest } from "knot";

/** One whitespace-free token that starts with a scheme and `://`, or is a `mailto:` address. */
const URL_PATTERN = /^(?:[a-z][a-z0-9+.-]*:\/\/|mailto:)\S+$/i;

/** Whether `text`, trimmed, is a URL. */
export function isUrl(text: string): boolean {
  return URL_PATTERN.test(text.trim());
}

/** `[selectedText](url)` for a URL pasted over a selection that is not itself a URL; else null. */
export function linkOver({ text, selectedText }: Pick<PasteRequest, "text" | "selectedText">): string | null {
  if (selectedText === "" || isUrl(selectedText) || !isUrl(text)) return null;
  return `[${selectedText}](${text.trim()})`;
}

export const pasteProvider: PasteProvider = { providePaste: linkOver };
