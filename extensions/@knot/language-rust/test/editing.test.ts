// The editing data native comment toggling, bracket matching, and
// auto-closing read for Rust.
import assert from "node:assert/strict";
import { test } from "node:test";
import { LANGUAGES } from "../src/languages.js";

const language = (id: string) => LANGUAGES.find((candidate) => candidate.id === id)!;

const rust = language("rust");

test("commenting inserts // and uncommenting recognizes doc comments", () => {
  // Uncommenting strips the longest token, so "/// a" becomes "a", not "/ a".
  assert.deepEqual(rust.comments, { line: ["//", "///", "//!"], block: ["/*", "*/"] });
});

test("brackets pair code delimiters and double quotes outside strings and comments", () => {
  // No angle brackets, which comparisons, shifts, and arrows would mismatch,
  // and no single quote, which lifetimes and labels use more than characters.
  assert.deepEqual(rust.brackets, [
    { open: "{", close: "}" },
    { open: "[", close: "]" },
    { open: "(", close: ")" },
    { open: '"', close: '"', notIn: ["string", "comment"] },
  ]);
  assert.equal(rust.autoCloseBefore, ";:.,=}])>");
});
