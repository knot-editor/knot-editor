import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import type { TreeItem, WindowTarget } from "knot";
import * as host from "knot/testing";
import { start } from "../src/extension.js";
import { ACTIVATE, OPEN_FOLDER, VIEW_ID, parseRowId, rowId } from "../src/rows.js";

const ROOT = "file:///work/app";
const OTHER = "file:///work/lib";

let window: WindowTarget;

async function children(parent: string | null = null, target: WindowTarget = window): Promise<TreeItem[]> {
  return host.treeItems(VIEW_ID, target, parent);
}

const labels = (items: readonly TreeItem[]) => items.map((item) => item.label);

beforeEach(async () => {
  host.reset();
  host.seedFile(`${ROOT}/src/main.ts`, "main");
  host.seedFile(`${ROOT}/src/a10.ts`, "a10");
  host.seedFile(`${ROOT}/src/a9.ts`, "a9");
  host.seedFile(`${ROOT}/src/B.ts`, "b");
  host.seedFile(`${ROOT}/docs/guide.md`, "guide");
  host.seedFile(`${ROOT}/README.md`, "readme");
  host.seedFile(`${OTHER}/lib.ts`, "lib");
  window = host.openWindow({ roots: [ROOT, OTHER] });
  await start();
});

test("every root of the window is a top-level row", async () => {
  const [app, lib] = await children();
  assert.deepEqual(labels([app!, lib!]), ["app", "lib"]);
  assert.equal(app?.collapsibleState, "expanded");
  assert.equal(app?.description, "/work/app");
  assert.deepEqual(app?.icon, { folder: "app" });
  assert.notEqual(app?.id, lib?.id);
});

test("a window without roots offers to open a folder", async () => {
  const bare = host.openWindow();
  const [row] = await children(null, bare);
  assert.equal(row?.label, "Open Folder…");
  assert.equal(row?.command, OPEN_FOLDER);
});

test("folders come first, then files, by name with numbers in natural order", async () => {
  const [app] = await children();
  assert.deepEqual(labels(await children(app!.id)), ["docs", "src", "README.md"]);
  const src = (await children(app!.id))[1]!;
  assert.deepEqual(labels(await children(src.id)), ["a9.ts", "a10.ts", "B.ts", "main.ts"]);
});

test("rows carry file and folder icons and the open command", async () => {
  const [app] = await children();
  const [docs, , readme] = await children(app!.id);
  assert.deepEqual(docs?.icon, { folder: "docs" });
  assert.equal(docs?.collapsibleState, "collapsed");
  assert.equal(docs?.command, undefined);
  assert.deepEqual(readme?.icon, { file: "README.md" });
  assert.equal(readme?.collapsibleState, "none");
  assert.equal(readme?.command, ACTIVATE);
  assert.deepEqual(readme?.arguments, { uri: `${ROOT}/README.md` });
  assert.equal(parseRowId(readme!.id).uri, `${ROOT}/README.md`);
});

test("ignored entries are dimmed, and the setting hides them", async () => {
  await (await import("knot")).settings.set("files.ignored", ["README.md"]);
  const [app] = await children();
  const readme = (await children(app!.id)).find((item) => item.label === "README.md");
  assert.equal(readme?.muted, true);
  await host.run("explorer.toggle-ignored");
  assert.ok(!labels(await children(app!.id)).includes("README.md"));
  assert.ok(host.view(VIEW_ID).invalidated.length > 0, "visible folders reload");
  await host.run("explorer.toggle-ignored");
  assert.ok(labels(await children(app!.id)).includes("README.md"));
});

test("each gesture opens its kind of tab", async () => {
  const uri = `${ROOT}/README.md`;
  const run = (gesture: string) =>
    host.run(ACTIVATE, { view: VIEW_ID, arguments: { gesture, arguments: { uri } } });
  for (const gesture of ["click", "doubleClick", "enter", "middleClick", "rightClick"]) {
    await run(gesture);
  }
  assert.deepEqual(host.opens().map((open) => open.options), [
    { target: host.windowTargets()[0], preview: true },
    { target: host.windowTargets()[0] },
    { target: host.windowTargets()[0] },
    { target: host.windowTargets()[0], focus: false },
  ]);
});

