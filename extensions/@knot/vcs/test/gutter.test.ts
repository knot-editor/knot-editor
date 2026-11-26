import assert from "node:assert/strict";
import { afterEach, beforeEach, mock, test } from "node:test";
import { diffs } from "@knot/diff";
import { documents } from "knot";
import type { Buffer, DocumentId } from "knot";
import * as host from "knot/testing";
import { backendContract, vcs } from "../src/contract.js";
import { start } from "../src/extension.js";

let started: Promise<unknown> | null = null;
/** Starts the package once per test, before anything that needs `vcs`. */
const running = () => (started ??= start());

/** A diff opened through the `diff` contract, which these tests serve. */
type Shown = {
  options: { original: { label: string; text?: string; language?: string }; modified: { label: string; document?: string }; line?: number };
  /** The text of the original side now, after any `setText`. */
  original: string;
  closed: boolean;
};
let shown: Shown[] = [];

// Package tests get Node's own timers, which mock.timers drives.
beforeEach(async () => {
  host.reset();
  started = null;
  shown = [];
  await diffs.serve({
    open: (options) => {
      shown.push({ options, original: (options.original as { text: string }).text, closed: false });
      return `diff-${shown.length}`;
    },
    setText: ({ id, text }) => {
      const diff = shown[Number(id.slice(5)) - 1]!;
      if (diff.closed) throw new Error("closed");
      diff.original = text;
    },
    close: (id) => {
      shown[Number(id.slice(5)) - 1]!.closed = true;
    },
  });
  mock.timers.enable({ apis: ["setTimeout"] });
});
afterEach(() => mock.timers.reset());

const settle = async () => {
  for (let turn = 0; turn < 8; turn += 1) await new Promise((resolve) => setImmediate(resolve));
};

/** Lets the debounce elapse and the work behind it finish. */
async function rest() {
  await settle();
  mock.timers.tick(200);
  await settle();
}

type Answers = {
  /** Repositories covering a root. */
  repositories?: (root: string) => string[];
  /** The repository of a directory. */
  repositoryOf?: (directory: string) => string | null;
  baseline?: (repository: string, file: string) => string | null;
  ref?: (repository: string) => { label: string } | null;
};

/** Serves a backend `id` and registers it; returns what stops it. */
async function backend(id: string, answers: Answers) {
  await running();
  const serving = await backendContract(id).serve({
    repositories: (root) => (answers.repositories ?? (() => []))(root),
    repositoryOf: (directory) => (answers.repositoryOf ?? (() => null))(directory),
    baseline: ({ repository, file }) => (answers.baseline ?? (() => null))(repository, file),
    status: () => [],
    ref: (repository) => (answers.ref ?? (() => null))(repository),
  });
  await (await vcs.client()).register(id);
  return serving;
}

/** The marks as [start, end, color, style] of byte offsets. */
function marks(id: DocumentId) {
  return host.contributions(id).map((item) => [
    item.range.startByteOffset,
    item.range.endByteOffset,
    item.gutter?.color,
    item.gutter?.style,
  ]);
}

async function bufferOf(id: DocumentId): Promise<Buffer> {
  return (await documents.get(id))!.buffer;
}

const ROOT = "file:///work";
const FILE = `${ROOT}/a.txt`;

/** A document of FILE in a window rooted at ROOT. */
function open(text: string, uri = FILE, roots = [ROOT]) {
  const id = host.addDocument({ uri, text });
  host.openWindow({ roots, documents: [id] });
  return id;
}

test("added, modified, and removed lines are marked against the baseline", async () => {
  const id = open("a\nB\nc\ne\nf\n");
  await backend("one", { repositories: () => [ROOT], baseline: () => "a\nb\nc\nd\ne\n" });
  await running();
  await settle();
  assert.deepEqual(marks(id), [
    [2, 3, "diff.modified", "bar"],
    [6, 6, "diff.removed", "deletion"],
    [8, 9, "diff.added", "bar"],
  ]);
});

