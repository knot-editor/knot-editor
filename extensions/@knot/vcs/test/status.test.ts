import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import * as host from "knot/testing";
import { backendContract, vcs } from "../src/contract.js";
import { start } from "../src/extension.js";

type Status = "added" | "modified" | "deleted" | "renamed" | "untracked" | "conflicted";
type Entry = { file: string; status: Status };

let started: Promise<unknown> | null = null;
const running = () => (started ??= start());

beforeEach(() => {
  host.reset();
  started = null;
});

const settle = async () => {
  for (let turn = 0; turn < 8; turn += 1) await new Promise((resolve) => setImmediate(resolve));
};

const ROOT = "file:///work";

type Answers = {
  repositories?: (root: string) => string[];
  status?: (repository: string) => Entry[] | Promise<Entry[]>;
};

async function backend(id: string, answers: Answers) {
  await running();
  const serving = await backendContract(id).serve({
    repositories: (root) => (answers.repositories ?? (() => []))(root),
    repositoryOf: () => null,
    baseline: () => null,
    status: (repository) => (answers.status ?? (() => []))(repository),
  });
  await (await vcs.client()).register(id);
  return serving;
}

/** The one decoration of a URI, or null. */
function shown(uri: string) {
  const [decoration] = host.decorationsOf(uri);
  return decoration ?? null;
}

const colorOf = (uri: string) => shown(uri)?.color ?? null;

function openRoot(roots = [ROOT]) {
  return host.openWindow({ roots, documents: [] });
}

test("each status shows its color, badge, and tooltip", async () => {
  openRoot();
  const files: Record<Status, string> = {
    added: "a",
    modified: "m",
    deleted: "d",
    renamed: "r",
    untracked: "u",
    conflicted: "c",
  };
  await backend("one", {
    repositories: () => [ROOT],
    status: () => Object.entries(files).map(([status, name]) => ({ file: `${ROOT}/${name}.txt`, status: status as Status })),
  });
  await settle();
  const expected: Record<Status, [string, string]> = {
    added: ["A", "Added"],
    modified: ["M", "Modified"],
    deleted: ["D", "Deleted"],
    renamed: ["R", "Renamed"],
    untracked: ["U", "Untracked"],
    conflicted: ["C", "Conflicted"],
  };
  for (const [status, name] of Object.entries(files)) {
    const [badge, tooltip] = expected[status as Status];
    assert.deepEqual(shown(`${ROOT}/${name}.txt`), { color: `vcs.${status}`, badge: { text: badge }, tooltip });
  }
});

test("the status colors are defined for both appearances", async () => {
  await running();
  const keys = host.colorDefinitions().map((definition) => definition.key);
  for (const status of ["added", "modified", "deleted", "renamed", "untracked", "conflicted"]) {
    assert.ok(keys.includes(`vcs.${status}`), status);
  }
});

test("a folder takes the color of its most important change, up to the repository root", async () => {
  openRoot();
  await backend("one", {
    repositories: () => [ROOT],
    status: () => [
      { file: `${ROOT}/src/deep/new.txt`, status: "untracked" },
      { file: `${ROOT}/src/deep/edit.txt`, status: "modified" },
      { file: `${ROOT}/src/add.txt`, status: "added" },
      { file: `${ROOT}/docs/gone.txt`, status: "deleted" },
      { file: `${ROOT}/docs/clash.txt`, status: "conflicted" },
    ],
  });
  await settle();
  assert.deepEqual(shown(`${ROOT}/src/deep`), { color: "vcs.modified" });
  assert.deepEqual(shown(`${ROOT}/src`), { color: "vcs.modified" });
  assert.deepEqual(shown(`${ROOT}/docs`), { color: "vcs.conflicted" });
  assert.deepEqual(shown(ROOT), { color: "vcs.conflicted" });
  assert.equal(shown("file:///"), null);
});

