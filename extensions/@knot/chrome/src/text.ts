// The text the status bar items show.
import type { DocumentInfo, LanguageInfo, TextSnapshot } from "knot";

/** Selections up to this many bytes are counted in characters; larger ones
 * would send the whole selection to JavaScript on every change. */
export const COUNT_LIMIT = 1 << 20;

/** The 1-based column of a byte offset in a line's text, counted in Unicode
 * scalar values with a tab as one, as Go to Line reads columns. */
export function columnAt(text: string, offset: number): number {
  let bytes = 0;
  let column = 1;
  for (const character of text) {
    if (bytes >= offset) break;
    const code = character.codePointAt(0)!;
    bytes += code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
    column++;
  }
  return column;
}

/** The selection's size: its characters when the selection is small, else
 * its size in kilobytes. */
export function selectionSize(text: TextSnapshot | null, bytes: number): string {
  if (text !== null) return `${[...text.text].length} selected`;
  return `${Math.round(bytes / 1024)} KB selected`;
}

export function positionText(line: number, column: number, selected: string | null): string {
  return `Ln ${line}, Col ${column}${selected === null ? "" : ` (${selected})`}`;
}

export function indentationText(indentation: DocumentInfo["indentation"]): string {
  return `${indentation.style === "tabs" ? "Tab Size" : "Spaces"}: ${indentation.size}`;
}

export function languageText(id: string, languages: readonly LanguageInfo[]): string {
  if (id === "plaintext") return "Plain Text";
  return languages.find((language) => language.id === id)?.name ?? id;
}
