import { settings } from "knot";

declare module "knot" {
  interface Settings {
    /** Select the active file in the explorer whenever it changes. */
    "explorer.autoReveal": boolean;
    /** Show entries that ignore rules match, dimmed. */
    "explorer.showIgnored": boolean;
  }
}

export const AUTO_REVEAL = "explorer.autoReveal";
export const SHOW_IGNORED = "explorer.showIgnored";

export async function defineSettings() {
  await settings.define({
    key: AUTO_REVEAL,
    type: "boolean",
    default: true,
    description: "Select the active file in the explorer whenever it changes.",
  });
  await settings.define({
    key: SHOW_IGNORED,
    type: "boolean",
    default: true,
    description: "Show files and folders that ignore rules match, dimmed; off hides them.",
  });
}
