import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { diffs } from "@knot/diff";
import { settings } from "knot";
import type { TreeItem } from "knot";
import * as host from "knot/testing";
import { backendContract, vcs } from "../src/contract.js";
import { start } from "../src/extension.js";
import { folderChildren, idOf, listChildren, parseId } from "../src/rows.js";
import type { Group, GroupAction } from "../src/status.js";

const ROOT = "file:///work";
const REPOSITORY = { backend: "one", root: ROOT };
const VIEW = "vcs.changes";

type Side = { label: string; text: string };
type Compared = { original: Side; modified: "document" | Side } | null;
type Shown = { options: { original: Side; modified: { label: string; document?: string; text?: string } }; original: string; closed: boolean };

let started: Promise<unknown> | null = null;
const running = () => (started ??= start());
let shown: Shown[] = [];

beforeEach(async () => {
  host.reset();
  started = null;
  shown = [];
  await diffs.serve({
    open: (options) => {
      shown.push({ options: options as Shown["options"], original: (options.original as Side).text, closed: false });
      return `diff-${shown.length}`;
    },
    setText: ({ id, side, text }) => {
      const diff = shown[Number(id.slice(5)) - 1]!;
      if (diff.closed) throw new Error("closed");
      if (side === "original") diff.original = text;
    },
    close: (id) => {
      shown[Number(id.slice(5)) - 1]!.closed = true;
    },
  });
});

const settle = async () => {
  for (let turn = 0; turn < 8; turn += 1) await new Promise((resolve) => setImmediate(resolve));
};

type Answers = { groups: Group[]; compare?: (group: string, file: string) => Compared };

const STAGE: GroupAction = { label: "Stage", icon: "git.stage", command: "git.stage", on: ["file", "folder", "group"] as const, showOnHover: true };
const ROLLBACK: GroupAction = { label: "Rollback", command: "git.rollback", on: ["file"], menuOnly: true };

async function backend(id: string, answers: Answers, root = ROOT) {
  await running();
  const serving = await backendContract(id).serve({
    repositories: () => [root],
    repositoryOf: () => null,
    baseline: () => null,
    status: () => ({ files: [], groups: answers.groups as never }),
    compare: ({ group, file }) => (answers.compare ?? (() => null))(group, file),
    rollback: () => {},
    ref: () => null,
  });
  await (await vcs.client()).register(id);
  await settle();
  return serving;
}

const rootRows = (window: ReturnType<typeof host.openWindow>) => host.treeItems(VIEW, window);
const labels = (items: TreeItem[]) => items.map((item) => item.label);

const group = (id: string, label: string, files: string[], actions: GroupAction[] = []): Group => ({ id, label, files, actions });

test("a repository's non-empty groups are the top level, each with its file count", async () => {
  const window = host.openWindow({ roots: [ROOT] });
  await backend("one", {
    groups: [group("staged", "Staged", []), group("unstaged", "Unstaged", [`${ROOT}/a.txt`, `${ROOT}/src/b.txt`]), group("untracked", "Untracked", [`${ROOT}/c.txt`])],
  });
  const rows = await rootRows(window);
  assert.deepEqual(labels(rows), ["Unstaged", "Untracked"]);
  assert.deepEqual(rows.map((row) => row.statusText), ["2", "1"]);
  assert.deepEqual(parseId(rows[0]!.id), ["group", "one", ROOT, "unstaged"]);
});

test("a window without a repository shows one row, and a clean repository says so", async () => {
  const bare = host.openWindow({ roots: [] });
  await running();
  assert.deepEqual(labels(await rootRows(bare)), ["No repository"]);

  const window = host.openWindow({ roots: [ROOT] });
  await backend("one", { groups: [group("staged", "Staged", [])] });
  assert.deepEqual(labels(await rootRows(window)), ["No changes"]);
});

test("several repositories are rows named after their roots, with their groups under them", async () => {
  const window = host.openWindow({ roots: [ROOT, "file:///other"] });
  await backend("one", { groups: [group("unstaged", "Unstaged", [`${ROOT}/a.txt`])] });
  await backend("two", { groups: [group("staged", "Staged", [`file:///other/x.txt`])] }, "file:///other");
  const rows = await rootRows(window);
  assert.ok(rows.length >= 2);
  const first = rows[0]!;
  assert.equal(first.label, "work");
  assert.equal(first.collapsibleState, "expanded");
  assert.deepEqual(labels(await host.treeItems(VIEW, window, first.id)), ["Unstaged"]);
});

