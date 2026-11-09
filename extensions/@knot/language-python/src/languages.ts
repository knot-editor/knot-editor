// The languages this package registers: one per LSP language ID, each
// claiming the file names, extensions, and shebang interpreters detection
// matches.
import type { LanguageDefinition } from "knot";
import * as folds from "./queries/folds.js";
import * as python from "./queries/python.js";

export const LANGUAGES: readonly LanguageDefinition[] = [
  {
    id: "python",
    name: "Python",
    grammar: "python",
    fileExtensions: ["py", "pyi", "pyw"],
    interpreters: ["python", "python3"],
    queries: { highlights: python.highlights, indents: python.indents, folds: folds.python },
    // Docstrings are strings, not block comments.
    comments: { line: ["#"] },
    brackets: [
      { open: "{", close: "}" },
      { open: "[", close: "]" },
      { open: "(", close: ")" },
      { open: '"', close: '"', notIn: ["string", "comment"] },
      { open: "'", close: "'", notIn: ["string", "comment"] },
    ],
    autoCloseBefore: ";:.,=}])>",
  },
];

/** The `file.<id>` icon of each language: a Lucide glyph, in a color of the
 * language's own where it has one (the `icon.<id>` color key). */
export const FILE_ICONS: Readonly<
  Record<string, { lucide: string; color?: { dark: string; light: string } }>
> = {
  python: { lucide: "file-code", color: { dark: "#6ea8d9", light: "#3572a5" } },
};
