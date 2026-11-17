import assert from "node:assert/strict";
import { test } from "node:test";
import type { Notice } from "knot";
import { bellOptions, groupOf, panelItems, relativeTime } from "../src/rows.js";

const NOW = new Date(2026, 10, 17, 15, 0, 0).getTime();
const minutes = (n: number) => n * 60_000;

function notice(id: number, over: Partial<Notice> = {}): Notice {
  return {
    id,
    message: `message ${id}`,
    title: null,
    severity: "info",
    autoDismiss: true,
    actions: [],
    target: null,
    source: "test",
    time: NOW - minutes(id),
    read: false,
    ...over,
  };
}

test("times read as minutes and hours today, then as a date", () => {
  assert.equal(relativeTime(NOW - 5_000, NOW), "just now");
  assert.equal(relativeTime(NOW - minutes(7), NOW), "7m ago");
  assert.equal(relativeTime(NOW - minutes(125), NOW), "2h ago");
  const yesterday = new Date(2026, 10, 16, 23, 0, 0).getTime();
  assert.equal(groupOf(yesterday, NOW), "earlier");
  assert.equal(relativeTime(yesterday, NOW), new Date(yesterday).toLocaleDateString(undefined, { month: "short", day: "numeric" }));
});

test("the panel groups notices into Today and Earlier, newest first", () => {
  const old = notice(9, { time: new Date(2026, 10, 10, 9, 0, 0).getTime() });
  const notices = [notice(1), notice(2), old];
  assert.deepEqual(panelItems(notices, null, NOW).map((item) => [item.id, item.label, item.kind]), [
    ["group:today", "Today", "group"],
    ["group:earlier", "Earlier", "group"],
  ]);
  assert.deepEqual(panelItems(notices, "group:today", NOW).map((item) => item.id), ["notice:1", "notice:2"]);
  assert.deepEqual(panelItems(notices, "group:earlier", NOW).map((item) => item.id), ["notice:9"]);
  assert.deepEqual(panelItems([notice(1)], null, NOW).map((item) => item.id), ["group:today"]);
});

test("an empty history shows one friendly row", () => {
  const [row, ...rest] = panelItems([], null, NOW);
  assert.equal(rest.length, 0);
  assert.equal(row?.label, "No notifications");
  assert.equal(row?.command, undefined);
});

test("a row shows the severity, the title over the message, the time, and the unread badge", () => {
  const titled = notice(1, { title: "Explorer", message: "cannot\nrename", severity: "warning" });
  const [row] = panelItems([titled], "group:today", NOW);
  assert.equal(row?.label, "Explorer");
  assert.equal(row?.detail, "cannot rename");
  assert.equal(row?.icon, "notifications.warning");
  assert.equal(row?.detailMaxLines, 4);
  assert.equal(row?.bold, true);
  assert.equal(row?.statusText, "1m ago");
  assert.equal(row?.badge, "text.accent");
  const [read] = panelItems([{ ...titled, read: true }], "group:today", NOW);
  assert.equal(read?.badge, null);
  assert.equal(read?.bold, false);
  const [untitled] = panelItems([notice(2)], "group:today", NOW);
  assert.equal(untitled?.label, "test");
  assert.equal(untitled?.detail, "message 2");
});

test("a row shows the first action as a button, Dismiss on hover, and the rest in its menu", () => {
  const [row] = panelItems(
    [notice(3, { actions: [{ title: "Open Log", command: "app.open-log", arguments: null }] })],
    "group:today",
    NOW,
  );
  assert.deepEqual(
    row?.actions?.map((action) => [action.label, action.command, action.arguments, action.menuOnly, action.showOnHover]),
    [
      ["Open Log", "notifications.run-action", { id: 3, index: 0 }, false, undefined],
      ["Dismiss", "notifications.dismiss", { id: 3 }, undefined, true],
    ],
  );
});

test("the bell counts unread notices in the color of the most severe", () => {
  assert.equal(bellOptions([]).text, "");
  assert.equal(bellOptions([]).color, undefined);
  const mixed = [notice(1), notice(2, { severity: "error" }), notice(3, { severity: "warning" }), notice(4, { severity: "error", read: true })];
  const bell = bellOptions(mixed);
  assert.equal(bell.text, "3");
  assert.equal(bell.color, "notification.error");
  assert.equal(bell.side, "right");
  assert.equal(bellOptions([notice(1, { read: true })]).text, "");
});
