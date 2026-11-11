import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { recovery } from "knot";
import * as host from "knot/testing";
import { VIEW_ID } from "../src/tree.js";
import { ALPHA, BETA, registry, seedProject, seedSession, session, startLaunched } from "./support.js";

beforeEach(() => host.reset());

test("names need not be unique, and renaming to blank restores the inferred name", async () => {
  seedProject("a", "Alpha", [ALPHA]);
  seedProject("b", "Beta", [BETA]);
  host.openWindow();
  await startLaunched();
  host.answer("showInput", { kind: "accepted", value: "beta" });
  await host.run("projects.rename", { arguments: { id: "a" } });
  assert.deepEqual(registry().projects.map((project) => project.name), ["beta", "Beta"]);
  host.answer("showInput", { kind: "accepted", value: "alpha" });
  await host.run("projects.rename", { arguments: { id: "a" } });
  assert.equal((host.prompts().at(-1)?.options as { value: string }).value, "beta");
  assert.equal(registry().projects[0]?.name, null);
  await host.run("projects.rename", { arguments: { id: "missing" } });
  assert.deepEqual(registry().projects.map((project) => project.name), [null, "Beta"]);
  assert.deepEqual(host.messages(), ["project not found"]);
});

test("the picker lists display names with the roots' paths below", async () => {
  seedProject("a", "Alpha", [ALPHA]);
  host.setRecord("projects", {
    ...(host.record("projects") as object),
    projects: [
      { id: "a", name: "Alpha", roots: [ALPHA], createdAt: 1, lastOpenedAt: 1 },
      { id: "u", name: null, roots: ["file:///x/app/", "file:///y/lib/"], createdAt: 1, lastOpenedAt: 1 },
    ],
  });
  host.openWindow();
  await startLaunched();
  host.answer("showPicker", { kind: "dismissed" });
  await host.run("projects.select");
  const options = host.prompts()[0]?.options as { items: { label: string; detail: string }[] };
  assert.deepEqual(options.items.map((item) => [item.label, item.detail]), [
    ["Alpha", "/src/alpha"],
    ["app +1", "/x/app, /y/lib"],
  ]);
});

test("removal warns about suspended text and releases it only after both deletes", async () => {
  seedProject("a", "Alpha", [ALPHA]);
  const draft = host.addDocument({ text: "unsaved" });
  host.openWindow({ documents: [draft] });
  const capture = (await recovery.retain(draft)).id;
  seedSession("a", "suspended", [{ entries: [{ document: draft, capture }] }]);
  await startLaunched();

  host.answer("showConfirmation", { kind: "confirmed" });
  host.failNext("storage.delete", "StorageUnavailable");
  await host.run("projects.remove", { arguments: { id: "a" } });
  const [confirmation] = host.prompts();
  assert.match(
    (confirmation?.options as { message: string }).message,
    /1 unsaved document in its suspended session will be discarded/,
  );
  assert.deepEqual(host.captureIds(), [capture]);
  assert.match(host.messages()[0]!, /cannot remove project: StorageUnavailable/);

  host.answer("showConfirmation", { kind: "confirmed" });
  seedProject("a", "Alpha", [ALPHA]);
  await host.run("projects.remove", { arguments: { id: "a" } });
  assert.deepEqual(registry().projects, []);
  assert.equal(session("a"), undefined);
  assert.deepEqual(host.captureIds(), []);
});

test("removing the active project leaves its window as it is", async () => {
  seedProject("a", "Alpha", [ALPHA]);
  seedSession("a", "active");
  const window = host.openWindow({ roots: [ALPHA] });
  await startLaunched();
  host.answer("showConfirmation", { kind: "confirmed" });
  await host.run("projects.remove", { arguments: { id: "a" } });
  const [confirmation] = host.prompts();
  assert.equal((confirmation?.options as { message: string }).message, "Open documents and folders stay open.");
  assert.deepEqual(registry().projects, []);
  assert.deepEqual(host.rootsOf(window), [ALPHA]);
});

test("a dismissed confirmation removes nothing", async () => {
  seedProject("a", "Alpha", [ALPHA]);
  host.openWindow();
  await startLaunched();
  host.answer("showConfirmation", { kind: "dismissed" });
  await host.run("projects.remove", { arguments: { id: "a" } });
  assert.equal(registry().projects.length, 1);
});

test("selecting from the picker or a tree row switches to the project", async () => {
  seedProject("a", "Alpha", [ALPHA]);
  seedProject("b", "Beta", [BETA]);
  host.openWindow();
  await startLaunched();

  host.answer("showPicker", { kind: "dismissed" });
  await host.run("projects.select");
  assert.equal(host.replaced().length, 0);

  host.answer("showPicker", { kind: "selected", id: "b" });
  await host.run("projects.select");
  assert.deepEqual(host.rootsOf(host.windowTargets()[0]!), [BETA]);

  host.setSelectedTreeItem("a");
  await host.run("projects.select", { view: VIEW_ID, arguments: { gesture: "rightClick", arguments: null } });
  assert.equal(host.replaced().length, 1);
  await host.run("projects.select", { view: VIEW_ID, arguments: { gesture: "click", arguments: null } });
  assert.equal(host.replaced().length, 2);
  assert.deepEqual(host.windowTargets().map(host.rootsOf), [[ALPHA]]);
});

test("a new project window opens with the project's roots", async () => {
  seedProject("a", "Alpha", [ALPHA]);
  seedSession("a", "active");
  const window = host.openWindow({ roots: [ALPHA] });
  const loose = host.openWindow();
  await startLaunched();
  await host.run("projects.new-window", { window: loose });
  assert.deepEqual(host.messages(), ["this window is not part of a project; select a project first"]);
  await host.run("projects.new-window", { window });
  assert.deepEqual(host.windowTargets().map(host.rootsOf), [[ALPHA], [], [ALPHA]]);
});
