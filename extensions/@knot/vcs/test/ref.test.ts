import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { windows } from "knot";
import * as host from "knot/testing";
import { backendContract, vcs } from "../src/contract.js";
import { start } from "../src/extension.js";

type Ref = { label: string; tooltip?: string } | null;

let started: Promise<unknown> | null = null;
const running = () => (started ??= start());

beforeEach(() => {
  host.reset();
  started = null;
});

const settle = async () => {
  for (let turn = 0; turn < 8; turn += 1) await new Promise((resolve) => setImmediate(resolve));
};

const A = "file:///a";
const B = "file:///b";

/** A backend whose repositories are the given roots and whose ref is read from `refs` by repository. */
async function backend(refs: Record<string, Ref>) {
  await running();
  await backendContract("one").serve({
    repositories: (root) => Object.keys(refs).filter((repository) => root === repository || repository.startsWith(`${root}/`)),
    repositoryOf: (directory) => Object.keys(refs).find((repository) => directory === repository || directory.startsWith(`${repository}/`)) ?? null,
    baseline: () => null,
    status: () => ({ files: [], groups: [] }), compare: () => null, rollback: () => {}, commitOptions: () => [], commitMessage: () => null, commit: () => null,
    ref: (repository) => refs[repository] ?? null,
  });
  await (await vcs.client()).register("one");
}

/** What the window's status bar shows for the ref item. */
function shown(window: ReturnType<typeof host.openWindow>) {
  const item = host.statusItems(window).find((candidate) => candidate.id === "vcs.ref");
  return item === undefined ? null : { text: item.text, icon: item.icon, tooltip: item.tooltip };
}

test("a window shows the ref of its first root", async () => {
  const window = host.openWindow({ roots: [A] });
  await backend({ [A]: { label: "main", tooltip: "Branch main" } });
  await settle();
  assert.deepEqual(shown(window), { text: "main", icon: "vcs.branch", tooltip: "Branch main" });
});

test("the focused editor's repository wins over the first root's", async () => {
  const document = host.addDocument({ uri: `${B}/src/file.txt` });
  const window = host.openWindow({ roots: [A, B], documents: [document] });
  await backend({ [A]: { label: "first" }, [B]: { label: "second" } });
  await settle();
  assert.equal(shown(window)?.text, "second");
});

test("a focused file outside every repository falls back to the first root", async () => {
  const document = host.addDocument({ uri: "file:///elsewhere/file.txt" });
  const window = host.openWindow({ roots: [A], documents: [document] });
  await backend({ [A]: { label: "main" } });
  await settle();
  assert.equal(shown(window)?.text, "main");
});

test("a window without a repository shows nothing", async () => {
  const window = host.openWindow({ roots: ["file:///plain"] });
  const bare = host.openWindow({});
  await backend({ [A]: { label: "main" } });
  await settle();
  assert.equal(shown(window), null);
  assert.equal(shown(bare), null);
});

test("a repository the backend has no ref for shows nothing", async () => {
  const window = host.openWindow({ roots: [A] });
  await backend({ [A]: null });
  await settle();
  assert.equal(shown(window), null);
});

test("each window shows its own ref", async () => {
  const first = host.openWindow({ roots: [A] });
  const second = host.openWindow({ roots: [B] });
  await backend({ [A]: { label: "one" }, [B]: { label: "two" } });
  await settle();
  assert.equal(shown(first)?.text, "one");
  assert.equal(shown(second)?.text, "two");
});

test("the ref follows the focused editor", async () => {
  const inA = host.addDocument({ uri: `${A}/x.txt` });
  const inB = host.addDocument({ uri: `${B}/y.txt` });
  const window = host.openWindow({ roots: [A, B], documents: [inA] });
  await backend({ [A]: { label: "one" }, [B]: { label: "two" } });
  await settle();
  assert.equal(shown(window)?.text, "one");

  host.present(window, inB);
  await host.emit({ type: "activeTargetChanged", activeWindow: window, activeTarget: host.tabTarget(window, 1) });
  await settle();
  assert.equal(shown(window)?.text, "two");
});

test("the ref follows the roots of the window", async () => {
  const window = host.openWindow({ roots: [A] });
  await backend({ [A]: { label: "one" }, [B]: { label: "two" } });
  await settle();

  const workspace = host.setRoots(window, [B]);
  await host.emit({ type: "workspaceChanged", workspace });
  await settle();
  assert.equal(shown(window)?.text, "two");

  host.setRoots(window, ["file:///plain"]);
  await host.emit({ type: "workspaceChanged", workspace: (await windows.get(window))!.workspace });
  await settle();
  assert.equal(shown(window), null);
});

test("a change reported by the backend updates the ref", async () => {
  const window = host.openWindow({ roots: [A] });
  const refs: Record<string, Ref> = { [A]: { label: "main" } };
  await backend(refs);
  await settle();
  assert.equal(shown(window)?.text, "main");

  refs[A] = { label: "abc1234" };
  await (await vcs.client()).changed({ backend: "one", repository: A });
  await settle();
  assert.equal(shown(window)?.text, "abc1234");
});

test("a status change leaves the ref alone", async () => {
  const window = host.openWindow({ roots: [A] });
  const refs: Record<string, Ref> = { [A]: { label: "main" } };
  await backend(refs);
  await settle();

  refs[A] = { label: "other" };
  await (await vcs.client()).statusChanged({ backend: "one", repository: A });
  await settle();
  assert.equal(shown(window)?.text, "main");
});
