import assert from "node:assert/strict";
import { test } from "node:test";
import { indentationText, languageText, positionText, selectionSize } from "../src/text.js";

test("positions read as line, column, and the selection when there is one", () => {
  assert.equal(positionText(3, 5, null), "Ln 3, Col 5");
  assert.equal(positionText(3, 5, "12 selected"), "Ln 3, Col 5 (12 selected)");
});

test("small selections count characters and large ones kilobytes", () => {
  assert.equal(selectionSize({ text: "é😀a" } as never, 7), "3 selected");
  assert.equal(selectionSize(null, 3 * 1024 * 1024), "3072 KB selected");
});

test("indentation names the style and its size", () => {
  assert.equal(indentationText({ style: "spaces", size: 4 }), "Spaces: 4");
  assert.equal(indentationText({ style: "tabs", size: 8 }), "Tab Size: 8");
});

test("languages show their name, falling back to the ID", () => {
  const registered = [{ id: "rust", name: "Rust" }] as never;
  assert.equal(languageText("rust", registered), "Rust");
  assert.equal(languageText("zig", registered), "zig");
  assert.equal(languageText("plaintext", registered), "Plain Text");
});
