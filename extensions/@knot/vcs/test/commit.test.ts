import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { diffs } from "@knot/diff";
import * as host from "knot/testing";
import { backendContract, vcs } from "../src/contract.js";
import { start } from "../src/extension.js";
import type { Group } from "../src/status.js";

const ROOT = "file:///work";
const OTHER = "file:///other";

type Calls = { repository: string; message: string; options: string[] }[];

let started: Promise<unknown> | null = null;
const running = () => (started ??= start());
const settle = async () => {
  for (let turn = 0; turn < 10; turn += 1) await new Promise((resolve) => setImmediate(resolve));
};

beforeEach(async () => {
  host.reset();
  started = null;
  await diffs.serve({
    open: () => "diff",
    setText: () => {},
    keep: () => {},
    activate: () => {},
    close: () => {},
  });
});

type Fake = {
  calls: Calls;
  statuses: string[];
  /** What commit answers: a summary, null for a cancelled commit, or an error. */
  answer: { summary: string } | null | Error;
};

async function backend(id: string, roots: string[], fake: Fake) {
  await running();
  const groups: Group[] = [{ id: "staged", label: "Staged", files: [`${roots[0]}/a.txt`], actions: [] }];
  await backendContract(id).serve({
    repositories: (root) => (roots.includes(root) ? [root] : []),
    repositoryOf: () => null,
    baseline: () => null,
    status: (repository) => {
      fake.statuses.push(repository);
      return { files: [], groups: groups as never };
    },
    compare: () => null,
    rollback: () => {},
    commitOptions: () => [
      { id: "amend", label: "Amend" },
      { id: "signoff", label: "Sign-off", tooltip: "Add a Signed-off-by line" },
    ],
    commitMessage: ({ options }) => (options.includes("amend") ? "previous message" : null),
    commit: ({ repository, message, options }) => {
      fake.calls.push({ repository, message, options: [...options] });
      if (fake.answer instanceof Error) throw fake.answer;
      return fake.answer;
    },
    ref: () => null,
  });
  await (await vcs.client()).register(id);
  await settle();
}

const fake = (): Fake => ({ calls: [], statuses: [], answer: { summary: "abc1234: Subject" } });

type Layout = { children: any[] };
const layout = (): Layout => host.view("vcs.changes").updates.at(-1)!.layout as unknown as Layout;
const editor = () => layout().children.find((member) => member.type === "editor");
const controls = () => layout().children.find((member) => member.type === "controls").items as any[];
const checkbox = (label: string) => controls().find((item) => item.label === label);
const commitButton = () => checkbox("Commit");

async function type(text: string) {
  const buffer = editor().buffer;
  const { range, revision } = await buffer.snapshot();
  await buffer.applyEdits([{ range, text }], { ifRevision: revision });
  await settle();
}
const text = async () => (await editor().buffer.snapshot()).text;

async function toggle(label: string, window?: ReturnType<typeof host.openWindow>) {
  const item = checkbox(label);
  await host.run(item.command, { arguments: item.arguments, view: "vcs.changes", ...(window === undefined ? {} : { window }) });
  await settle();
}

async function commit() {
  assert.equal(await host.run(commitButton().command, { view: "vcs.changes" }), "completed");
  await settle();
}

test("the panel is a tree over a message editor and the backend's options, with Commit disabled while empty", async () => {
  host.openWindow({ roots: [ROOT] });
  await backend("one", [ROOT], fake());
  assert.equal(layout().children[0].type, "tree");
  assert.equal(editor().placeholder, "Commit message (⌘⏎ to commit)");
  assert.equal((editor() as { resize?: string }).resize, "commit-message");
  assert.deepEqual((editor() as { rulers?: number[] }).rulers, [72]);
  assert.deepEqual(controls().map((item) => [item.type, item.label, item.checked]), [
    ["checkbox", "Amend", false],
    ["checkbox", "Sign-off", false],
    ["button", "Commit", undefined],
  ]);
  assert.equal(checkbox("Sign-off").tooltip, "Add a Signed-off-by line");
  assert.equal(commitButton().disabled, true);
  await type("Fix it");
  assert.equal(commitButton().disabled, false);
  await type("");
  assert.equal(commitButton().disabled, true);
});

