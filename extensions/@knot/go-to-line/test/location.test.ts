import assert from "node:assert/strict";
import { test } from "node:test";
import type { TextSnapshot } from "knot";
import { columnAt, columnOffset, parseLocation } from "../src/location.js";

test("lines, compiler locations, and whitespace parse", () => {
  assert.deepEqual(parseLocation("120"), { line: 120, column: null });
  assert.deepEqual(parseLocation(" 120:8 "), { line: 120, column: 8 });
  assert.deepEqual(parseLocation("120,8"), { line: 120, column: 8 });
  assert.deepEqual(parseLocation("007"), { line: 7, column: null });
});

test("anything else is not a location", () => {
  for (const text of ["", " ", "abc", "0", "1:0", "+5", "-1", "1:", ":3", "1:2:3", "1.5", "1 2"]) {
    assert.equal(parseLocation(text), null, text);
  }
});

/** A snapshot of one line, converting offsets as the host does. */
function line(text: string): TextSnapshot {
  const bytes = (part: string) => new TextEncoder().encode(part).length;
  return {
    text,
    range: { startByteOffset: 0, endByteOffset: bytes(text) },
    revision: 0,
    byteOffsetAtUtf16: (offset) => bytes(text.slice(0, offset)),
    utf16OffsetAtByte: (offset) => {
      let index = 0;
      while (bytes(text.slice(0, index)) < offset) index += 1;
      return index;
    },
  };
}

test("columns count scalar values, with a tab as one, and clamp to the end", () => {
  const text = line("\té😀x");
  assert.equal(columnOffset(text, 1), 0);
  assert.equal(columnOffset(text, 2), 1);
  assert.equal(columnOffset(text, 3), 3);
  assert.equal(columnOffset(text, 4), 7);
  assert.equal(columnOffset(text, 5), 8);
  assert.equal(columnOffset(text, 99), 8);
  assert.deepEqual([0, 1, 3, 7, 8].map((offset) => columnAt(text, offset)), [1, 2, 3, 4, 5]);
});
