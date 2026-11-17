// The explorer's entries in the tab header menu: file actions on the
// clicked tab's document, which read the captured tab instead of the
// active file.
import { menus } from "knot";
import type { MenuItemOptions } from "knot";
import { COPY_PATH, COPY_RELATIVE_PATH, REVEAL, REVEAL_ACTIVE } from "./rows.js";

/** Copying, then revealing, the clicked tab's document. */
const TAB_ITEMS: readonly MenuItemOptions[] = [
  { command: COPY_PATH, group: "3_file", order: 0 },
  { command: COPY_RELATIVE_PATH, group: "3_file", order: 1 },
  { command: REVEAL_ACTIVE, title: "Reveal in Explorer", group: "3_file", order: 2 },
  { command: REVEAL, group: "3_file", order: 3 },
];

export async function registerMenus() {
  for (const item of TAB_ITEMS) await menus.add("tab", { ...item, view: "editor" });
}