test("a change reloads only the loaded folders that contain it", async () => {
  const [app] = await children();
  const src = (await children(app!.id))[1]!;
  await children(src.id);
  const view = host.view(VIEW_ID);
  view.invalidated.length = 0;

  host.seedFile(`${ROOT}/src/new.ts`, "new");
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(view.invalidated.map((call) => call.parent), [src.id]);

  view.invalidated.length = 0;
  host.seedFile(`${ROOT}/docs/more.md`, "more");
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(view.invalidated.length, 0, "docs was never loaded");

  view.invalidated.length = 0;
  host.seedFile(`${ROOT}/top.txt`, "top");
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(view.invalidated.map((call) => call.parent), [app!.id]);
});

test("a batch naming the root reloads every loaded folder of it", async () => {
  const [app, lib] = await children();
  const src = (await children(app!.id))[1]!;
  await children(src.id);
  await children(lib!.id);
  const view = host.view(VIEW_ID);
  view.invalidated.length = 0;
  host.notifyWatchers(ROOT, [ROOT]);
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(view.invalidated.map((call) => call.parent).sort(), [app!.id, src.id].sort());
});

test("roots are watched once and unwatched when no window shows them", async () => {
  assert.deepEqual(host.watchedUris().sort(), [ROOT, OTHER]);
  host.setRoots(window, [ROOT]);
  await host.emit({ type: "workspaceChanged", workspace: { window, roots: [], generation: 1 } });
  assert.deepEqual(host.watchedUris(), [ROOT]);
  const second = host.openWindow({ roots: [ROOT] });
  await host.emit({ type: "windowOpened", window: { target: second } as never });
  assert.deepEqual(host.watchedUris(), [ROOT]);
});

test("a changed workspace reloads the root rows of that window", async () => {
  const view = host.view(VIEW_ID);
  view.invalidated.length = 0;
  host.setRoots(window, [OTHER]);
  await host.emit({ type: "workspaceChanged", workspace: { window, roots: [], generation: 1 } });
  assert.deepEqual(view.invalidated, [{ parent: null, window }]);
});

test("the active editor file is revealed when it changes, unless autoReveal is off", async () => {
  const id = host.addDocument({ uri: `${ROOT}/src/main.ts`, text: "main" });
  host.present(window, id);
  const target = host.tabTarget(window, 0);
  const view = host.view(VIEW_ID);
  await host.emit({ type: "activeTargetChanged", activeWindow: window, activeTarget: target });
  const [app] = await children();
  assert.deepEqual(view.reveals.at(-1), [
    app!.id,
    rowId(parseRowId(app!.id).root, `${ROOT}/src`),
    rowId(parseRowId(app!.id).root, `${ROOT}/src/main.ts`),
  ]);

  await (await import("knot")).settings.set("explorer.autoReveal", false);
  const before = view.reveals.length;
  await host.emit({ type: "activeTargetChanged", activeWindow: window, activeTarget: target });
  assert.equal(view.reveals.length, before);
});

test("Reveal Active File reveals the focused editor's file and focuses the explorer", async () => {
  const id = host.addDocument({ uri: `${OTHER}/lib.ts`, text: "lib" });
  host.present(window, id);
  await host.run("explorer.reveal-active-file");
  const [, lib] = await children();
  const root = parseRowId(lib!.id).root;
  assert.deepEqual(host.view(VIEW_ID).reveals.at(-1), [lib!.id, rowId(root, `${OTHER}/lib.ts`)]);
});

test("a file outside every root is not revealed", async () => {
  const id = host.addDocument({ uri: "file:///elsewhere/x.ts", text: "x" });
  host.present(window, id);
  await host.run("explorer.reveal-active-file");
  assert.equal(host.view(VIEW_ID).reveals.length, 0);
});

test("Toggle Explorer toggles the view", async () => {
  const view = host.view(VIEW_ID);
  const shown = view.visible;
  await host.run("explorer.toggle");
  assert.equal(view.visible, !shown);
});