test("a window without a repository shows the tree alone", async () => {
  host.openWindow({ roots: [] });
  await running();
  await settle();
  assert.deepEqual(layout().children.map((member) => member.type), ["tree"]);
});

test("committing sends the message and options, clears the draft, refreshes the status, and says what was committed", async () => {
  host.openWindow({ roots: [ROOT] });
  const calls = fake();
  await backend("one", [ROOT], calls);
  await type("Fix it\n\nBecause.");
  await toggle("Sign-off");
  assert.equal(checkbox("Sign-off").checked, true);
  const before = calls.statuses.length;
  await commit();
  assert.deepEqual(calls.calls, [{ repository: ROOT, message: "Fix it\n\nBecause.", options: ["signoff"] }]);
  assert.equal(await text(), "");
  assert.equal(checkbox("Sign-off").checked, false);
  assert.equal(commitButton().disabled, true);
  assert.ok(calls.statuses.length > before);
  const notice = host.notices().at(-1)!;
  assert.equal(notice.message, "Committed abc1234: Subject");
  assert.deepEqual(notice.actions.map((action) => action.command), ["vcs.showOutput"]);
});

test("a failed commit keeps the draft and raises an error notice with a Show Output action", async () => {
  host.openWindow({ roots: [ROOT] });
  const calls = fake();
  calls.answer = new Error("hook rejected the message");
  await backend("one", [ROOT], calls);
  await type("Bad");
  await commit();
  assert.equal(await text(), "Bad");
  const notice = host.notices().at(-1)!;
  assert.equal(notice.severity, "error");
  assert.match(notice.message, /hook rejected the message/);
  assert.deepEqual(notice.actions.map((action) => action.command), ["vcs.showOutput"]);
});

test("a cancelled commit keeps the draft and raises no notice", async () => {
  host.openWindow({ roots: [ROOT] });
  const calls = fake();
  calls.answer = null;
  await backend("one", [ROOT], calls);
  await type("Keep me");
  await commit();
  assert.equal(await text(), "Keep me");
  assert.equal(host.notices().length, 0);
});

test("an empty message is refused before the backend, except when amending", async () => {
  host.openWindow({ roots: [ROOT] });
  const calls = fake();
  await backend("one", [ROOT], calls);
  await type("  \n");
  await commit();
  assert.equal(calls.calls.length, 0);
  assert.match(host.notices().at(-1)!.message, /commit message/i);

  await type("");
  await toggle("Amend");
  assert.equal(await text(), "previous message");
  await type("");
  assert.equal(commitButton().disabled, false);
  await commit();
  assert.deepEqual(calls.calls, [{ repository: ROOT, message: "", options: ["amend"] }]);
});

test("Amend swaps the draft for the suggested message and puts the user's draft back when it is off", async () => {
  host.openWindow({ roots: [ROOT] });
  await backend("one", [ROOT], fake());
  await type("my draft");
  await toggle("Amend");
  assert.equal(await text(), "previous message");
  assert.equal(checkbox("Amend").checked, true);
  await toggle("Sign-off");
  assert.equal(await text(), "previous message");
  await toggle("Amend");
  assert.equal(await text(), "my draft");
  assert.equal(checkbox("Amend").checked, false);
});

test("with several repositories the panel commits the one of the last chosen file, and names it", async () => {
  const window = host.openWindow({ roots: [ROOT, OTHER] });
  const calls = fake();
  await backend("one", [ROOT, OTHER], calls);
  assert.equal(editor().placeholder, "Commit message for work (⌘⏎ to commit)");
  await host.run("vcs.showFileChanges", {
    window,
    view: "vcs.changes.tree",
    arguments: { backend: "one", repository: OTHER, group: "staged", file: `${OTHER}/a.txt` },
  }).catch(() => {});
  await settle();
  assert.equal(editor().placeholder, "Commit message for other (⌘⏎ to commit)");
  await type("In other");
  await commit();
  assert.equal(calls.calls[0]!.repository, OTHER);
});

