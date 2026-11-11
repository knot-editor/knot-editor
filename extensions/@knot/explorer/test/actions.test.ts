import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import type { WindowTarget } from "knot";
import * as host from "knot/testing";
import { DELETE, NEW_FILE, NEW_FOLDER, RENAME } from "../src/actions.js";
import { start } from "../src/extension.js";
import { VIEW_ID, parseRowId, rowId } from "../src/rows.js";
import { childUri, pathSegments, renameSelection } from "../src/names.js";

const ROOT = "file:///work/app";

let window: WindowTarget;
let rootRow: string;
let rootId: string;

async function rows(parent: string | null = null) {
  return host.treeItems(VIEW_ID, window, parent);
}

beforeEach(async () => {
  host.reset();
  host.seedFile(`${ROOT}/src/main.ts`, "main");
  host.seedFile(`${ROOT}/README.md`, "readme");
  window = host.openWindow({ roots: [ROOT] });
  await start();
  [rootRow] = (await rows()).map((item) => item.id) as [string];
  rootId = parseRowId(rootRow).root;
});

test("names split into path segments and refuse empty, absolute, and relative ones", () => {
  assert.deepEqual(pathSegments("a/b/c.txt"), { segments: ["a", "b", "c.txt"] });
  for (const bad of ["/a", "a//b", "a/", "a/../b", "."]) assert.ok("problem" in pathSegments(bad), bad);
  assert.equal(childUri("file:///d/", ["a b", "c"]), "file:///d/a%20b/c");
  assert.deepEqual(renameSelection("main.test.ts", false), [0, 9]);
  assert.deepEqual(renameSelection("main.ts", true), [0, 7]);
  assert.deepEqual(renameSelection(".gitignore", false), [0, 10]);
});

test("rows offer new items in folders and rename and delete below the roots", async () => {
  const [src, readme] = await rows(rootRow);
  const commands = (item: typeof src) => item?.actions?.map((action) => action.command);
  assert.deepEqual(commands((await rows())[0]), [NEW_FILE, NEW_FOLDER]);
  assert.deepEqual(commands(src), [NEW_FILE, NEW_FOLDER, RENAME, DELETE]);
  assert.deepEqual(commands(readme), [RENAME, DELETE]);
  assert.ok(src?.actions?.every((action) => action.menuOnly === true));
  assert.deepEqual(src?.actions?.[0]?.arguments, { id: src?.id });
});

test("New File validates names, creates missing folders, opens a tab, and reloads the parent", async () => {
  host.answer("editNewItem", { kind: "accepted", value: "docs/guide.md" });
  host.type("README.md", "docs/guide.md", "/x");
  await host.run(NEW_FILE, { arguments: { id: rootRow } });
  const [prompt] = host.prompts().filter((entry) => entry.kind === "editNewItem");
  assert.equal((prompt?.options as { id: string }).id, rootRow);
  assert.deepEqual(host.inputMessages(), [null, "README.md already exists here", null, "A name cannot start with a slash"]);
  assert.equal(await (await import("knot")).files.stat(`${ROOT}/docs/guide.md`) !== null, true);
  assert.deepEqual(host.opens().map((open) => open.uri), [`${ROOT}/docs/guide.md`]);
  assert.ok(host.view(VIEW_ID).invalidated.some((call) => call.parent === rootRow));
  assert.equal(host.view(VIEW_ID).reveals.at(-1)?.at(-1), rowId(rootId, `${ROOT}/docs/guide.md`));
});

test("New Folder creates a folder without opening anything, and a dismissed edit changes nothing", async () => {
  host.answer("editNewItem", { kind: "accepted", value: "assets" });
  await host.run(NEW_FOLDER, { arguments: { id: rootRow } });
  assert.equal((await (await import("knot")).files.stat(`${ROOT}/assets`))?.kind, "directory");
  assert.equal(host.opens().length, 0);

  host.answer("editNewItem", { kind: "dismissed" });
  await host.run(NEW_FILE, { arguments: { id: rootRow } });
  assert.equal(await (await import("knot")).files.stat(`${ROOT}/dismissed`), null);
});

test("Rename moves the entry and refuses names that exist", async () => {
  const readme = (await rows(rootRow)).find((item) => item.label === "README.md")!;
  host.answer("editLabel", { kind: "accepted", value: "NOTES.md" });
  host.type("src", "a/b", "");
  await host.run(RENAME, { arguments: { id: readme.id } });
  const [prompt] = host.prompts().filter((entry) => entry.kind === "editLabel");
  assert.deepEqual((prompt?.options as { selection: number[] }).selection, [0, 6]);
  assert.deepEqual(host.inputMessages(), [null, "src already exists here", "A name cannot contain a slash", "A name must be provided"]);
  const api = await import("knot");
  assert.equal(await api.files.stat(`${ROOT}/README.md`), null);
  assert.equal((await api.files.stat(`${ROOT}/NOTES.md`))?.kind, "file");
});

test("Delete asks first and moves the entry away only when confirmed", async () => {
  const readme = (await rows(rootRow)).find((item) => item.label === "README.md")!;
  const api = await import("knot");
  host.answer("showConfirmation", { kind: "dismissed" });
  await host.run(DELETE, { arguments: { id: readme.id } });
  assert.equal((await api.files.stat(`${ROOT}/README.md`))?.kind, "file");
  host.answer("showConfirmation", { kind: "confirmed" });
  await host.run(DELETE, { arguments: { id: readme.id } });
  assert.equal(await api.files.stat(`${ROOT}/README.md`), null);
  assert.ok(host.view(VIEW_ID).invalidated.some((call) => call.parent === rootRow));
});

test("a root is neither renamed nor deleted, and F2 and Delete are bound in the view", async () => {
  assert.equal(await host.run(RENAME, { arguments: { id: rootRow } }), "unavailable");
  assert.equal(await host.run(DELETE, { arguments: { id: rootRow } }), "unavailable");
  const bound = host.bindings().filter((binding) => binding.view === VIEW_ID).map((binding) => [binding.key, binding.command]);
  assert.deepEqual(bound.sort(), [
    ["cmd-backspace", DELETE],
    ["delete", DELETE],
    ["f2", RENAME],
  ]);
});
