// The languages this package registers: one per LSP language ID, each
// claiming the file names, extensions, and shebang interpreters detection
// matches.
import type { LanguageDefinition } from "knot";
import * as injections from "./queries/injections.js";
import * as markdown from "./queries/markdown.js";
import * as markdownInline from "./queries/markdown_inline.js";

// Editing data both languages share: comments and brackets apply with the
// language at the cursor, which inside a paragraph is Markdown Inline.
const EDITING = {
  comments: { block: ["<!--", "-->"] },
  // Prose: Enter never opens a middle line, and quotes only surround,
  // since apostrophes would auto-close.
  brackets: [
    { open: "{", close: "}", newline: false },
    { open: "[", close: "]", newline: false },
    { open: "(", close: ")", newline: false },
    { open: "<", close: ">", newline: false },
    { open: "`", close: "`" },
    { open: '"', close: '"', autoClose: false },
    { open: "'", close: "'", autoClose: false },
  ],
  autoCloseBefore: ";:.,=}])>",
} as const satisfies Partial<LanguageDefinition>;

export const LANGUAGES: readonly LanguageDefinition[] = [
  {
    id: "markdown",
    name: "Markdown",
    grammar: "markdown",
    fileExtensions: ["md", "markdown", "mkd"],
    queries: { highlights: markdown.highlights, injections: injections.markdown },
    ...EDITING,
  },
  {
    // Markdown's inline syntax, which Markdown injects into each paragraph.
    id: "markdown_inline",
    name: "Markdown Inline",
    grammar: "markdown_inline",
    queries: { highlights: markdownInline.highlights, injections: injections.markdownInline },
    ...EDITING,
  },
];
