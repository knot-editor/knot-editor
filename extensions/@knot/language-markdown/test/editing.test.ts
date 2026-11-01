// The editing data native comment toggling, bracket matching, and
// auto-closing read for Markdown.
import assert from "node:assert/strict";
import { test } from "node:test";
import { LANGUAGES } from "../src/languages.js";

const language = (id: string) => LANGUAGES.find((candidate) => candidate.id === id)!;

test("commenting wraps each line in an HTML comment", () => {
  assert.deepEqual(language("markdown").comments, { block: ["<!--", "-->"] });
});

test("prose pairs never open a middle line, and quotes only surround", () => {
  // Auto-closing quotes would double every apostrophe.
  assert.deepEqual(language("markdown").brackets, [
    { open: "{", close: "}", newline: false },
    { open: "[", close: "]", newline: false },
    { open: "(", close: ")", newline: false },
    { open: "<", close: ">", newline: false },
    { open: "`", close: "`" },
    { open: '"', close: '"', autoClose: false },
    { open: "'", close: "'", autoClose: false },
  ]);
  assert.equal(language("markdown").autoCloseBefore, ";:.,=}])>");
});

test("the inline language exists only for injection and has no editing data", () => {
  const inline = language("markdown_inline");
  assert.deepEqual([inline.comments, inline.brackets, inline.autoCloseBefore], [undefined, undefined, undefined]);
});
