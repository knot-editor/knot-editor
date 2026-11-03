// The text the status bar items show.
import type { DocumentInfo, LanguageInfo, TextSnapshot } from "knot";

/** Selections up to this many bytes are counted in characters; larger ones
 * would send the whole selection to JavaScript on every change. */
export const COUNT_LIMIT = 1 << 20;

/** The 1-based column of a byte offset relative to the snapshot, counted in
 * Unicode scalar values with a tab as one, as Go to Line reads columns. */
export function columnAt(line: TextSnapshot, offset: number): number {
  return [...line.text.slice(0, line.utf16OffsetAtByte(offset))].length + 1;
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
