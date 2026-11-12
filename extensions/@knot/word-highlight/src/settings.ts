import { settings } from "knot";

declare module "knot" {
  interface Settings {
    /** Highlight the occurrences of the word at the cursor. */
    "wordHighlight.enabled": boolean;
    /** Highlight the other occurrences of the selected text. */
    "selectionHighlight.enabled": boolean;
    /** Milliseconds the cursor rests before highlights update. */
    "wordHighlight.delay": number;
  }
}

export const WORD_ENABLED = "wordHighlight.enabled";
export const SELECTION_ENABLED = "selectionHighlight.enabled";
export const DELAY = "wordHighlight.delay";

export async function defineSettings() {
  await settings.define({
    key: WORD_ENABLED,
    type: "boolean",
    default: true,
    description: "Highlight the occurrences of the word at the cursor.",
  });
  await settings.define({
    key: SELECTION_ENABLED,
    type: "boolean",
    default: true,
    description: "Highlight the other occurrences of the selected text.",
  });
  await settings.define({
    key: DELAY,
    type: "integer",
    default: 75,
    description: "Milliseconds the cursor rests before the highlights update.",
  });
}
