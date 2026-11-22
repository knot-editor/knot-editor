import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { providers } from "knot";
import * as host from "knot/testing";

beforeEach(() => host.reset());

type Greeter = { greet(name: string): Promise<string> };

test("get returns the latest registration and all every one in order", async () => {
  await providers.register("sample.greet", { greet: (name: string) => `hello ${name}` });
  const later = await providers.register("sample.greet", { greet: (name: string) => `hi ${name}` });
  assert.equal(await (await providers.get<Greeter>("sample.greet"))!.greet("knot"), "hi knot");
  const all = await providers.all<Greeter>("sample.greet");
  assert.deepEqual(await Promise.all(all.map((handle) => handle.greet("all"))), ["hello all", "hi all"]);
  await later[Symbol.asyncDispose]();
  assert.equal(await (await providers.get<Greeter>("sample.greet"))!.greet("knot"), "hello knot");
  assert.equal(await providers.get("missing"), null);
});

test("a held handle rejects once disposed, and provider failures cross back", async () => {
  const registration = await providers.register("sample.greet", {
    greet: () => { throw new Error("provider broke"); },
  });
  const handle = (await providers.get<Greeter>("sample.greet"))!;
  await assert.rejects(handle.greet("knot"), { code: "ProviderFailure", message: "provider broke" });
  await registration[Symbol.asyncDispose]();
  await assert.rejects(handle.greet("knot"), { code: "ProviderNotFound" });
});

test("a provider sees the caller's window", async () => {
  const window = host.openWindow({ roots: [] });
  await providers.register("sample.window", {
    same: (_: null, context: { window: unknown }) => context.window === window,
  });
  assert.equal(await (await providers.get<{ same(): Promise<boolean> }>("sample.window"))!.same(), true);
});

test("Knot's contracts are served as providers the test calls by name", async () => {
  const { completionContract, pasteContract } = await import("knot");
  await completionContract.serve({ provideCompletions: (request) => [{ label: request.prefix, insertText: request.prefix }] });
  const served = await pasteContract("markdown").serve({ providePaste: () => "answer" });
  const completion = (await providers.get<{ provideCompletions(request: unknown): Promise<unknown> }>("knot:completion"))!;
  assert.deepEqual(await completion.provideCompletions({ prefix: "a" }), [{ label: "a", insertText: "a" }]);
  const paste = (await providers.get<{ providePaste(request: unknown): Promise<unknown> }>("knot:paste.markdown"))!;
  assert.equal(await paste.providePaste({}), "answer");
  await served[Symbol.asyncDispose]();
  assert.equal(await providers.get("knot:paste.markdown"), null);
  assert.throws(() => pasteContract(""), TypeError);
  await assert.rejects(completionContract.serve({} as never), TypeError);
});
