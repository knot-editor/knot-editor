import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { buffers } from "knot";
import * as host from "knot/testing";

beforeEach(() => host.reset());

const insert = async (buffer: { snapshot(): Promise<{ revision: number }>; applyEdits: Function }, text: string) => {
  const { revision } = await buffer.snapshot();
  await buffer.applyEdits([{ range: { startByteOffset: 0, endByteOffset: 0 }, text }], { ifRevision: revision });
};

test("a buffer holds text and is edited unless it is read-only", async () => {
  const editable = await buffers.create({ text: "a" });
  await insert(editable, "b");
  assert.equal((await editable.snapshot()).text, "ba");
  const fixed = await buffers.create({ text: "x", readOnly: true });
  await assert.rejects(() => insert(fixed, "y"), { code: "BufferReadOnly" });
  await assert.rejects(() => buffers.create({ readOnly: true, persist: "key" }), TypeError);
});

test("a disposed buffer is closed", async () => {
  const buffer = await buffers.create();
  await buffer[Symbol.asyncDispose]();
  await assert.rejects(() => buffer.snapshot(), { code: "BufferClosed" });
});

test("persisted text starts a later buffer until cleared, and a key has one live buffer", async () => {
  const first = await buffers.create({ text: "draft", persist: "message" });
  await assert.rejects(() => buffers.create({ persist: "message" }), { code: "PersistKeyInUse" });
  await insert(first, "a");
  await first[Symbol.asyncDispose]();
  const second = await buffers.create({ text: "ignored", persist: "message" });
  assert.equal((await second.snapshot()).text, "adraft");
  await second.clearPersisted();
  await second[Symbol.asyncDispose]();
  assert.equal((await (await buffers.create({ text: "fresh", persist: "message" })).snapshot()).text, "fresh");
});
