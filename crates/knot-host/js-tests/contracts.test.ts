import assert from "node:assert/strict";
import { test } from "node:test";
import { loadBootstrap, stubBindings } from "./harness.js";

const stub = stubBindings({
  respond: ({ operation, args }) =>
    operation === "serveContract" ? 5
      : operation === "contractClient" ? 9
      : operation === "callContract" ? (args[1] === "refresh" ? "null" : '["a"]')
      : null,
});
const host = await loadBootstrap(stub);
const { defineContract, s } = host.contractModule("@example/provider");
const rows = defineContract("rows", {
  files: s.fn(s.string(), s.array(s.string())),
  refresh: s.fn(s.void(), s.void()),
});
const descriptor = JSON.stringify(rows.methods);

test("serving sends the owner, name, and descriptor", async () => {
  await assert.rejects(rows.serve({ files: () => [] } as never), /refresh method/);
  const registration = await rows.serve({ files: () => [], refresh() {} });
  assert.deepEqual(stub.calls.find((call) => call.operation === "serveContract")?.args, [
    "@example/provider", "rows", descriptor,
  ]);
  await registration.dispose();
  assert.deepEqual(stub.calls.at(-1), { operation: "unregisterProvider", args: [5] });
});

test("a client calls through the host with JSON text and the caller's window", async () => {
  const client = await rows.client();
  assert.deepEqual(Object.keys(client).sort(), ["files", "refresh"]);
  assert.deepEqual(stub.calls.find((call) => call.operation === "contractClient")?.args, [
    "@example/provider", "rows", descriptor,
  ]);
  assert.deepEqual(await client.files("query"), ["a"]);
  assert.deepEqual(stub.calls.at(-1), { operation: "callContract", args: [9, "files", '"query"', null] });
  assert.equal(await client.refresh(), undefined, "a void result is undefined");
  assert.deepEqual(stub.calls.at(-1)?.args, [9, "refresh", "null", null]);
  await assert.rejects((client.files as (...values: unknown[]) => Promise<unknown>)("a", "b"), /at most one argument/);
});

test("a package defines each contract name once", () => {
  assert.throws(() => defineContract("rows", { files: s.fn(s.void(), s.void()) }),
    /contract rows is defined twice in @example\/provider/);
  const other = host.contractModule("@example/other");
  assert.doesNotThrow(() => other.defineContract("rows", { files: s.fn(s.void(), s.void()) }));
});
