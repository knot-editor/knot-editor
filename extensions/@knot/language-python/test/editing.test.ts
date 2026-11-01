// The editing data native comment toggling, bracket matching, and
// auto-closing read for Python.
import assert from "node:assert/strict";
import { test } from "node:test";
import { LANGUAGES } from "../src/languages.js";

const language = (id: string) => LANGUAGES.find((candidate) => candidate.id === id)!;

const python = language("python");

test("comments are # lines only; docstrings are strings", () => {
  assert.deepEqual(python.comments, { line: ["#"] });
});

test("quotes auto-close outside strings and comments", () => {
  assert.deepEqual(python.brackets, [
    { open: "{", close: "}" },
    { open: "[", close: "]" },
    { open: "(", close: ")" },
    { open: '"', close: '"', notIn: ["string", "comment"] },
    { open: "'", close: "'", notIn: ["string", "comment"] },
  ]);
  assert.equal(python.autoCloseBefore, ";:.,=}])>");
});
