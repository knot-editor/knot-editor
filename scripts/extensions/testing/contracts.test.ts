import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { completionContract, pasteContract } from "knot";
import { defineContract, s } from "knot/contract";
import * as host from "knot/testing";

beforeEach(() => host.reset());

const greeter = defineContract("greeter", { greet: s.fn(s.string(), s.string()) });

test("a client binds to the latest registration and falls back when it is disposed", async () => {
  await assert.rejects(greeter.client(), { code: "ContractNotFound" });
  await greeter.serve({ greet: (name) => `hello ${name}` });
  const later = await greeter.serve({ greet: (name) => `hi ${name}` });
  assert.equal(await (await greeter.client()).greet("knot"), "hi knot");
  await later[Symbol.asyncDispose]();
  assert.equal(await (await greeter.client()).greet("knot"), "hello knot");
});

test("a held client rejects once disposed, and provider failures cross back", async () => {
  const registration = await greeter.serve({
    greet: () => { throw new Error("provider broke"); },
  });
  const client = await greeter.client();
  await assert.rejects(client.greet("knot"), { code: "ProviderFailure", message: "provider broke" });
  await registration[Symbol.asyncDispose]();
  await assert.rejects(client.greet("knot"), { code: "ContractNotFound" });
});

test("a provider sees the caller's window", async () => {
  const window = host.openWindow({ roots: [] });
  const same = defineContract("same", { same: s.fn(s.void(), s.boolean()) });
  await same.serve({ same: (_, context) => context.window === window });
  assert.equal(await (await same.client()).same(), true);
});

test("serve requires every method", async () => {
  await assert.rejects(greeter.serve({} as never), TypeError);
});

test("Knot's contracts are served under knot: names the test calls with contractClient", async () => {
  await completionContract.serve({ provideCompletions: (request) => [{ label: request.prefix, insertText: request.prefix }] });
  const served = await pasteContract("markdown").serve({ providePaste: () => "answer" });
  const completion = await host.contractClient("knot:completion");
  assert.deepEqual(await completion.provideCompletions({ prefix: "a" }), [{ label: "a", insertText: "a" }]);
  const paste = await host.contractClient("knot:paste.markdown");
  assert.equal(await paste.providePaste({}), "answer");
  await served[Symbol.asyncDispose]();
  await assert.rejects(host.contractClient("knot:paste.markdown"), { code: "ContractNotFound" });
  assert.throws(() => pasteContract(""), TypeError);
  await assert.rejects(completionContract.serve({} as never), TypeError);
});
