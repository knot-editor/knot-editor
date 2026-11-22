import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { workbench } from "knot";
import * as host from "knot/testing";

beforeEach(() => host.reset());

test("a notice is recorded with its defaults and observers see the history", async () => {
  const events: string[] = [];
  const observation = await workbench.observe((event) => {
    if (event.type === "notificationsChanged") events.push(event.notifications.map((n) => n.message).join(","));
  });
  const info = await workbench.showNotice({ message: "saved" });
  const warning = await workbench.showNotice({ message: "careful", severity: "warning", title: "Heads up" });
  assert.deepEqual(
    host.notices().map((notice) => [notice.id, notice.severity, notice.autoDismiss, notice.read]),
    [
      [info, "info", true, false],
      [warning, "warning", false, false],
    ],
  );
  assert.deepEqual(events, ["saved", "saved,careful"]);
  await observation[Symbol.asyncDispose]();
});

test("reading, dismissing, and clearing change the history", async () => {
  const first = await workbench.showNotice({ message: "one" });
  const second = await workbench.showNotice({ message: "two" });
  await workbench.markNotificationsRead([first]);
  assert.deepEqual(host.notices().map((notice) => notice.read), [true, false]);
  await workbench.dismissNotification(first);
  assert.deepEqual(host.messages(), ["two"]);
  await workbench.clearNotifications();
  assert.deepEqual(host.notices(), []);
  assert.equal(second > first, true);
});
