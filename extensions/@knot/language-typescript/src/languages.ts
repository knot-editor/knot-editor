// The languages this package registers: one per LSP language ID, each
// claiming the file names, extensions, and shebang interpreters detection
// matches. Languages that share a grammar family compose their highlight
// queries the way nvim-treesitter's `; inherits:` does: the inherited query
// comes first, so the more specific patterns after it win. Indent queries
// compose the same way, and so do fold queries.
import type { LanguageDefinition } from "knot";
import * as ecma from "./queries/ecma.js";
import * as folds from "./queries/folds.js";
import * as javascript from "./queries/javascript.js";
import * as jsx from "./queries/jsx.js";
import * as typescript from "./queries/typescript.js";

const javascriptHighlights = ecma.highlights + jsx.highlights + javascript.highlights;
const javascriptIndents = ecma.indents + jsx.indents;
const javascriptFolds = folds.ecma + folds.jsx;

// Editing data every ECMAScript language shares. JSX children comment with
// `{/* */}`, which needs syntax-scoped configuration.
const EDITING = {
  comments: { line: ["//"], block: ["/*", "*/"] },
  // No angle brackets: comparisons and arrows would mismatch.
  brackets: [
    { open: "{", close: "}" },
    { open: "[", close: "]" },
    { open: "(", close: ")" },
    { open: '"', close: '"', notIn: ["string", "comment"] },
    { open: "'", close: "'", notIn: ["string", "comment"] },
    { open: "`", close: "`", notIn: ["string"] },
  ],
  autoCloseBefore: ";:.,=}])>",
} as const satisfies Partial<LanguageDefinition>;

export const LANGUAGES: readonly LanguageDefinition[] = [
  {
    id: "javascript",
    name: "JavaScript",
    grammar: "javascript",
    fileExtensions: ["js", "mjs", "cjs"],
    interpreters: ["node"],
    queries: { highlights: javascriptHighlights, indents: javascriptIndents, folds: javascriptFolds },
    ...EDITING,
  },
  {
    id: "javascriptreact",
    name: "JavaScript JSX",
    grammar: "javascript",
    fileExtensions: ["jsx"],
    queries: { highlights: javascriptHighlights, indents: javascriptIndents, folds: javascriptFolds },
    ...EDITING,
  },
  {
    id: "typescript",
    name: "TypeScript",
    grammar: "typescript",
    fileExtensions: ["ts", "mts", "cts"],
    interpreters: ["deno", "ts-node"],
    queries: {
      highlights: ecma.highlights + typescript.highlights,
      indents: ecma.indents + typescript.indents,
      folds: folds.ecma + folds.typescript,
    },
    ...EDITING,
  },
  {
    id: "typescriptreact",
    name: "TypeScript JSX",
    grammar: "tsx",
    fileExtensions: ["tsx"],
    queries: {
      highlights: ecma.highlights + typescript.highlights + jsx.highlights,
      indents: ecma.indents + typescript.indents + jsx.indents,
      folds: folds.ecma + folds.typescript + folds.jsx,
    },
    ...EDITING,
  },
];

/** The `file.<id>` icon of each language: a Lucide glyph, in a color of the
 * language's own where it has one (the `icon.<id>` color key). */
export const FILE_ICONS: Readonly<
  Record<string, { lucide: string; color?: { dark: string; light: string } }>
> = {
  typescript: { lucide: "file-code", color: { dark: "#5aa0e6", light: "#3178c6" } },
  typescriptreact: { lucide: "file-code", color: { dark: "#4bc0d9", light: "#1f7a99" } },
  javascript: { lucide: "file-code", color: { dark: "#e5c84b", light: "#a58a00" } },
  javascriptreact: { lucide: "file-code", color: { dark: "#4bc0d9", light: "#1f7a99" } },
};
