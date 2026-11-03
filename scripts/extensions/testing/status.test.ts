import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { settings, workbench } from "knot";
import * as host from "knot/testing";

beforeEach(() => host.reset());

const texts = (window: ReturnType<typeof host.openWindow>) =>
  host.statusItems(window).map((item) => item.text);

test("items sort by priority within a side and the latest registration of an id wins", async () => {
  const window = host.openWindow();
  await workbench.registerStatusItem("a", { text: "a" });
  await workbench.registerStatusItem("b", { text: "b", priority: 5 });
  const first = await workbench.registerStatusItem("c", { text: "first", side: "right" });
  const second = await workbench.registerStatusItem("c", { text: "second", side: "right" });
  assert.deepEqual(texts(window), ["b", "a", "second"]);
  await second.dispose();
  assert.deepEqual(texts(window), ["b", "a", "first"]);
  await first.dispose();
  assert.deepEqual(texts(window), ["b", "a"]);
});

test("a window's own value shadows the shared one until it is cleared", async () => {
  const one = host.openWindow();
  const two = host.openWindow();
  const item = await workbench.registerStatusItem("position", { text: "Ln 1" });
  await item.update({ text: "Ln 9" }, { window: one });
  await item.update({ text: "Ln 2" });
  assert.deepEqual([texts(one), texts(two)], [["Ln 9"], ["Ln 2"]]);
  await item.update(null, { window: one });
  assert.deepEqual(texts(one), ["Ln 2"]);
  await item.dispose();
  await assert.rejects(item.update({ text: "late" }), { code: "StatusItemRegistrationNotFound" });
});

test("hidden ids leave the bar without disposing the item", async () => {
  const window = host.openWindow();
  await workbench.registerStatusItem("position", { text: "Ln 1" });
  await settings.set("statusBar.hiddenItems", ["position"]);
  assert.deepEqual(texts(window), []);
  await settings.reset("statusBar.hiddenItems");
  assert.deepEqual(texts(window), ["Ln 1"]);
});
