import assert from "node:assert/strict";
import { test } from "node:test";
import { rebase } from "../src/search.js";

const scope = { startByteOffset: 10, endByteOffset: 20 };
const edit = (start: number, end: number, text: string) => ({
  range: { startByteOffset: start, endByteOffset: end },
  text,
});
const range = (start: number, end: number) => ({ startByteOffset: start, endByteOffset: end });

test("edits before the scope move it and edits after leave it", () => {
  assert.deepEqual(rebase(scope, [edit(0, 0, "abc")]), range(13, 23));
  assert.deepEqual(rebase(scope, [edit(2, 6, "")]), range(6, 16));
  assert.deepEqual(rebase(scope, [edit(25, 25, "abc")]), scope);
});

test("text typed at either end or inside joins the scope", () => {
  assert.deepEqual(rebase(scope, [edit(10, 10, "ab")]), range(12, 22));
  assert.deepEqual(rebase(scope, [edit(20, 20, "ab")]), range(10, 22));
  assert.deepEqual(rebase(scope, [edit(15, 15, "é")]), range(10, 22));
});

test("edits are in the scope's coordinates, so each shifts by those before it", () => {
  assert.deepEqual(rebase(scope, [edit(0, 2, ""), edit(12, 14, "xyz")]), range(8, 19));
});

test("an edit across an end keeps what lies outside it", () => {
  assert.deepEqual(rebase(scope, [edit(5, 14, "")]), range(5, 11));
  assert.deepEqual(rebase(scope, [edit(15, 30, "ab")]), range(10, 17));
  assert.deepEqual(rebase(scope, [edit(5, 14, "abcd")]), range(5, 15));
});

test("an edit that removes the whole scope leaves nothing", () => {
  assert.equal(rebase(scope, [edit(10, 20, "")]), null);
  assert.equal(rebase(scope, [edit(0, 30, "")]), null);
});
