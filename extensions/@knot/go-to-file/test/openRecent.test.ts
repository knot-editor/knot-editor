import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import type { PickerRow } from "knot";
import * as host from "knot/testing";
import { CLEAR_RECENT_COMMAND, OPEN_RECENT_COMMAND, start } from "../src/extension.js";

const ROOT = "file:///work/app";
let window: ReturnType<typeof host.openWindow>;

async function activate(uri: string) {
  const id = host.addDocument({ uri });
  host.present(window, id);
  const tabs = host.documentsIn(window).length;
  await host.emit({
    type: "activeTargetChanged",
    activeWindow: window,
    activeTarget: host.tabTarget(window, tabs - 1),
  });
}

async function rows() {
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(OPEN_RECENT_COMMAND);
  const shown = host.prompts().at(-1)?.options as { title: string; items?: readonly PickerRow[] };
  assert.equal(shown.title, "Open Recent");
  return shown.items ?? [];
}

beforeEach(async () => {
  host.reset();
  for (const name of ["a.ts", "b.ts", "src/c.ts"]) host.seedFile(`${ROOT}/${name}`, name);
  host.seedFile("file:///elsewhere/my%20dir/x.ts", "outside");
  window = host.openWindow({ roots: [ROOT] });
  await start();
});

test("lists every recent file newest first, including the current one and files outside the roots", async () => {
  await activate(`${ROOT}/a.ts`);
  await activate("file:///elsewhere/my%20dir/x.ts");
  await activate(`${ROOT}/src/c.ts`);

  const listed = await rows();
  assert.deepEqual(listed.map((row) => [row.label, row.description]), [
    ["c.ts", "src"],
    ["x.ts", "/elsewhere/my dir"],
    ["a.ts", null],
  ]);
});

test("several roots prefix the description with the root name", async () => {
  host.seedFile("file:///work/lib/util.ts", "util");
  host.setRoots(host.windowTargets()[0]!, [ROOT, "file:///work/lib"]);
  await activate(`${ROOT}/src/c.ts`);
  await activate("file:///work/lib/util.ts");

  assert.deepEqual((await rows()).map((row) => row.description), ["lib", "app/src"]);
});

test("choosing a row opens that file", async () => {
  await activate(`${ROOT}/a.ts`);
  host.answer("showPicker", { kind: "selected", id: `${ROOT}/a.ts` });
  const before = host.documentIds().length;
  await host.run(OPEN_RECENT_COMMAND);
  assert.ok(host.documentIds().some((id) => host.document(id).resourceUri === `${ROOT}/a.ts`));
  assert.ok(host.documentIds().length >= before);
});

test("a window without roots still lists", async () => {
  host.reset();
  window = host.openWindow();
  host.seedFile("file:///elsewhere/y.ts", "y");
  await start();
  await activate("file:///elsewhere/y.ts");

  assert.deepEqual((await rows()).map((row) => row.label), ["y.ts"]);
});

test("files that no longer exist are left out and forgotten", async () => {
  await activate(`${ROOT}/a.ts`);
  await activate("file:///elsewhere/my%20dir/x.ts");
  await activate(`${ROOT}/b.ts`);
  host.removeFile(`${ROOT}/a.ts`);
  host.removeFile("file:///elsewhere/my%20dir/x.ts");

  assert.deepEqual((await rows()).map((row) => row.label), ["b.ts"]);
  assert.deepEqual(host.records()["history/recent-files"], [`${ROOT}/b.ts`]);
});

test("an ignored file the listing hides stays when it exists", async () => {
  host.seedFile(`${ROOT}/.git/h.ts`, "h");
  await activate(`${ROOT}/.git/h.ts`);

  assert.deepEqual((await rows()).map((row) => row.label), ["h.ts"]);
});

test("a provider failure keeps the entry", async () => {
  await activate(`${ROOT}/a.ts`);
  host.removeFile(`${ROOT}/a.ts`);
  host.failNext("files.list", "ProviderFailure");
  host.failNext("files.stat", "ProviderFailure");

  assert.deepEqual((await rows()).map((row) => row.label), ["a.ts"]);
});

test("Clear Recent Files empties the list", async () => {
  await activate(`${ROOT}/a.ts`);
  await host.run(CLEAR_RECENT_COMMAND);

  assert.deepEqual(await rows(), []);
});