test("a document equal to its baseline shows nothing", async () => {
  const id = open("a\nb\n");
  await backend("one", { repositories: () => [ROOT], baseline: () => "a\nb\n" });
  await running();
  await settle();
  assert.deepEqual(marks(id), []);
});

test("typing updates the marks after the debounce", async () => {
  const id = open("a\nb\n");
  await backend("one", { repositories: () => [ROOT], baseline: () => "a\nb\n" });
  await running();
  await settle();
  const buffer = await bufferOf(id);
  await buffer.applyEdits([{ range: { startByteOffset: 0, endByteOffset: 1 }, text: "X" }], { ifRevision: 0 });
  await rest();
  assert.deepEqual(marks(id), [[0, 1, "diff.modified", "bar"]]);
  await buffer.applyEdits([{ range: { startByteOffset: 0, endByteOffset: 1 }, text: "a" }], { ifRevision: 1 });
  await rest();
  assert.deepEqual(marks(id), []);
});

test("a file without a baseline shows nothing, and loses its marks when the baseline goes", async () => {
  const id = open("a\nb\n");
  let baseline: string | null = null;
  await backend("one", { repositories: () => [ROOT], baseline: () => baseline });
  await running();
  await settle();
  assert.deepEqual(marks(id), []);

  baseline = "a\n";
  await (await vcs.client()).changed({ backend: "one", repository: ROOT });
  await settle();
  assert.deepEqual(marks(id), [[2, 3, "diff.added", "bar"]]);

  baseline = null;
  await (await vcs.client()).changed({ backend: "one", repository: ROOT });
  await settle();
  assert.deepEqual(marks(id), []);
});

test("a document outside every root is looked up from its own directory", async () => {
  const outside = "file:///elsewhere/b.txt";
  const id = open("x\n", outside);
  const asked: string[] = [];
  await backend("one", {
    repositories: () => [],
    repositoryOf: (directory) => {
      asked.push(directory);
      return "file:///elsewhere";
    },
    baseline: () => "",
  });
  await running();
  await settle();
  assert.deepEqual(marks(id), [[0, 1, "diff.added", "bar"]]);
  assert.deepEqual(asked, ["file:///elsewhere"]);
});

test("a document no repository contains is not looked up beyond its directory", async () => {
  const id = open("x\n", "file:///elsewhere/b.txt");
  await backend("one", { repositories: () => [ROOT], repositoryOf: () => null, baseline: () => "" });
  await running();
  await settle();
  assert.deepEqual(marks(id), []);
});

test("the deepest repository wins when two backends cover a file", async () => {
  const inner = `${ROOT}/inner`;
  const first = open("new\n", `${inner}/c.txt`);
  const second = host.addDocument({ uri: `${ROOT}/b.txt`, text: "new\n" });
  host.openWindow({ roots: [ROOT], documents: [second] });
  const baselines: string[] = [];
  await backend("outer", {
    repositories: () => [ROOT],
    baseline: (repository, file) => {
      baselines.push(`outer ${repository} ${file}`);
      return "";
    },
  });
  await backend("nested", {
    repositories: () => [inner],
    baseline: (repository, file) => {
      baselines.push(`nested ${repository} ${file}`);
      return "new\n";
    },
  });
  await running();
  await settle();
  // The nested backend owns the inner file, whose baseline equals its text;
  // the outer one owns the other, whose baseline is empty.
  assert.deepEqual(marks(first), []);
  assert.deepEqual(marks(second), [[0, 3, "diff.added", "bar"]]);
  // The inner file was first asked of the outer backend, which registered first.
  assert.equal(baselines.at(-1) === `outer ${ROOT} ${ROOT}/inner/c.txt`, false);
  assert.ok(baselines.includes(`nested ${inner} ${inner}/c.txt`));
  assert.ok(baselines.includes(`outer ${ROOT} ${ROOT}/b.txt`));
});

