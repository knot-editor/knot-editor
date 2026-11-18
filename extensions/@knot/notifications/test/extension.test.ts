import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { workbench } from "knot";
import * as host from "knot/testing";
import { start } from "../src/extension.js";
import { VIEW_ID } from "../src/rows.js";

let window: ReturnType<typeof host.openWindow>;

beforeEach(async () => {
  host.reset();
  window = host.openWindow();
  await start();
});

const rows = async (parent: string | null = null) => host.treeItems(VIEW_ID, window, parent);
const bell = () => host.statusItems(window).find((item) => item.id === "bell");

test("the panel starts hidden at the bottom with an empty state and an idle bell", async () => {
  const view = host.view(VIEW_ID);
  assert.equal(view.visible, false);
  assert.equal(view.contribution.placement?.location, "rightTop");
  assert.deepEqual((await rows()).map((row) => row.label), ["No notifications"]);
  assert.deepEqual(view.contribution.actions?.map((action) => action.command), [
    "notifications.mark-all-read",
    "notifications.clear",
  ]);
  assert.equal(bell()?.text, "");
  assert.equal(bell()?.icon, "notifications.bell");
});

test("every icon the package names resolves", async () => {
  const icons = [
    ...(host.view(VIEW_ID).contribution.actions ?? []).map((action) => action.icon),
    bell()?.icon,
    "notifications.info", "notifications.warning", "notifications.error", "notifications.empty",
    "notifications.dismiss", "notifications.bell-unread", `tool.${VIEW_ID}`,
  ];
  for (const icon of icons) assert.notEqual(host.resolveIcon(icon as string), null, String(icon));
});

test("a raised notice appears as a row and counts on the bell", async () => {
  await workbench.showNotice({ title: "Explorer", message: "cannot rename", severity: "error" });
  assert.deepEqual((await rows()).map((row) => row.label), ["Today"]);
  assert.deepEqual((await rows("group:today")).map((row) => row.label), ["Explorer"]);
  assert.equal(bell()?.text, "1");
  assert.equal(bell()?.color, "notification.error");
  assert.ok(host.view(VIEW_ID).invalidations > 0);
});

test("notices raised before the package started are listed", async () => {
  host.reset();
  window = host.openWindow();
  await workbench.showNotice({ message: "early" });
  await start();
  assert.equal(bell()?.text, "1");
});

test("showing the panel opens it and marks every notice read", async () => {
  await workbench.showNotice({ message: "one" });
  await workbench.showNotice({ message: "two", severity: "warning" });
  assert.equal(await host.run("notifications.show", { window }), "completed");
  assert.equal(host.view(VIEW_ID).visible, true);
  assert.ok(host.notices().every((notice) => notice.read));
  assert.equal(bell()?.text, "");
});

test("the bell's command toggles the panel", async () => {
  assert.equal(bell()?.command?.name, "notifications.toggle");
  await host.run("notifications.toggle", { window });
  assert.equal(host.view(VIEW_ID).visible, true);
  await host.run("notifications.toggle", { window });
  assert.equal(host.view(VIEW_ID).visible, false);
});

test("Mark All Read, Dismiss, and Clear change the history", async () => {
  const first = await workbench.showNotice({ message: "one" });
  await workbench.showNotice({ message: "two" });
  await host.run("notifications.dismiss", { window, arguments: { id: first } });
  assert.deepEqual(host.notices().map((notice) => notice.message), ["two"]);
  await host.run("notifications.mark-all-read", { window });
  assert.equal(host.notices()[0]?.read, true);
  await host.run("notifications.clear", { window });
  assert.deepEqual(host.notices(), []);
  assert.deepEqual((await rows()).map((row) => row.label), ["No notifications"]);
});

test("activating a row runs the first action and reads the notice; one without actions is only read", async () => {
  const ran: unknown[] = [];
  const { commands } = await import("knot");
  await commands.register("test.go", ({ arguments: args }) => void ran.push(args));
  const withAction = await workbench.showNotice({
    message: "fix it",
    actions: [{ title: "Go", command: { name: "test.go", arguments: { where: "here" } } }],
  });
  const plain = await workbench.showNotice({ message: "fyi" });
  await host.run("notifications.activate", { window, arguments: { gesture: "click", arguments: { id: withAction } } });
  assert.deepEqual(ran, [{ where: "here" }]);
  assert.equal(host.notices().find((notice) => notice.id === withAction)?.read, true);
  await host.run("notifications.activate", { window, arguments: { gesture: "rightClick", arguments: { id: plain } } });
  assert.equal(host.notices().find((notice) => notice.id === plain)?.read, false);
  await host.run("notifications.activate", { window, arguments: { gesture: "enter", arguments: { id: plain } } });
  assert.equal(host.notices().find((notice) => notice.id === plain)?.read, true);
  assert.equal(ran.length, 1);
});

test("a menu action runs the chosen action of its notice", async () => {
  const ran: string[] = [];
  const { commands } = await import("knot");
  await commands.register("test.a", () => void ran.push("a"));
  await commands.register("test.b", () => void ran.push("b"));
  const id = await workbench.showNotice({
    message: "choose",
    actions: [
      { title: "A", command: { name: "test.a" } },
      { title: "B", command: { name: "test.b" } },
    ],
  });
  await host.run("notifications.run-action", { window, arguments: { id, index: 1 } });
  assert.deepEqual(ran, ["b"]);
});

test("the bell can be hidden through statusBar.hiddenItems", async () => {
  assert.ok(bell() !== undefined);
  const { settings } = await import("knot");
  await settings.set("statusBar.hiddenItems", ["bell"]);
  assert.equal(bell(), undefined);
});
