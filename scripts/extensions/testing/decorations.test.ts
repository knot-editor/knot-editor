import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { decorations } from "knot";
import * as host from "knot/testing";

beforeEach(() => host.reset());

test("a source sets, removes, and clears decorations until it is disposed", async () => {
  const source = await decorations.createSource();
  await source.set({ "file:///a": { color: "error", badge: { text: "E" } }, "file:///b": { tooltip: "b" } });
  await source.set({ "file:///b": null });
  assert.deepEqual(host.decorationsOf("file:///a"), [{ color: "error", badge: { text: "E" } }]);
  assert.deepEqual(host.decorationsOf("file:///b"), []);
  await source.clear();
  assert.deepEqual(host.decorationsOf("file:///a"), []);
  await source.set({ "file:///a": { color: "info" } });
  await source[Symbol.asyncDispose]();
  assert.deepEqual(host.decorationsOf("file:///a"), []);
  await assert.rejects(source.set({}), { code: "OwnershipDisposed" });
});

test("a malformed badge or URI is rejected", async () => {
  const source = await decorations.createSource();
  await assert.rejects(source.set({ "file:///a": { badge: { text: "ABC" } } }), TypeError);
  await assert.rejects(source.set({ "file:///a": { badge: { text: "" } } }), TypeError);
  await assert.rejects(source.set({ "file:///a": { badge: { icon: "" } } }), TypeError);
  await assert.rejects(source.set({ nope: { color: "error" } }), { code: "InvalidUri" });
});
