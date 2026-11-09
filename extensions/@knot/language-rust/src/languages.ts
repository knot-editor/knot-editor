// The languages this package registers: one per LSP language ID, each
// claiming the file names, extensions, and shebang interpreters detection
// matches.
import type { LanguageDefinition } from "knot";
import * as folds from "./queries/folds.js";
import * as indents from "./queries/indents.js";
import * as injections from "./queries/injections.js";
import * as rust from "./queries/rust.js";

export const LANGUAGES: readonly LanguageDefinition[] = [
  {
    id: "rust",
    name: "Rust",
    grammar: "rust",
    fileExtensions: ["rs"],
    queries: { highlights: rust.highlights, injections: injections.rust, indents: indents.rust, folds: folds.rust },
    comments: { line: ["//", "///", "//!"], block: ["/*", "*/"] },
    // No single quote: lifetimes and labels outnumber character literals.
    // No angle brackets: comparisons, shifts, and arrows would mismatch.
    brackets: [
      { open: "{", close: "}" },
      { open: "[", close: "]" },
      { open: "(", close: ")" },
      { open: '"', close: '"', notIn: ["string", "comment"] },
    ],
    autoCloseBefore: ";:.,=}])>",
  },
];

/** The `file.<id>` icon of each language: a Lucide glyph, in a color of the
 * language's own where it has one (the `icon.<id>` color key). */
export const FILE_ICONS: Readonly<
  Record<string, { lucide: string; color?: { dark: string; light: string } }>
> = {
  rust: { lucide: "file-code", color: { dark: "#dea584", light: "#b7410e" } },
};
