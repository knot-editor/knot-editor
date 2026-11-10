import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { completion, settings } from "knot";
import * as host from "knot/testing";

beforeEach(() => host.reset());

const words = ["xset", "set", "reset", "toggle word"];

test("match follows the completion.styles setting and options.styles", async () => {
  assert.deepEqual((await completion.match("set", words)).map((found) => found.index), [0, 1, 2]);
  assert.deepEqual((await completion.match("set", words, { styles: ["prefix"] })).map((found) => found.index), [1]);
  await settings.set({ "completion.styles": ["prefix", "substring"] });
  assert.deepEqual(await completion.match("set", words), [
    { index: 1, ranges: [[0, 3]] },
    { index: 0, ranges: [[1, 4]] },
    { index: 2, ranges: [[2, 5]] },
  ]);
});

test("orderless takes parts in any order and an empty query keeps the order", async () => {
  assert.deepEqual(await completion.match("tog word", words, { styles: ["orderless"] }), [
    { index: 3, ranges: [[0, 3], [7, 11]] },
  ]);
  assert.deepEqual(await completion.match("  ", words), words.map((_, index) => ({ index, ranges: [] })));
});
