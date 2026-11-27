import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import * as host from "knot/testing";
import { backendContract, vcs } from "../src/contract.js";
import { start } from "../src/extension.js";
import { LIMIT, SHOW_OUTPUT, trimmed } from "../src/output.js";

beforeEach(() => {
  host.reset();
  started = null;
});

let started: Promise<unknown> | null = null;
const running = () => (started ??= start());

async function backend(id: string) {
  await running();
  const serving = await backendContract(id).serve({
    repositories: () => [],
    repositoryOf: () => null,
    baseline: () => null,
    status: () => ({ files: [], groups: [] }),
    compare: () => null,
    rollback: () => {},
    commitOptions: () => [],
    commitMessage: () => null,
    commit: () => null,
    hunkActions: () => [],
    ref: () => null,
  });
  await (await vcs.client()).register(id);
  return serving;
}

const entry = (command: string, output = "", failed = false) => ({
  backend: "fake",
  repository: "file:///work",
  command,
  output,
  failed,
});

test("the output is a tool window of a text frame, hidden until asked for", async () => {
  await running();
  const { contribution, visible, text } = host.view("vcs.output");
  assert.equal(contribution.content?.kind, "text");
  assert.equal(contribution.placement?.location, "bottomRight");
  assert.equal(visible, false);
  assert.equal(text, "");
});

test("each command appends a block with its line, output, and result", async () => {
  await backend("fake");
  const client = await vcs.client();
  await client.log(entry("git add -A -- a.txt"));
  await client.log(entry("git commit", "hook said no", true));
  assert.equal(
    host.view("vcs.output").text,
    "[fake] file:///work\n$ git add -A -- a.txt\nDone\n\n" +
      "[fake] file:///work\n$ git commit\nhook said no\nFailed\n\n",
  );
});

test("a backend that is not registered cannot log", async () => {
  await backend("fake");
  const client = await vcs.client();
  await client.log({ ...entry("git status"), backend: "other" });
  assert.equal(host.view("vcs.output").text, "");
  const served = await backend("later");
  await (await vcs.client()).unregister("later");
  await (await vcs.client()).log({ ...entry("git status"), backend: "later" });
  await served[Symbol.asyncDispose]();
  assert.equal(host.view("vcs.output").text, "");
});

test("the log keeps its end, from the start of a line", async () => {
  const line = "x".repeat(99) + "\n";
  const long = line.repeat(Math.ceil((LIMIT * 2) / line.length));
  const cut = trimmed(long);
  assert.ok(cut.length <= LIMIT && cut.length > LIMIT - line.length);
  assert.ok(cut.startsWith(line));
  assert.equal(trimmed("short\n"), "short\n");

  await backend("fake");
  const client = await vcs.client();
  await client.log(entry("git big", long));
  await client.log(entry("git last"));
  const text = host.view("vcs.output").text;
  assert.ok(text.length <= LIMIT);
  assert.ok(text.endsWith("$ git last\nDone\n\n"));
});

test("Show Version Control Output shows and focuses the tool window", async () => {
  await running();
  host.openWindow();
  assert.equal(host.view("vcs.output").visible, false);
  await host.run(SHOW_OUTPUT);
  assert.equal(host.view("vcs.output").visible, true);
});

test("the Changes toolbar has an output button", async () => {
  host.openWindow();
  await running();
  for (let turn = 0; turn < 8; turn += 1) await new Promise((resolve) => setImmediate(resolve));
  const toolbar = host.view("vcs.changes").updates.at(-1)?.toolbar ?? [];
  assert.ok(toolbar.some((button) => "command" in button && button.command === SHOW_OUTPUT));
});