test("the tree layout nests files in folders, folders first, and lists a single entry folder without children", () => {
  const files = [`${ROOT}/z.txt`, `${ROOT}/src/b/deep.rs`, `${ROOT}/src/a.rs`, `${ROOT}/vendor/`];
  const unstaged = group("unstaged", "Unstaged", files);
  const top = folderChildren(REPOSITORY, unstaged, ROOT);
  assert.deepEqual(labels(top), ["src", "vendor", "z.txt"]);
  assert.equal(top[0]!.collapsibleState, "expanded");
  assert.equal(top[0]!.resource, `${ROOT}/src`);
  assert.equal(top[1]!.collapsibleState, "none");
  const src = folderChildren(REPOSITORY, unstaged, `${ROOT}/src`);
  assert.deepEqual(labels(src), ["b", "a.rs"]);
  assert.deepEqual(labels(folderChildren(REPOSITORY, unstaged, `${ROOT}/src/b`)), ["deep.rs"]);
});

test("the list layout is one ordered list, each file with its directory as detail", () => {
  const unstaged = group("unstaged", "Unstaged", [`${ROOT}/z.txt`, `${ROOT}/src/b%20c.rs`, `${ROOT}/src/a.rs`]);
  const rows = listChildren(REPOSITORY, unstaged);
  assert.deepEqual(labels(rows), ["a.rs", "b c.rs", "z.txt"]);
  assert.deepEqual(rows.map((row) => row.detail), ["src", "src", undefined]);
});

test("a file row carries its resource, ID, activation, and the actions of the backend then the menu", () => {
  const unstaged = group("unstaged", "Unstaged", [`${ROOT}/a.txt`], [STAGE, ROLLBACK, { label: "Stage All", command: "git.stage", on: ["group"] as const }]);
  const [file] = folderChildren(REPOSITORY, unstaged, ROOT);
  assert.equal(file!.id, idOf(["file", "one", ROOT, "unstaged", `${ROOT}/a.txt`]));
  assert.equal(file!.resource, `${ROOT}/a.txt`);
  assert.equal(file!.command, "vcs.showFileChanges");
  assert.deepEqual(file!.arguments, { backend: "one", repository: ROOT, group: "unstaged", file: `${ROOT}/a.txt` });
  assert.deepEqual(
    file!.actions!.map((action) => action.label),
    ["Stage", "Rollback", "Show Diff", "Open File", "Copy Path", "Copy Relative Path", "Copy File Name", file!.actions!.at(-1)!.label],
  );
  const stage = file!.actions![0]!;
  assert.deepEqual(stage.arguments, { repository: ROOT, group: "unstaged", path: `${ROOT}/a.txt` });
  assert.equal(stage.showOnHover, true);
  assert.equal(file!.actions![1]!.menuOnly, true);
  assert.equal(file!.actions!.at(-1)!.command, "file.reveal");
});

test("group and folder rows run the actions meant for them with the repository root or the folder as path", async () => {
  const window = host.openWindow({ roots: [ROOT] });
  const unstaged = group("unstaged", "Unstaged", [`${ROOT}/src/a.rs`], [{ ...STAGE, on: ["file", "folder"] }, { label: "Stage All", command: "git.stage", on: ["group"] as const }]);
  await backend("one", { groups: [unstaged] });
  const [header] = await rootRows(window);
  assert.deepEqual(header!.actions!.map((action) => action.label), ["Stage All"]);
  assert.deepEqual(header!.actions![0]!.arguments, { repository: ROOT, group: "unstaged", path: ROOT });
  const [folder] = await host.treeItems(VIEW, window, header!.id);
  assert.deepEqual(folder!.actions![0]!.arguments, { repository: ROOT, group: "unstaged", path: `${ROOT}/src` });
});

test("the layout setting switches the rows of a group, and the toggle command flips it", async () => {
  const window = host.openWindow({ roots: [ROOT] });
  await backend("one", { groups: [group("unstaged", "Unstaged", [`${ROOT}/src/a.rs`])] });
  const [header] = await rootRows(window);
  assert.deepEqual(labels(await host.treeItems(VIEW, window, header!.id)), ["src"]);
  assert.equal(await host.run("vcs.changes.toggleLayout"), "completed");
  assert.equal(await settings.get("vcs.changes.layout"), "list");
  assert.deepEqual(labels(await host.treeItems(VIEW, window, header!.id)), ["a.rs"]);
  await host.run("vcs.changes.toggleLayout");
  assert.equal(await settings.get("vcs.changes.layout"), "tree");
});

