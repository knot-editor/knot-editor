import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { recovery } from "knot";
import * as host from "knot/testing";
import { VIEW_ID } from "../src/tree.js";
import { ALPHA, BETA, registry, seedProject, seedSession, session, startLaunched } from "./support.js";

beforeEach(() => host.reset());

test("creating a project records it and switches a blank window to it", async () => {
  const blank = host.openWindow({ documents: [host.addDocument()] });
  await startLaunched();
  host.answer("pickFolder", { kind: "selected", uri: ALPHA });
  host.answer("showInput", { kind: "accepted", value: "  Alpha " });
  await host.run("projects.create", { window: blank });

  const [project] = registry().projects;
  assert.equal(project?.name, "Alpha");
  assert.deepEqual(project?.roots, [ALPHA]);
  assert.equal(registry().recentProjectId, project?.id);
  assert.deepEqual(host.prompts().map((prompt) => prompt.kind), ["pickFolder", "showInput"]);
  assert.equal((host.prompts()[1]?.options as { value: string }).value, "alpha");
  const [window] = host.windowTargets();
  assert.notEqual(window, blank);
  assert.deepEqual(host.rootsOf(window!), [ALPHA]);
  assert.equal(session(project!.id)?.state, "active");
  const [row] = await host.treeItems(VIEW_ID, window!);
  assert.equal(row?.statusText, "active");
});

test("creation rejects a used folder, and the folder name or a blank leaves the project unnamed", async () => {
  seedProject("a", "Alpha", [ALPHA]);
  host.openWindow();
  await startLaunched();
  host.answer("pickFolder", { kind: "selected", uri: ALPHA });
  await host.run("projects.create");
  host.answer("pickFolder", { kind: "selected", uri: BETA });
  host.answer("showInput", { kind: "accepted", value: "beta" });
  await host.run("projects.create");
  host.answer("pickFolder", { kind: "selected", uri: "file:///src/gamma/" });
  host.answer("showInput", { kind: "accepted", value: "  " });
  await host.run("projects.create");
  assert.deepEqual(host.messages(), ["Alpha already uses this folder"]);
  assert.deepEqual(registry().projects.map((project) => project.name), ["Alpha", null, null]);
});

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

test("the picker lists display names with the roots' paths", async () => {
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
  const options = host.prompts()[0]?.options as { items: { label: string; description: string }[] };
  assert.deepEqual(options.items.map((item) => [item.label, item.description]), [
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

  host.setSelectedText("Alpha");
  await host.run("projects.select", { view: VIEW_ID });
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
