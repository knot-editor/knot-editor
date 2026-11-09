// The languages this package registers: one per LSP language ID, each
// claiming the file names, extensions, and shebang interpreters detection
// matches.
import type { LanguageDefinition } from "knot";
import * as folds from "./queries/folds.js";
import * as toml from "./queries/toml.js";

export const LANGUAGES: readonly LanguageDefinition[] = [
  {
    id: "toml",
    name: "TOML",
    grammar: "toml",
    fileExtensions: ["toml"],
    fileNames: ["Cargo.lock", "Pipfile"],
    queries: { highlights: toml.highlights, indents: toml.indents, folds: folds.toml },
    comments: { line: ["#"] },
    brackets: [
      // Inline tables stay on one line.
      { open: "{", close: "}", newline: false },
      { open: "[", close: "]" },
      { open: '"', close: '"', notIn: ["string", "comment"] },
      { open: "'", close: "'", notIn: ["string", "comment"] },
    ],
    autoCloseBefore: ",]}",
  },
];

/** The `file.<id>` icon of each language: a Lucide glyph, in a color of the
 * language's own where it has one (the `icon.<id>` color key). */
export const FILE_ICONS: Readonly<
  Record<string, { lucide: string; color?: { dark: string; light: string } }>
> = {
  toml: { lucide: "file-cog", color: { dark: "#c8845a", light: "#9c4221" } },
};
