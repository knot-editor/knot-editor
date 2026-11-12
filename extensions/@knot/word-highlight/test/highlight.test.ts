import assert from "node:assert/strict";
import { test } from "node:test";
import { SELECTION_COLOR, SELECTION_LIMIT, decorations, selectionQuery } from "../src/highlight.js";

test("a selection query is single-line text that is not only whitespace and not long", () => {
  assert.equal(selectionQuery("foo bar"), "foo bar");
  assert.equal(selectionQuery(" x "), " x ");
  assert.equal(selectionQuery(""), null);
  assert.equal(selectionQuery("  \t"), null);
  assert.equal(selectionQuery("a\nb"), null);
  assert.equal(selectionQuery("a\r"), null);
  assert.equal(selectionQuery("x".repeat(SELECTION_LIMIT)), "x".repeat(SELECTION_LIMIT));
  assert.equal(selectionQuery("x".repeat(SELECTION_LIMIT + 1)), null);
  assert.equal(selectionQuery("😀".repeat(SELECTION_LIMIT)), "😀".repeat(SELECTION_LIMIT));
});

test("decorations paint each match as a background and leave out the skipped one", () => {
  const matches = [
    { startByteOffset: 0, endByteOffset: 3 },
    { startByteOffset: 8, endByteOffset: 11 },
  ];
  assert.deepEqual(decorations(matches, SELECTION_COLOR, matches[0]!), [
    { range: matches[1], decoration: { color: SELECTION_COLOR, style: "background" } },
  ]);
  assert.equal(decorations(matches, SELECTION_COLOR, null).length, 2);
});
