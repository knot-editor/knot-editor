import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { history } from "knot";
import * as host from "knot/testing";

beforeEach(() => host.reset());

const items = ["a", "b", "c", "d"].map((id) => ({ id }));

test("recording moves an id to the front and order lists recent items first", async () => {
  assert.deepEqual(await history.order("things", items), items);
  await history.record("things", "c");
  await history.record("things", "b");
  await history.record("things", "c");
  assert.deepEqual(await history.recent("things"), ["c", "b"]);
  assert.deepEqual((await history.order("things", items)).map((item) => item.id), ["c", "b", "a", "d"]);
});

test("names are separate, ids unknown to the items are ignored, and a key function picks the identity", async () => {
  await history.record("one", "x");
  await history.record("two", "b");
  assert.deepEqual(await history.recent("one"), ["x"]);
  assert.deepEqual((await history.order("two", items)).map((item) => item.id), ["b", "a", "c", "d"]);
  const files = [{ uri: "u1" }, { uri: "u2" }];
  await history.record("files", "u2");
  assert.deepEqual(await history.order("files", files, (file) => file.uri), [{ uri: "u2" }, { uri: "u1" }]);
});

test("the list keeps the 50 most recent ids", async () => {
  for (let index = 0; index < 60; index++) await history.record("many", `id${index}`);
  const recent = await history.recent("many");
  assert.equal(recent.length, 50);
  assert.equal(recent[0], "id59");
});

test("forgetting removes one id, or the whole list without one", async () => {
  await history.record("things", "a");
  await history.record("things", "b");
  await history.forget("things", "a");
  await history.forget("things", "missing");
  assert.deepEqual(await history.recent("things"), ["b"]);
  await history.forget("things");
  assert.deepEqual(await history.recent("things"), []);
});
