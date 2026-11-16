import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { commands } from "knot";
import * as host from "knot/testing";
import { COMMAND, SEARCH_EVERYWHERE_COMMAND, SEARCH_ICON, start } from "../src/extension.js";

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
  assert.deepEqual(matched?.[0]?.icon, { file: "button.ts" });
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

test("a toolbar search button runs Go to File", async () => {
  const [window] = host.windowTargets();
  assert.deepEqual(
    host.toolbarItems(window!).map((item) => [item.id, item.text, item.icon, item.side, item.command?.name]),
    [["go-to-file", "", SEARCH_ICON, "right", COMMAND]],
  );
});

test("Search Everywhere asks for each tab and lists files and actions", async () => {
  await commands.register("demo.save", async () => {}, {
    title: "Save Everything",
    description: "Save all",
  });
  host.selectTab("files", "actions");
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(SEARCH_EVERYWHERE_COMMAND);

  const [shown] = host.prompts();
  const options = shown?.options as { title: string; tabs: { id: string }[] };
  assert.deepEqual(options.tabs.map((tab) => tab.id), ["all", "files", "actions"]);
  const [all, files, actions] = host.pickerRows();
  assert.equal(all?.length, 3, "an empty All tab lists files only");
  assert.deepEqual(files?.map((row) => row.label), all?.map((row) => row.label));
  assert.ok(actions?.some((row) => row.id === "command:demo.save" && row.label === "Save Everything"));
  assert.ok(actions?.every((row) => row.id.startsWith("command:")));
});

test("typing lists matching files, then actions, in the All tab", async () => {
  await commands.register("demo.main", async () => {}, { title: "Main Action" });
  host.query("main");
  host.answer("showPicker", { kind: "dismissed" });
  await host.run(SEARCH_EVERYWHERE_COMMAND);

  const typed = host.pickerRows()[1];
  assert.deepEqual(typed?.map((row) => row.label), ["main.ts", "Main Action"]);
  assert.deepEqual(typed?.[1]?.labelMatches, [[0, 4]]);
  assert.equal(typed?.[1]?.icon, undefined);
});

test("choosing an action runs it and choosing a file opens it", async () => {
  let ran = 0;
  await commands.register("demo.run", async () => { ran += 1; }, { title: "Run Me" });
  host.answer("showPicker", { kind: "selected", id: "command:demo.run" });
  await host.run(SEARCH_EVERYWHERE_COMMAND);
  assert.equal(ran, 1);

  host.answer("showPicker", { kind: "selected", id: `${ROOT}/README.md` });
  await host.run(SEARCH_EVERYWHERE_COMMAND);
  assert.deepEqual(
    host.documentIds().map((id) => host.document(id).resourceUri),
    [`${ROOT}/README.md`],
  );
});
