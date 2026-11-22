import assert from "node:assert/strict";
import { test } from "node:test";
import { loadBootstrap, stubBindings } from "./harness.js";

const stub = stubBindings({
  respond: ({ operation }) => (operation === "activeBuffer" ? 7 : operation === "snapshot" ? snapshot : null),
});
const snapshot = { text: "a😀é", range: { startByteOffset: 0, endByteOffset: 7 }, revision: 3 };
const host = await loadBootstrap(stub);

test("a JSON value is copied into a frozen plain tree", () => {
  const input = { name: "x", list: [1, { deep: null }], ok: true };
  const copy = host.validateJsonValue(input) as { list: { deep: null }[] };
  assert.deepEqual(JSON.parse(JSON.stringify(copy)), input);
  assert.notEqual(copy, input);
  assert.ok(Object.isFrozen(copy) && Object.isFrozen(copy.list));
  assert.equal(Object.getPrototypeOf(copy), null);
});

test("a value shared twice is not a cycle", () => {
  const shared = { n: 1 };
  assert.doesNotThrow(() => host.validateJsonValue({ a: shared, b: shared }));
});

test("values that are not plain JSON are rejected", () => {
  const cycle: Record<string, unknown> = {};
  cycle.self = cycle;
  const sparse = [1, , 3];
  const cases: [unknown, RegExp][] = [
    [Number.NaN, /finite numbers/],
    [Number.POSITIVE_INFINITY, /finite numbers/],
    [undefined, /JSON-compatible/],
    [() => 1, /JSON-compatible/],
    [cycle, /cycles/],
    [new Date(0), /plain JSON data/],
    [{ toJSON: () => 1 }, /plain JSON data/],
    [{ [Symbol("k")]: 1 }, /plain JSON data/],
    [sparse, /dense/],
    [Object.assign([1], { extra: true }), /indexed data/],
    [{ nested: [undefined] }, /JSON-compatible/],
  ];
  for (const [value, message] of cases) assert.throws(() => host.validateJsonValue(value), message);
});

test("a nonnegative safe integer is the only accepted offset", () => {
  assert.equal(host.safeInteger(0), 0);
  assert.equal(host.safeInteger(Number.MAX_SAFE_INTEGER), Number.MAX_SAFE_INTEGER);
  assert.throws(() => host.safeInteger("1"), TypeError);
  for (const bad of [-1, 1.5, 2 ** 53]) assert.throws(() => host.safeInteger(bad), RangeError);
});

test("ownership refuses use after dispose and releases once", async () => {
  let removed = 0;
  let released = 0;
  const owned = host.ownership(() => removed++, async () => void released++);
  owned.assertActive();
  const first = owned.dispose();
  assert.equal(owned.dispose(), first);
  await first;
  assert.deepEqual([removed, released], [1, 1]);
  assert.throws(() => owned.assertActive(), { name: "KnotHostError", code: "OwnershipDisposed" });
});

test("dispose waits for tracked work and reports a failed release", async () => {
  const order: string[] = [];
  let finish!: () => void;
  const owned = host.ownership(() => order.push("removed"), async () => {
    order.push("release");
    throw new Error("release failed");
  });
  const pending = owned.track(new Promise<void>((resolve) => (finish = resolve)).then(() => order.push("settled")));
  const disposed = owned.dispose();
  const outcome = assert.rejects(disposed, /release failed/);
  await Promise.resolve();
  assert.deepEqual(order, ["removed", "release"]);
  finish();
  await pending;
  await outcome;
  assert.deepEqual(order, ["removed", "release", "settled"]);
});

test("arguments are validated before a request leaves the isolate", async () => {
  stub.calls.length = 0;
  await assert.rejects(host.storageWrite("k", Number.NaN), /finite numbers/);
  await assert.rejects(host.storageWrite(1 as never, 1), /storage key must be text/);
  assert.equal(stub.calls.length, 0);
  await host.storageWrite("k", { a: [1] });
  assert.equal(stub.calls[0]?.operation, "storageWrite");
  assert.deepEqual(JSON.parse(JSON.stringify(stub.calls[0]?.args)), ["k", { a: [1] }]);
});

test("a notice rejects an unknown severity without asking the host", async () => {
  stub.calls.length = 0;
  await assert.rejects(host.showNotice({ message: "m", severity: "fatal" }), /notice severity/);
  assert.equal(stub.calls.length, 0);
});

test("a snapshot converts between UTF-16 and UTF-8 offsets", async () => {
  const buffer = await host.activeBuffer();
  assert.ok(buffer);
  const view = await buffer.snapshot();
  // "a" is 1 byte, the emoji is 2 UTF-16 units and 4 bytes, "é" is 2 bytes.
  assert.equal(view.byteOffsetAtUtf16(0), 0);
  assert.equal(view.byteOffsetAtUtf16(1), 1);
  assert.equal(view.byteOffsetAtUtf16(3), 5);
  assert.equal(view.byteOffsetAtUtf16(4), 7);
  assert.throws(() => view.byteOffsetAtUtf16(2), RangeError);
  assert.throws(() => view.byteOffsetAtUtf16(5), RangeError);
  assert.equal(view.utf16OffsetAtByte(5), 3);
  assert.throws(() => view.utf16OffsetAtByte(2), RangeError);
});