test("a repository nested below a root is found for files inside it", async () => {
  const inner = `${ROOT}/inner`;
  const id = open("x\n", `${inner}/c.txt`);
  const used: string[] = [];
  await backend("one", {
    repositories: () => [ROOT, inner],
    baseline: (repository) => {
      used.push(repository);
      return "";
    },
  });
  await running();
  await settle();
  assert.deepEqual(used, [inner]);
  assert.deepEqual(marks(id), [[0, 1, "diff.added", "bar"]]);
});

test("an edit that races the marks does not lose them", async () => {
  const id = open("a\nb\n");
  await backend("one", { repositories: () => [ROOT], baseline: () => "a\n" });
  const buffer = await bufferOf(id);
  // The first replacement is made stale by an edit, as typing would.
  const replace = buffer.contributions.replace.bind(buffer.contributions);
  let raced = false;
  buffer.contributions.replace = async (items, options) => {
    if (!raced) {
      raced = true;
      await buffer.applyEdits([{ range: { startByteOffset: 0, endByteOffset: 0 }, text: "z" }], { ifRevision: options.ifRevision });
    }
    return replace(items, options);
  };
  await running();
  await rest();
  assert.equal(raced, true);
  assert.deepEqual(marks(id), [[0, 4, "diff.modified", "bar"]]);
});

test("a backend that stops serving loses its marks, and another takes the file over", async () => {
  const id = open("a\nb\n");
  const first = await backend("first", { repositories: () => [ROOT], baseline: () => "a\n" });
  await running();
  await settle();
  assert.deepEqual(marks(id), [[2, 3, "diff.added", "bar"]]);

  await first[Symbol.asyncDispose]();
  await backend("second", { repositories: () => [ROOT], baseline: () => "a\nb\n" });
  await (await vcs.client()).changed({ backend: "first", repository: ROOT });
  await settle();
  assert.deepEqual(marks(id), []);
  assert.deepEqual(await (await vcs.client()).backends(), ["second"]);
});

test("a backend that stops with nothing else covering the file clears its marks", async () => {
  const id = open("a\nb\n");
  const only = await backend("only", { repositories: () => [ROOT], baseline: () => "a\n" });
  await running();
  await settle();
  assert.equal(marks(id).length, 1);

  await only[Symbol.asyncDispose]();
  await (await vcs.client()).changed({ backend: "only", repository: ROOT });
  await settle();
  assert.deepEqual(marks(id), []);
});

test("a backend registered later covers documents already open", async () => {
  const id = open("a\nb\n");
  await running();
  await settle();
  assert.deepEqual(marks(id), []);
  await backend("late", { repositories: () => [ROOT], baseline: () => "a\n" });
  await settle();
  assert.deepEqual(marks(id), [[2, 3, "diff.added", "bar"]]);
});

test("saving fetches the baseline again", async () => {
  const id = open("a\nb\n");
  host.document(id).dirty = true;
  let baseline = "a\n";
  await backend("one", { repositories: () => [ROOT], baseline: () => baseline });
  await running();
  await settle();
  assert.deepEqual(marks(id), [[2, 3, "diff.added", "bar"]]);

  baseline = "a\nb\n";
  host.document(id).dirty = false;
  await host.emit({ type: "documentChanged", document: (await documents.get(id))! });
  await settle();
  assert.deepEqual(marks(id), []);
});

test("a moved document is looked up again and its baseline fetched from the new repository", async () => {
  const id = open("a\nb\n");
  const other = "file:///other";
  await backend("one", {
    repositories: () => [ROOT, other],
    baseline: (repository) => (repository === ROOT ? "a\n" : "a\nb\n"),
  });
  await running();
  await settle();
  assert.deepEqual(marks(id), [[2, 3, "diff.added", "bar"]]);

  host.document(id).resourceUri = `${other}/a.txt`;
  await host.emit({ type: "documentChanged", document: (await documents.get(id))! });
  await settle();
  assert.deepEqual(marks(id), []);
});

