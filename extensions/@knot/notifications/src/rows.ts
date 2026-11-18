// The panel's rows and the stripe dot, as plain data from the notice history.
import type { Notice, NoticeSeverity, TreeItem } from "knot";

export const VIEW_ID = "builtin.notifications";
export const SHOW = "notifications.show";
export const TOGGLE = "notifications.toggle";
export const CLEAR = "notifications.clear";
export const MARK_ALL_READ = "notifications.mark-all-read";
export const ACTIVATE = "notifications.activate";
export const RUN_ACTION = "notifications.run-action";
export const DISMISS = "notifications.dismiss";

export const TOOL_ICON = `tool.${VIEW_ID}`;
export const EMPTY_ICON = "notifications.empty";
export const DISMISS_ICON = "notifications.dismiss";
export const MARK_READ_ICON = "notifications.mark-read";
export const CLEAR_ICON = "notifications.clear";

/** A longer message is cut off here; the popup shows it in full. */
const MESSAGE_LINES = 4;

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

type Group = "today" | "earlier";
const GROUPS: readonly { id: Group; label: string }[] = [
  { id: "today", label: "Today" },
  { id: "earlier", label: "Earlier" },
];

export const severityIcon = (severity: NoticeSeverity) => `notifications.${severity}`;
const severityColor = (severity: NoticeSeverity) => `notification.${severity}`;

const RANK: Record<NoticeSeverity, number> = { info: 0, warning: 1, error: 2 };

/** The time of a notice relative to `now`: minutes and hours within the day,
 * then the date. */
export function relativeTime(time: number, now: number): string {
  const age = Math.max(0, now - time);
  if (age < MINUTE) return "just now";
  if (age < HOUR) return `${Math.floor(age / MINUTE)}m ago`;
  if (groupOf(time, now) === "today") return `${Math.floor(age / HOUR)}h ago`;
  return new Date(time).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export function groupOf(time: number, now: number): Group {
  return new Date(time).toDateString() === new Date(now).toDateString() ? "today" : "earlier";
}

const flatten = (text: string) => text.replace(/\s+/g, " ").trim();

export function noticeRow(notice: Notice, now: number): TreeItem {
  const message = flatten(notice.message);
  const titled = notice.title !== null && flatten(notice.title) !== "";
  return {
    id: `notice:${notice.id}`,
    // The source names a notice that has no title of its own.
    label: titled ? flatten(notice.title!) : notice.source,
    bold: !notice.read,
    detail: message,
    detailMaxLines: MESSAGE_LINES,
    icon: severityIcon(notice.severity),
    collapsibleState: "none",
    command: ACTIVATE,
    arguments: { id: notice.id },
    statusText: relativeTime(notice.time, now),
    // One color for every unread dot; the icon already says the severity.
    badge: notice.read ? null : "text.accent",
    actions: [
      ...notice.actions.map((action, index) => ({
        label: action.title,
        command: RUN_ACTION,
        arguments: { id: notice.id, index },
        // The first action is a button on the row; the rest wait in the menu.
        menuOnly: index > 0,
      })),
      { label: "Dismiss", icon: DISMISS_ICON, command: DISMISS, arguments: { id: notice.id }, showOnHover: true },
    ],
  };
}

const EMPTY: TreeItem = {
  id: "empty",
  label: "No notifications",
  detail: "Notices from Knot and your packages collect here.",
  icon: EMPTY_ICON,
  collapsibleState: "none",
  muted: true,
};

const groupId = (group: Group) => `group:${group}`;

/** The tree's children of `parentId`: the groups that hold notices, newest
 * first inside each, or one friendly row when there are none. */
export function panelItems(notices: readonly Notice[], parentId: string | null, now: number): TreeItem[] {
  if (parentId === null) {
    if (notices.length === 0) return [EMPTY];
    return GROUPS.filter(({ id }) => notices.some((notice) => groupOf(notice.time, now) === id)).map(
      ({ id, label }) => ({ id: groupId(id), label, kind: "group", collapsibleState: "expanded" }),
    );
  }
  const group = GROUPS.find(({ id }) => groupId(id) === parentId);
  if (group === undefined) return [];
  return notices
    .filter((notice) => groupOf(notice.time, now) === group.id)
    .sort((a, b) => b.time - a.time || b.id - a.id)
    .map((notice) => noticeRow(notice, now));
}

export function unread(notices: readonly Notice[]): Notice[] {
  return notices.filter((notice) => !notice.read);
}

/** The dot on the stripe button: red while an unread notice is an error,
 * blue while any other is unread, none once all are read. */
export function stripeBadge(notices: readonly Notice[]): "info" | "error" | null {
  const pending = unread(notices);
  if (pending.length === 0) return null;
  return pending.some((notice) => notice.severity === "error") ? "error" : "info";
}
