import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import * as host from "knot/testing";
import { COMMAND, start } from "../src/extension.js";

const ROOT = "file:///work/app";
let window: ReturnType<typeof host.openWindow>;

/** Opens the file in a new tab and reports it as active. */
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

async function labels(...queries: string[]) {
  host.query(...queries);
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(COMMAND);
  return host.pickerRows().map((rows) => rows.map((row) => row.id.replace(`${ROOT}/`, "")));
}

beforeEach(async () => {
  host.reset();
  for (const name of ["a.ts", "b.ts", "c.ts", "src/button.ts"]) host.seedFile(`${ROOT}/${name}`, name);
  host.seedFile("file:///elsewhere/a.ts", "outside");
  window = host.openWindow({ roots: [ROOT] });
  await start();
});

test("an empty query lists recent files first, without the current one", async () => {
  await activate(`${ROOT}/b.ts`);
  await activate(`${ROOT}/c.ts`);
  await activate(`${ROOT}/a.ts`);

  const [empty] = await labels();
  assert.deepEqual(empty?.slice(0, 2), ["c.ts", "b.ts"]);
  assert.equal(empty?.filter((id) => id === "a.ts").length, 1);
});

test("recent files matching the query lead, without duplicates", async () => {
  await activate(`${ROOT}/src/button.ts`);
  await activate(`${ROOT}/a.ts`);
  await activate(`${ROOT}/c.ts`);

  const [, matched] = await labels("ts");
  assert.deepEqual(matched?.slice(0, 2), ["a.ts", "src/button.ts"]);
  assert.equal(new Set(matched).size, matched?.length);
});

test("files outside the roots are not listed", async () => {
  await activate("file:///elsewhere/a.ts");
  await activate(`${ROOT}/b.ts`);
  await activate(`${ROOT}/c.ts`);

  const [empty] = await labels();
  assert.equal(empty?.includes("file:///elsewhere/a.ts"), false);
  assert.equal(empty?.[0], "b.ts");
});