test("closing a document releases its marks and stops following it", async () => {
  const id = open("a\nb\n");
  let fetched = 0;
  await backend("one", {
    repositories: () => [ROOT],
    baseline: () => {
      fetched += 1;
      return "a\n";
    },
  });
  await running();
  await settle();
  assert.equal(fetched, 1);
  assert.equal(marks(id).length, 1);

  await host.emit({ type: "documentClosed", document: id });
  await settle();
  assert.deepEqual(marks(id), []);
  await (await vcs.client()).changed({ backend: "one", repository: ROOT });
  await settle();
  assert.equal(fetched, 1);
});

test("an untitled document shows nothing", async () => {
  const id = host.addDocument({ text: "x\n" });
  host.openWindow({ roots: [ROOT], documents: [id] });
  await backend("one", { repositories: () => [ROOT], baseline: () => "" });
  await running();
  await settle();
  assert.deepEqual(marks(id), []);
});

test("Show Changes opens the baseline against the document at the caret line", async () => {
  const id = open("a\nB\nc\n");
  await backend("one", { repositories: () => [ROOT], baseline: () => "a\nb\nc\n", ref: () => ({ label: "main" }) });
  await rest();
  host.setEditorView({ anchor: 2, caret: 2, top: 0, scrollX: 0 });
  assert.equal(await host.run("vcs.showChanges"), "completed");
  const [diff] = shown;
  assert.deepEqual(diff!.options.original, { label: "main", text: "a\nb\nc\n", language: "plaintext" });
  assert.deepEqual(diff!.options.modified, { label: "Working Copy", document: id });
  assert.equal(diff!.options.line, 1);
});

test("Compare with Last Commit says so when nothing changed", async () => {
  open("a\nb\n");
  await backend("one", { repositories: () => [ROOT], baseline: () => "a\nb\n" });
  await rest();
  assert.equal(await host.run("vcs.showChanges"), "completed");
  assert.equal(shown.length, 0);
  assert.equal(host.notices().length, 1);
});

test("Show Changes is unavailable for a file without a baseline", async () => {
  open("a\n");
  await backend("one", { repositories: () => [ROOT], baseline: () => null });
  await rest();
  assert.equal(await host.run("vcs.showChanges"), "unavailable");
  assert.equal(shown.length, 0);
});

test("clicking a change mark opens the diff at that hunk, replacing an open one", async () => {
  const id = open("a\nb\nc\nD\n");
  await backend("one", { repositories: () => [ROOT], baseline: () => "a\nb\nc\nd\n" });
  await rest();
  await host.clickContribution(id, 0);
  await host.clickContribution(id, 0);
  const [first, second] = shown;
  assert.equal(first!.closed, true);
  assert.equal(second!.closed, false);
  assert.equal(second!.options.line, 3);
});

test("an open diff follows the baseline and closes when the baseline is gone", async () => {
  const id = open("a\nB\n");
  let baseline: string | null = "a\nb\n";
  await backend("one", { repositories: () => [ROOT], baseline: () => baseline });
  await rest();
  await host.run("vcs.showChanges");
  const [diff] = shown;
  baseline = "a\nx\n";
  await (await vcs.client()).changed({ backend: "one", repository: ROOT });
  await rest();
  assert.equal(diff!.original, "a\nx\n");
  assert.equal(diff!.closed, false);
  baseline = null;
  await (await vcs.client()).changed({ backend: "one", repository: ROOT });
  await rest();
  assert.equal(diff!.closed, true);
  void id;
});

test("an open diff closes with its document", async () => {
  const id = open("a\nB\n");
  await backend("one", { repositories: () => [ROOT], baseline: () => "a\nb\n" });
  await rest();
  await host.run("vcs.showChanges");
  await host.emit({ type: "documentClosed", document: id } as never);
  await settle();
  assert.equal(shown[0]!.closed, true);
});