test("each repository keeps its own draft", async () => {
  const window = host.openWindow({ roots: [ROOT, OTHER] });
  await backend("one", [ROOT, OTHER], fake());
  await type("first");
  await host.run("vcs.showFileChanges", {
    window,
    view: "vcs.changes.tree",
    arguments: { backend: "one", repository: OTHER, group: "staged", file: `${OTHER}/a.txt` },
  }).catch(() => {});
  await settle();
  assert.equal(await text(), "");
  await type("second");
});

test("Cmd+Enter commits only from the Changes tool window", async () => {
  host.openWindow({ roots: [ROOT] });
  const calls = fake();
  await backend("one", [ROOT], calls);
  await type("Message");
  assert.equal(await host.run("vcs.commit.fromMessage"), "unavailable");
  assert.equal(calls.calls.length, 0);
  assert.equal(await host.run("vcs.commit.fromMessage", { view: "vcs.changes" }), "completed");
  assert.equal(calls.calls.length, 1);
});

test("Cmd+Alt+C and Cmd+Enter are bound, and the focus command shows the tool window", async () => {
  host.openWindow({ roots: [ROOT] });
  await backend("one", [ROOT], fake());
  const bound = host.bindings().map((binding) => [binding.key, binding.command]);
  assert.ok(bound.some(([key, command]) => /-alt-c$/.test(String(key)) && command === "vcs.focusCommitMessage"));
  assert.ok(bound.some(([key, command]) => /-enter$/.test(String(key)) && command === "vcs.commit.fromMessage"));
  assert.equal(host.view("vcs.changes").visible, true);
  assert.equal(await host.run("vcs.focusCommitMessage"), "completed");
  const [focused] = host.view("vcs.changes").focuses;
  assert.match(String(focused), /^message:/);
});

test("a commit records its message, and the history picker replaces the draft with an earlier one", async () => {
  host.openWindow({ roots: [ROOT] });
  await backend("one", [ROOT], fake());
  assert.equal(await host.run("vcs.commitMessageHistory"), "completed");
  assert.equal(host.notices().at(-1)!.message, "No earlier commit messages.");
  await type("First\n\nBody.\n");
  await commit();
  await type("Second");
  await commit();
  await type("draft");
  host.answer("showPicker", { kind: "selected", id: "First\n\nBody." });
  assert.equal(await host.run("vcs.commitMessageHistory"), "completed");
  const picker = host.prompts().at(-1)!.options as { items: { id: string; label: string }[] };
  assert.deepEqual(picker.items.map((item) => item.label), ["Second", "First"]);
  assert.equal(await text(), "First\n\nBody.");
  assert.equal(commitButton().disabled, false);
});

test("the history keeps 50 messages per repository, and Ctrl+M recalls only from the Changes tool window", async () => {
  host.openWindow({ roots: [ROOT, OTHER] });
  await backend("one", [ROOT], fake());
  await backend("two", [OTHER], fake());
  for (let index = 0; index < 55; index += 1) {
    await type(`message ${index}`);
    await commit();
  }
  assert.equal(await host.run("vcs.commitMessageHistory.fromMessage"), "unavailable");
  host.answer("showPicker", { kind: "dismissed" });
  assert.equal(await host.run("vcs.commitMessageHistory.fromMessage", { view: "vcs.changes" }), "completed");
  const picker = host.prompts().at(-1)!.options as { items: { label: string }[] };
  assert.equal(picker.items.length, 50);
  assert.equal(picker.items[0]!.label, "message 54");
  assert.ok(host.bindings().some((binding) => binding.key === "ctrl-m" && binding.command === "vcs.commitMessageHistory.fromMessage"));
});
