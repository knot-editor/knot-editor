import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import * as host from "knot/testing";
import { backendContract, vcs } from "../src/contract.js";
import { start } from "../src/extension.js";

beforeEach(() => host.reset());

test("the same backend ID yields the same contract and another ID another", () => {
  assert.equal(backendContract("git"), backendContract("git"));
  assert.notEqual(backendContract("git"), backendContract("other"));
  assert.equal(backendContract("git").name, "backend.git");
  assert.throws(() => backendContract(""), TypeError);
});

test("a registered backend is listed while it is served and not after it stops", async () => {
  await start();
  const serving = await backendContract("one").serve({
    repositories: () => [],
    repositoryOf: () => null,
    baseline: () => null,
    status: () => [],
    ref: () => null,
  });
  const client = await vcs.client();
  await client.register("one");
  assert.deepEqual(await client.backends(), ["one"]);

  await serving[Symbol.asyncDispose]();
  assert.deepEqual(await client.backends(), []);
});

test("a backend that unregisters is not listed", async () => {
  await start();
  await backendContract("one").serve({ repositories: () => [], repositoryOf: () => null, baseline: () => null, status: () => [], ref: () => null });
  const client = await vcs.client();
  await client.register("one");
  await client.unregister("one");
  assert.deepEqual(await client.backends(), []);
});
