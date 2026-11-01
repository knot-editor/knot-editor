// The editing data native comment toggling, bracket matching, and
// auto-closing read for the ECMAScript languages, which share one set.
import assert from "node:assert/strict";
import { test } from "node:test";
import { LANGUAGES } from "../src/languages.js";

const language = (id: string) => LANGUAGES.find((candidate) => candidate.id === id)!;

const IDS = ["javascript", "javascriptreact", "typescript", "typescriptreact"];

test("every ECMAScript language comments with // and /* */", () => {
  // Only "//", as in VS Code and Zed: toggling a "///" triple-slash
  // directive leaves "/ <reference …>".
  for (const id of IDS) {
    assert.deepEqual(language(id).comments, { line: ["//"], block: ["/*", "*/"] }, id);
  }
});

test("quotes auto-close outside strings and comments, template literals outside strings", () => {
  // No angle brackets, which comparisons and arrows would mismatch.
  for (const id of IDS) {
    assert.deepEqual(
      language(id).brackets,
      [
        { open: "{", close: "}" },
        { open: "[", close: "]" },
        { open: "(", close: ")" },
        { open: '"', close: '"', notIn: ["string", "comment"] },
        { open: "'", close: "'", notIn: ["string", "comment"] },
        { open: "`", close: "`", notIn: ["string"] },
      ],
      id,
    );
    assert.equal(language(id).autoCloseBefore, ";:.,=}])>", id);
  }
});
