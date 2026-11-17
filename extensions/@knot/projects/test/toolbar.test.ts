import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import * as host from "knot/testing";
import { SWITCH_COMMAND } from "../src/toolbar.js";
import { ALPHA, BETA, seedProject, seedSession, startLaunched } from "./support.js";

beforeEach(() => host.reset());

const widget = (window: ReturnType<typeof host.openWindow>) =>
  host.toolbarItems(window).map((item) => [item.text, item.command?.name]);

test("each window's widget shows its project's name and follows renames and opened projects", async () => {
  seedProject("a", "Alpha", [ALPHA]);
  seedProject("b", "Beta", [BETA]);
  seedSession("a", "active");
  seedSession("b", "suspended");
  const bound = host.openWindow({ roots: [ALPHA] });
  const unbound = host.openWindow();
  await startLaunched();

  assert.deepEqual(widget(bound), [["Alpha", SWITCH_COMMAND]]);
  assert.deepEqual(widget(unbound), []);

  host.answer("showInput", { kind: "accepted", value: "Renamed", highlighted: null });
  await host.run("projects.rename", { arguments: { id: "a" } });
  assert.deepEqual(widget(bound), [["Renamed", SWITCH_COMMAND]]);

  host.answer("showPicker", { kind: "selected", id: "b" });
  await host.run(SWITCH_COMMAND, { window: bound });
  // The window stays as it is and the project opens in a window of its own.
  const shown = host.windowTargets().flatMap((window) => host.toolbarItems(window).map((item) => item.text));
  assert.deepEqual(shown.filter((text) => text !== "").sort(), ["Beta", "Renamed"]);
});
