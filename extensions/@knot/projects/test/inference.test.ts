import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import * as host from "knot/testing";
import { inferProjectRoot, type InferenceInput } from "../src/inference.js";
import {
  INFERENCE_DISMISSED_KEY,
  createState,
  dismissedFromJson,
  loadDismissed,
  rememberDismissed,
} from "../src/state.js";
import { startLaunched } from "./support.js";

beforeEach(() => host.reset());

function input(overrides: Partial<InferenceInput> & { documentUri: string | null }): InferenceInput {
  return {
    windowRoots: [],
    allRoots: [],
    homeUri: null,
    dismissed: new Set(),
    ...overrides,
  };
}

const DOC = "file:///repo/src/main.rs";

test("nearest ancestor wins and a .git file counts", async () => {
  host.seedFile("file:///repo/.git/HEAD", "ref: refs/heads/main");
  host.seedFile("file:///repo/sub/.git", "gitdir: ../.git/worktrees/sub");
  assert.equal(await inferProjectRoot(input({ documentUri: "file:///repo/sub/src/main.rs" })), "file:///repo/sub/");
});

test("a .git directory counts", async () => {
  host.seedFile("file:///repo/.git/HEAD", "ref: refs/heads/main");
  assert.equal(await inferProjectRoot(input({ documentUri: DOC })), "file:///repo/");
});

test("no marker resolves to null", async () => {
  host.seedFile("file:///repo/src/main.rs", "fn main() {}");
  assert.equal(await inferProjectRoot(input({ documentUri: DOC })), null);
});

test("a document under one of its window's roots resolves to null", async () => {
  host.seedFile("file:///repo/.git/HEAD", "ref: refs/heads/main");
  assert.equal(await inferProjectRoot(input({ documentUri: DOC, windowRoots: ["file:///repo/"] })), null);
});

test("a document at the filesystem root resolves to null", async () => {
  host.seedFile("file:///.git", "gitdir: /elsewhere");
  assert.equal(await inferProjectRoot(input({ documentUri: "file:///main.rs" })), null);
});

test("the home directory never infers", async () => {
  host.seedFile("file:///Users/me/.git/HEAD", "ref: refs/heads/main");
  const documentUri = "file:///Users/me/dotfiles/todo.txt";
  assert.equal(
    await inferProjectRoot(input({ documentUri, homeUri: "file:///Users/me/" })),
    null,
  );
  assert.equal(await inferProjectRoot(input({ documentUri, homeUri: null })), "file:///Users/me/");
});

test("a root already open in another window resolves to null", async () => {
  host.seedFile("file:///repo/.git/HEAD", "ref: refs/heads/main");
  assert.equal(
    await inferProjectRoot(input({ documentUri: DOC, allRoots: ["file:///other/", "file:///repo"] })),
    null,
  );
});

test("a dismissed root resolves to null", async () => {
  host.seedFile("file:///repo/.git/HEAD", "ref: refs/heads/main");
  assert.equal(
    await inferProjectRoot(input({ documentUri: DOC, dismissed: new Set(["file:///repo"]) })),
    null,
  );
});

test("unstored and non-file documents resolve to null", async () => {
  host.seedFile("file:///repo/.git/HEAD", "ref: refs/heads/main");
  assert.equal(await inferProjectRoot(input({ documentUri: null })), null);
  assert.equal(await inferProjectRoot(input({ documentUri: "https://example.com/repo/src/main.rs" })), null);
});

test("a stat failure resolves to null", async () => {
  host.seedFile("file:///repo/.git/HEAD", "ref: refs/heads/main");
  host.failNext("files.stat", "StorageUnavailable");
  assert.equal(await inferProjectRoot(input({ documentUri: DOC })), null);
});

test("dismissal records round-trip normalized roots", () => {
  assert.deepEqual([...dismissedFromJson({ schema: 1, roots: ["file:///repo/", "file:///sub"] })], [
    "file:///repo",
    "file:///sub",
  ]);
  assert.throws(() => dismissedFromJson({ schema: 2, roots: [] }), /unsupported/);
  assert.throws(() => dismissedFromJson({ schema: 1, roots: ["file:///repo/", 7] }), /invalid entry/);
});

test("dismissals load, persist, and keep the last good set on failure", async () => {
  const state = createState();
  await loadDismissed(state);
  assert.equal(state.dismissed.size, 0);

  await rememberDismissed(state, "file:///repo/");
  assert.deepEqual(host.record(INFERENCE_DISMISSED_KEY), { schema: 1, roots: ["file:///repo"] });

  const reloaded = createState();
  await loadDismissed(reloaded);
  assert.deepEqual([...reloaded.dismissed], ["file:///repo"]);

  host.setRecord(INFERENCE_DISMISSED_KEY, { schema: 9, roots: [] });
  await loadDismissed(reloaded);
  assert.deepEqual([...reloaded.dismissed], ["file:///repo"]);

  host.failNext("storage.write", "StorageUnavailable");
  await rememberDismissed(reloaded, "file:///sub/");
  assert.ok(reloaded.dismissed.has("file:///sub"));
});

test("starting with a stored dismissal record works", async () => {
  host.setRecord(INFERENCE_DISMISSED_KEY, { schema: 1, roots: ["file:///repo"] });
  await startLaunched();
});
