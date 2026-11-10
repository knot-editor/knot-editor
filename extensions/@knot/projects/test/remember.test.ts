import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { recovery, settings } from "knot";
import * as host from "knot/testing";
import { ALPHA, BETA, registry, seedSession, session, startLaunched } from "./support.js";

beforeEach(() => host.reset());

const GAMMA = "file:///src/gamma/";

function ids() {
  return registry().projects.map((project) => project.id);
}

test("a folder opened after launch becomes an unnamed active project bound to its window", async () => {
  const window = host.openWindow();
  await startLaunched();
  assert.equal(registry(), undefined);

  const workspace = host.setRoots(window, [ALPHA]);
  await host.emit({ type: "workspaceChanged", workspace });
  const [project] = registry().projects;
  assert.equal(project?.name, null);
  assert.deepEqual(project?.roots, [ALPHA]);
  assert.equal(registry().recentProjectId, project?.id);
  assert.equal(session(project!.id)?.state, "active");
  assert.equal(session(project!.id)?.windows.length, 1);

  // A second window on the same folder joins the project instead of making another.
  host.openWindow({ roots: [ALPHA] });
  await host.emit({ type: "workspaceChanged", workspace });
  assert.equal(registry().projects.length, 1);
  assert.equal(session(project!.id)?.windows.length, 2);
});

test("windows without roots are never remembered", async () => {
  const window = host.openWindow();
  await startLaunched();
  const cleared = host.setRoots(window, []);
  await host.emit({ type: "workspaceChanged", workspace: cleared });
  assert.equal(registry(), undefined);
});

test("launch remembers the folders of its windows once restored ones rebound", async () => {
  host.setRecord("projects", {
    schema: 2,
    projects: [
      { id: "a", name: "Alpha", roots: [ALPHA], createdAt: 1, lastOpenedAt: 1 },
      { id: "b", name: null, roots: [BETA], createdAt: 1, lastOpenedAt: 1 },
    ],
    recentProjectId: null,
  });
  seedSession("a", "active");
  seedSession("b", "suspended");
  host.openWindow({ roots: [ALPHA] });
  host.openWindow({ roots: [GAMMA] });
  host.openWindow({ roots: [GAMMA] });
  host.openWindow({ roots: [BETA] });
  host.openWindow();
  await startLaunched();

  assert.equal(registry().projects.length, 3);
  const created = registry().projects[2];
  assert.equal(created?.name, null);
  assert.deepEqual(created?.roots, [GAMMA]);
  assert.equal(session(created!.id)?.state, "active");
  assert.equal(session(created!.id)?.windows.length, 2);
  // Alpha's window rebound; Beta's suspended record is left to the switch.
  assert.equal(session("a")?.state, "active");
  assert.equal(session("b")?.state, "suspended");
});

test("pruning forgets the oldest unnamed projects beyond the limit, sparing protected ones", async () => {
  const draft = host.addDocument({ text: "kept" });
  const capture = (await recovery.retain(draft)).id;
  const project = (id: string, name: string | null, roots: string[], lastOpenedAt: number) => ({
    id,
    name,
    roots,
    createdAt: 1,
    lastOpenedAt,
  });
  host.setRecord("projects", {
    schema: 2,
    projects: [
      project("old", null, ["file:///src/old/"], 1),
      project("held", null, ["file:///src/held/"], 2),
      project("named", "Named", ["file:///src/named/"], 0),
      project("live", null, [ALPHA], 3),
      project("odd", null, ["file:///src/odd/"], 4),
      project("older", null, ["file:///src/older/"], 0),
    ],
    recentProjectId: null,
  });
  seedSession("old", "suspended");
  seedSession("older", "suspended");
  seedSession("held", "suspended", [{ entries: [{ document: draft, capture }] }]);
  seedSession("named", "suspended");
  seedSession("live", "active");
  host.setRecord("session:odd", { schema: 9, windows: [] });
  host.openWindow({ roots: [ALPHA] });
  const window = host.openWindow();
  await settings.set("projects.rememberedLimit", 2);
  await startLaunched();
  assert.equal(registry().projects.length, 6);

  await host.emit({ type: "workspaceChanged", workspace: host.setRoots(window, [BETA]) });
  const kept = ids();
  assert.equal(kept.includes("old") || kept.includes("older"), false);
  for (const id of ["held", "named", "live", "odd"]) assert.ok(kept.includes(id), id);
  assert.equal(kept.length, 5);
  assert.equal(session("old"), undefined);
  assert.equal(session("older"), undefined);
  assert.ok(session("held"));
  assert.deepEqual(host.captureIds(), [capture]);
});

test("the remembered limit defaults to thirty", async () => {
  await startLaunched();
  assert.equal(await settings.get("projects.rememberedLimit"), 30);
});
