// Bundled syntax highlighting languages. Every registration completes before
// start() resolves, because startup restores windows only after bundled
// packages start, so restored documents highlight from their first frame.
import { icons, languages, themes } from "knot";
import { FILE_ICONS, LANGUAGES } from "./languages.js";

export async function start() {
  await Promise.all([
    ...LANGUAGES.map((definition) => languages.register(definition)),
    ...Object.entries(FILE_ICONS).map(async ([id, { lucide, color }]) => {
      const key = `icon.${id}`;
      if (color !== undefined) {
        await themes.defineColor({ key, description: `The ${id} file icon`, defaults: color });
      }
      await icons.define(`file.${id}`, color === undefined ? { lucide } : { lucide, color: key });
    }),
  ]);
}
