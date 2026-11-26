// The explorer's entry in the tab header menu: it acts on the clicked tab's
// document, which it reads from the captured tab instead of the active file.
import { menus } from "knot";
import type { MenuItemOptions } from "knot";
import { REVEAL_ACTIVE } from "./rows.js";

/** Between the copied paths and the file manager entry of the tab menu. */
const TAB_ITEMS: readonly MenuItemOptions[] = [
  { command: REVEAL_ACTIVE, title: "Reveal in Explorer View", group: "3_file", order: 2 },
];

export async function registerMenus() {
  for (const item of TAB_ITEMS) await menus.add("tab", { ...item, view: "editor" });
}
