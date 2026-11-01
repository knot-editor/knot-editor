// The editing data native comment toggling, bracket matching, and
// auto-closing read for TOML.
import assert from "node:assert/strict";
import { test } from "node:test";
import { LANGUAGES } from "../src/languages.js";

const language = (id: string) => LANGUAGES.find((candidate) => candidate.id === id)!;

const toml = language("toml");

test("comments are # lines", () => {
  assert.deepEqual(toml.comments, { line: ["#"] });
});

test("inline tables never open a middle line; arrays may span lines", () => {
  assert.deepEqual(toml.brackets, [
    { open: "{", close: "}", newline: false },
    { open: "[", close: "]" },
    { open: '"', close: '"', notIn: ["string", "comment"] },
    { open: "'", close: "'", notIn: ["string", "comment"] },
  ]);
  assert.equal(toml.autoCloseBefore, ",]}");
});
