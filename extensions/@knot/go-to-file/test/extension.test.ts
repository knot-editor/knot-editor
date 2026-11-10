import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import * as host from "knot/testing";
import { COMMAND, start } from "../src/extension.js";

const ROOT = "file:///work/app";

beforeEach(async () => {
  host.reset();
  host.seedFile(`${ROOT}/src/main.ts`, "main");
  host.seedFile(`${ROOT}/src/ui/button.ts`, "button");
  host.seedFile(`${ROOT}/README.md`, "readme");
  host.openWindow({ roots: [ROOT] });
  await start();
});

test("each query lists the matching files with name and directory", async () => {
  host.query("butt");
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(COMMAND);

  const [shown] = host.prompts();
  assert.equal((shown?.options as { title: string }).title, "Go to File");
  const [all, matched] = host.pickerRows();
  assert.equal(all?.length, 3);
  assert.deepEqual(matched?.map((row) => [row.label, row.description]), [["button.ts", "src/ui"]]);
  assert.deepEqual(matched?.[0]?.labelMatches, [[0, 4]]);
});

test("choosing a row opens that file", async () => {
  host.answer("showPicker", { kind: "selected", id: `${ROOT}/src/main.ts` });
  await host.run(COMMAND);

  assert.deepEqual(
    host.documentIds().map((id) => host.document(id).resourceUri),
    [`${ROOT}/src/main.ts`],
  );
});

test("dismissing opens nothing", async () => {
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(COMMAND);

  assert.equal(host.documentIds().length, 0);
});

test("a window without roots lists nothing", async () => {
  host.reset();
  host.openWindow();
  await start();
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(COMMAND);

  assert.deepEqual(host.pickerRows(), [[]]);
});

test("several roots are ranked as one, each hit's root name starting its description", async () => {
  host.seedFile("file:///work/lib/util.ts", "util");
  host.seedFile("file:///work/lib/button.ts", "button");
  host.setRoots(host.windowTargets()[0]!, [ROOT, "file:///work/lib"]);
  host.query("butt");
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(COMMAND);

  const [all, matched] = host.pickerRows();
  assert.deepEqual(all?.map((row) => row.description), ["app", "app/src", "app/src/ui", "lib", "lib"]);
  assert.deepEqual(matched?.map((row) => [row.label, row.description]), [
    ["button.ts", "app/src/ui"],
    ["button.ts", "lib"],
  ]);
});
