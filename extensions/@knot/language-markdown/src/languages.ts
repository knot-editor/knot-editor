// The languages this package registers: one per LSP language ID, each
// claiming the file names, extensions, and shebang interpreters detection
// matches.
import type { LanguageDefinition } from "knot";
import * as injections from "./queries/injections.js";
import * as markdown from "./queries/markdown.js";
import * as markdownInline from "./queries/markdown_inline.js";

export const LANGUAGES: readonly LanguageDefinition[] = [
  {
    id: "markdown",
    name: "Markdown",
    grammar: "markdown",
    fileExtensions: ["md", "markdown", "mkd"],
    queries: { highlights: markdown.highlights, injections: injections.markdown },
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
  },
  {
    // Markdown's inline syntax, which Markdown injects into each paragraph.
    id: "markdown_inline",
    name: "Markdown Inline",
    grammar: "markdown_inline",
    queries: { highlights: markdownInline.highlights, injections: injections.markdownInline },
    // Comments toggle with the language at the cursor, which inside a
    // paragraph is this one.
    comments: { block: ["<!--", "-->"] },
  },
];
