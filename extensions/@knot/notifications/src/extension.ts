// The notification history as a tool, and a bell in the status bar. The
// popups are native; this package mirrors the application's notices through
// the public observation and changes them through the workbench operations.
// Opening the panel marks every notice read.
import { commands, icons, workbench } from "knot";
import type { CommandContext, JsonValue, Notice, StatusItem, ViewRegistration } from "knot";
import {
  ACTIVATE, BELL_ICON, TOOL_ICON, BELL_UNREAD_ICON, CLEAR, CLEAR_ICON, DISMISS, DISMISS_ICON, EMPTY_ICON,
  MARK_ALL_READ, MARK_READ_ICON, RUN_ACTION, SHOW, TOGGLE, VIEW_ID, bellOptions, panelItems, severityIcon, unread,
} from "./rows.js";

const SEVERITIES = [
  ["info", "info"],
  ["warning", "circle-alert"],
  ["error", "circle-x"],
] as const;

function object(value: JsonValue): Record<string, JsonValue> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, JsonValue>)
    : null;
}

/** The notice ID a row command or action names. A row command nests its
 * arguments under the gesture that activated it. */
function noticeId(args: JsonValue): number | null {
  const direct = object(args);
  const nested = object(direct?.arguments ?? null);
  const id = (nested ?? direct)?.id;
  return typeof id === "number" ? id : null;
}

function gestureOf(args: JsonValue): string | null {
  const gesture = object(args)?.gesture;
  return typeof gesture === "string" ? gesture : null;
}

export async function start() {
  for (const [severity, lucide] of SEVERITIES) {
    await icons.define(severityIcon(severity), { lucide, color: `notification.${severity}` });
  }
  await icons.define(TOOL_ICON, { lucide: "bell" });
  await icons.define(BELL_ICON, { lucide: "bell" });
  await icons.define(BELL_UNREAD_ICON, { lucide: "bell-dot" });
  await icons.define(EMPTY_ICON, { lucide: "bell-off" });
  await icons.define(DISMISS_ICON, { lucide: "x" });
  await icons.define(MARK_READ_ICON, { lucide: "check-check" });
  await icons.define(CLEAR_ICON, { lucide: "trash" });

  let notices: readonly Notice[] = [];
  let view: ViewRegistration;
  let bell: StatusItem;

  const find = (id: number | null) => notices.find((notice) => notice.id === id);

  const sync = async (next: readonly Notice[]) => {
    notices = next;
    await bell.update(bellOptions(notices));
    await view.invalidate();
  };

  const markAllRead = async () => {
    const ids = unread(notices).map((notice) => notice.id);
    if (ids.length > 0) await workbench.markNotificationsRead(ids);
  };

  const runAction = async (id: number, index: number) => {
    const action = find(id)?.actions[index];
    if (action === undefined) return;
    await workbench.markNotificationsRead([id]);
    await commands.invoke(action.command, action.arguments);
  };

  view = await workbench.registerView({
    id: VIEW_ID,
    title: "Notifications",
    content: {
      kind: "tree",
      provider: { getChildren: (parentId) => panelItems(notices, parentId, Date.now()) },
    },
    placement: { location: "bottom", visible: false },
    actions: [
      { label: "Mark All Read", icon: MARK_READ_ICON, command: MARK_ALL_READ },
      { label: "Clear All", icon: CLEAR_ICON, command: CLEAR },
    ],
  });
  bell = await workbench.registerStatusItem("bell", bellOptions([]));

  const observation = await workbench.observe(async (event) => {
    if (event.type === "notificationsChanged") await sync(event.notifications);
  });
  await sync(observation.snapshot.notifications);

  const open = async ({ target }: CommandContext) => {
    await view.focus(target.windowTarget);
    await view.invalidate();
    await markAllRead();
  };
  await commands.register(SHOW, open, {
    title: "Show Notifications",
    description: "Open the notification history.",
  });
  await commands.register(TOGGLE, async ({ target }) => {
    await view.toggle(target.windowTarget);
    await markAllRead();
  }, { palette: false });
  await commands.register(CLEAR, () => workbench.clearNotifications(), {
    title: "Clear Notifications",
    description: "Remove every notice from the history.",
  });
  await commands.register(MARK_ALL_READ, markAllRead, {
    title: "Mark All Notifications Read",
    description: "Mark every notice in the history read.",
  });
  await commands.register(DISMISS, async ({ arguments: args }) => {
    const id = noticeId(args);
    if (id !== null) await workbench.dismissNotification(id);
  }, { palette: false });
  await commands.register(RUN_ACTION, async ({ arguments: args }) => {
    const id = noticeId(args);
    const index = object(args)?.index;
    if (id !== null && typeof index === "number") await runAction(id, index);
  }, { palette: false });

  // A row runs the notice's first action, or just reads a notice without one.
  const activate = async ({ arguments: args }: CommandContext) => {
    const gesture = gestureOf(args);
    if (gesture !== null && gesture !== "click" && gesture !== "enter") return;
    const id = noticeId(args);
    const notice = find(id);
    if (id === null || notice === undefined) return;
    if (notice.actions.length > 0) await runAction(id, 0);
    else await workbench.markNotificationsRead([id]);
  };
  await commands.register(ACTIVATE, activate, { palette: false });
  await commands.registerForView(VIEW_ID, ACTIVATE, activate);
}