test("the view is invalidated when a snapshot arrives, and shows what the backend then reports", async () => {
  const window = host.openWindow({ roots: [ROOT] });
  const answers: Answers = { groups: [group("unstaged", "Unstaged", [`${ROOT}/a.txt`])] };
  await backend("one", answers);
  await rootRows(window);
  const before = host.view(VIEW).invalidated.length;
  answers.groups = [group("unstaged", "Unstaged", [`${ROOT}/a.txt`, `${ROOT}/b.txt`])];
  await (await vcs.client()).changed({ backend: "one", repository: ROOT });
  await settle();
  assert.ok(host.view(VIEW).invalidated.length > before);
  const [header] = await rootRows(window);
  assert.equal(header!.statusText, "2");
});

const showArguments = (groupId: string, file: string) => ({ backend: "one", repository: ROOT, group: groupId, file });
const FILE = `${ROOT}/a.txt`;

test("double click and Enter open a file's changes, and a click only selects", async () => {
  const window = host.openWindow({ roots: [ROOT] });
  host.addDocument({ uri: FILE, text: "new\n" });
  await backend("one", {
    groups: [group("staged", "Staged", [FILE])],
    compare: (groupId, file) => ({ original: { label: "HEAD", text: "old\n" }, modified: { label: "Index", text: "new\n" } }),
  });
  const gesture = (name: string) => ({ gesture: name, arguments: showArguments("staged", FILE) });
  await host.run("vcs.showFileChanges", { window, view: VIEW, arguments: gesture("click") });
  assert.equal(shown.length, 0);
  await host.run("vcs.showFileChanges", { window, view: VIEW, arguments: gesture("doubleClick") });
  assert.equal(shown.length, 1);
  assert.deepEqual(shown[0]!.options.original, { label: "HEAD", text: "old\n" });
  assert.deepEqual(shown[0]!.options.modified, { label: "Index", text: "new\n" });
});

test("an unstaged diff edits the working file's document", async () => {
  const window = host.openWindow({ roots: [ROOT] });
  const id = host.addDocument({ uri: FILE, text: "new\n" });
  await backend("one", {
    groups: [group("unstaged", "Unstaged", [FILE])],
    compare: () => ({ original: { label: "Index", text: "old\n" }, modified: "document" }),
  });
  await host.run("vcs.showFileChanges", { window, arguments: showArguments("unstaged", FILE) });
  assert.deepEqual(shown[0]!.options.modified, { label: "Working Copy", document: id });
});

test("a second request closes the open diff of the file and group, and another group keeps its own", async () => {
  const window = host.openWindow({ roots: [ROOT] });
  host.addDocument({ uri: FILE, text: "new\n" });
  await backend("one", {
    groups: [group("staged", "Staged", [FILE]), group("unstaged", "Unstaged", [FILE])],
    compare: (groupId) => ({ original: { label: groupId, text: "old\n" }, modified: { label: "Other", text: "new\n" } }),
  });
  await host.run("vcs.showFileChanges", { window, arguments: showArguments("staged", FILE) });
  await host.run("vcs.showFileChanges", { window, arguments: showArguments("unstaged", FILE) });
  assert.deepEqual(shown.map((diff) => diff.closed), [false, false]);
  await host.run("vcs.showFileChanges", { window, arguments: showArguments("staged", FILE) });
  assert.deepEqual(shown.map((diff) => diff.closed), [true, false, false]);
});

test("an open diff follows each new snapshot and closes when the file has no diff left", async () => {
  const window = host.openWindow({ roots: [ROOT] });
  host.addDocument({ uri: FILE, text: "new\n" });
  let text: string | null = "old\n";
  await backend("one", {
    groups: [group("staged", "Staged", [FILE])],
    compare: () => (text === null ? null : { original: { label: "HEAD", text }, modified: { label: "Index", text: "new\n" } }),
  });
  await host.run("vcs.showFileChanges", { window, arguments: showArguments("staged", FILE) });
  text = "older\n";
  await (await vcs.client()).changed({ backend: "one", repository: ROOT });
  await settle();
  assert.equal(shown[0]!.original, "older\n");
  assert.equal(shown[0]!.closed, false);
  text = null;
  await (await vcs.client()).changed({ backend: "one", repository: ROOT });
  await settle();
  assert.equal(shown[0]!.closed, true);
});

test("a file without a diff in its group says so and opens nothing", async () => {
  const window = host.openWindow({ roots: [ROOT] });
  await backend("one", { groups: [group("staged", "Staged", [FILE])] });
  await host.run("vcs.showFileChanges", { window, arguments: showArguments("staged", FILE) });
  assert.equal(shown.length, 0);
});
