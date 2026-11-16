// The context menu contents: what each surface's right-click menu offers.
import { menus } from "knot";
import type { MenuItemOptions } from "knot";

/** The tab header menu: closing, then splitting. */
const TAB_ITEMS: readonly MenuItemOptions[] = [
  { command: "workbench.close-tab", group: "1_close", order: 0 },
  { command: "workbench.close-other-tabs", group: "1_close", order: 1 },
  { command: "workbench.close-tabs-to-right", group: "1_close", order: 2 },
  { command: "workbench.close-pane", title: "Close All", group: "1_close", order: 3 },
  { command: "workbench.split-horizontal", group: "2_split", order: 0 },
  { command: "workbench.split-vertical", group: "2_split", order: 1 },
];

/** The editor's menu: the clipboard commands. */
const EDITOR_ITEMS: readonly MenuItemOptions[] = [
  { command: "editor.cut", view: "editor", group: "1_clipboard", order: 0 },
  { command: "copy", view: "editor", group: "1_clipboard", order: 1 },
  { command: "editor.paste", view: "editor", group: "1_clipboard", order: 2 },
];

/** The terminal's menu. */
const TERMINAL_ITEMS: readonly MenuItemOptions[] = [
  { command: "copy", view: "terminal", group: "1_clipboard", order: 0 },
  { command: "editor.paste", view: "terminal", group: "1_clipboard", order: 1 },
];

export async function registerMenus() {
  for (const item of TAB_ITEMS) await menus.add("tab", item);
  for (const item of [...EDITOR_ITEMS, ...TERMINAL_ITEMS]) await menus.add("content", item);
}