test("untracked and added changes rank below the others in a folder", async () => {
  openRoot();
  await backend("one", {
    repositories: () => [ROOT],
    status: () => [
      { file: `${ROOT}/a/new.txt`, status: "untracked" },
      { file: `${ROOT}/a/add.txt`, status: "added" },
      { file: `${ROOT}/b/new.txt`, status: "untracked" },
      { file: `${ROOT}/c/add.txt`, status: "added" },
      { file: `${ROOT}/c/rename.txt`, status: "renamed" },
    ],
  });
  await settle();
  assert.equal(colorOf(`${ROOT}/a`), "vcs.added");
  assert.equal(colorOf(`${ROOT}/b`), "vcs.untracked");
  assert.equal(colorOf(`${ROOT}/c`), "vcs.renamed");
});

test("a file back to unchanged loses its decoration and its folders recompute", async () => {
  openRoot();
  let entries: Entry[] = [
    { file: `${ROOT}/src/a.txt`, status: "modified" },
    { file: `${ROOT}/src/b.txt`, status: "untracked" },
  ];
  await backend("one", { repositories: () => [ROOT], status: () => entries });
  await settle();
  assert.equal(colorOf(`${ROOT}/src`), "vcs.modified");

  entries = [{ file: `${ROOT}/src/b.txt`, status: "untracked" }];
  await (await vcs.client()).changed({ backend: "one", repository: ROOT });
  await settle();
  assert.equal(shown(`${ROOT}/src/a.txt`), null);
  assert.equal(colorOf(`${ROOT}/src`), "vcs.untracked");

  entries = [];
  await (await vcs.client()).changed({ backend: "one", repository: ROOT });
  await settle();
  assert.equal(shown(`${ROOT}/src/b.txt`), null);
  assert.equal(shown(`${ROOT}/src`), null);
  assert.equal(shown(ROOT), null);
});

test("only the URIs that changed are sent to the source", async () => {
  openRoot();
  let entries: Entry[] = [
    { file: `${ROOT}/a.txt`, status: "modified" },
    { file: `${ROOT}/b.txt`, status: "modified" },
  ];
  await backend("one", { repositories: () => [ROOT], status: () => entries });
  await settle();
  const before = shown(`${ROOT}/b.txt`);
  entries = [
    { file: `${ROOT}/a.txt`, status: "untracked" },
    { file: `${ROOT}/b.txt`, status: "modified" },
  ];
  await (await vcs.client()).changed({ backend: "one", repository: ROOT });
  await settle();
  assert.equal(colorOf(`${ROOT}/a.txt`), "vcs.untracked");
  assert.deepEqual(shown(`${ROOT}/b.txt`), before);
});

test("the deepest repository owns a file, and the outer one's entry for the nested folder is dropped", async () => {
  openRoot();
  const INNER = `${ROOT}/vendor/lib`;
  await backend("one", {
    repositories: () => [ROOT, INNER],
    status: (repository) =>
      repository === ROOT
        ? [
            { file: INNER, status: "untracked" },
            { file: `${INNER}/inside.txt`, status: "untracked" },
            { file: `${ROOT}/top.txt`, status: "modified" },
          ]
        : [{ file: `${INNER}/inside.txt`, status: "modified" }],
  });
  await settle();
  assert.equal(colorOf(`${INNER}/inside.txt`), "vcs.modified");
  assert.deepEqual(shown(INNER), { color: "vcs.modified" });
  assert.equal(colorOf(`${ROOT}/top.txt`), "vcs.modified");
  assert.equal(colorOf(`${ROOT}/vendor`), null);
});

test("changing the roots shows the repositories of the new roots only", async () => {
  const window = openRoot();
  const OTHER = "file:///other";
  await backend("one", {
    repositories: (root) => [root],
    status: (repository) => [{ file: `${repository}/x.txt`, status: "modified" }],
  });
  await settle();
  assert.equal(colorOf(`${ROOT}/x.txt`), "vcs.modified");

  await host.emit({ type: "workspaceChanged", workspace: host.setRoots(window, [OTHER]) });
  await settle();
  assert.equal(shown(`${ROOT}/x.txt`), null);
  assert.equal(colorOf(`${OTHER}/x.txt`), "vcs.modified");
});

