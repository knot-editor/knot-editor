// The languages this package registers: one per LSP language ID, each
// claiming the file names, extensions, and shebang interpreters detection
// matches.
import type { LanguageDefinition } from "knot";
import * as python from "./queries/python.js";

export const LANGUAGES: readonly LanguageDefinition[] = [
  {
    id: "python",
    name: "Python",
    grammar: "python",
    fileExtensions: ["py", "pyi", "pyw"],
    interpreters: ["python", "python3"],
    queries: { highlights: python.highlights, indents: python.indents },
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