test("a document outside every root decorates its own repository", async () => {
  openRoot();
  const OUTSIDE = "file:///elsewhere";
  const id = host.addDocument({ uri: `${OUTSIDE}/f.txt`, text: "x\n" });
  host.openWindow({ roots: [ROOT], documents: [id] });
  await running();
  const serving = await backendContract("one").serve({
    repositories: () => [],
    repositoryOf: (directory) => (directory.startsWith(OUTSIDE) ? OUTSIDE : null),
    baseline: () => null,
    status: () => [{ file: `${OUTSIDE}/f.txt`, status: "modified" }],
  });
  await (await vcs.client()).register("one");
  await settle();
  assert.equal(colorOf(`${OUTSIDE}/f.txt`), "vcs.modified");
  await serving[Symbol.asyncDispose]();
});

test("a backend that stops serving loses its decorations", async () => {
  openRoot();
  const first = await backend("first", {
    repositories: () => [ROOT],
    status: () => [{ file: `${ROOT}/a.txt`, status: "modified" }],
  });
  await settle();
  assert.equal(colorOf(`${ROOT}/a.txt`), "vcs.modified");

  await first[Symbol.asyncDispose]();
  await (await vcs.client()).changed({ backend: "first", repository: ROOT });
  await settle();
  assert.equal(shown(`${ROOT}/a.txt`), null);
  assert.equal(shown(ROOT), null);
});

test("a backend that unregisters loses its decorations", async () => {
  openRoot();
  await backend("one", { repositories: () => [ROOT], status: () => [{ file: `${ROOT}/a.txt`, status: "added" }] });
  await settle();
  assert.equal(colorOf(`${ROOT}/a.txt`), "vcs.added");
  await (await vcs.client()).unregister("one");
  await settle();
  assert.equal(shown(`${ROOT}/a.txt`), null);
});

test("a failing status call keeps what was shown", async () => {
  openRoot();
  let fail = false;
  await backend("one", {
    repositories: () => [ROOT],
    status: () => {
      if (fail) throw new Error("boom");
      return [{ file: `${ROOT}/a.txt`, status: "modified" }];
    },
  });
  await settle();
  fail = true;
  await (await vcs.client()).changed({ backend: "one", repository: ROOT });
  await settle();
  assert.equal(colorOf(`${ROOT}/a.txt`), "vcs.modified");
});

test("a burst of notifications makes at most two status pulls per repository", async () => {
  openRoot();
  let pulls = 0;
  let gate: Promise<void> = Promise.resolve();
  await backend("one", {
    repositories: () => [ROOT],
    status: async () => {
      pulls += 1;
      await gate;
      return [{ file: `${ROOT}/a.txt`, status: "modified" }];
    },
  });
  await settle();
  const before = pulls;
  // The first pull of the burst waits while the rest arrive.
  let release: () => void = () => {};
  gate = new Promise<void>((resolve) => (release = resolve));
  const client = await vcs.client();
  for (let index = 0; index < 10; index += 1) {
    await client.statusChanged({ backend: "one", repository: ROOT });
    await client.changed({ backend: "one", repository: ROOT });
  }
  release();
  await settle();
  assert.equal(pulls - before, 2);
});

test("statusChanged pulls status and leaves the baselines alone", async () => {
  openRoot();
  let baselines = 0;
  let entries: Entry[] = [];
  await running();
  const serving = await backendContract("one").serve({
    repositories: () => [ROOT],
    repositoryOf: () => ROOT,
    baseline: () => {
      baselines += 1;
      return null;
    },
    status: () => entries,
  });
  await (await vcs.client()).register("one");
  await settle();
  const before = baselines;
  entries = [{ file: `${ROOT}/a.txt`, status: "untracked" }];
  await (await vcs.client()).statusChanged({ backend: "one", repository: ROOT });
  await settle();
  assert.equal(colorOf(`${ROOT}/a.txt`), "vcs.untracked");
  assert.equal(baselines, before);
  await serving[Symbol.asyncDispose]();
});
