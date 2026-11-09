// The icon pack the user chose with Select Icon Pack. Settings are not
// persisted, so the package keeps the pack name in its storage and writes the
// setting again at start, where later personal config still overrides it.
import { icons, settings, storage, workbench } from "knot";

export const RECORD_KEY = "iconPack";

/** The stored pack name; a missing or unreadable record means no pack. */
export async function restore(): Promise<string | undefined> {
  try {
    const result = await storage.read(RECORD_KEY);
    if (!result.found || typeof result.value !== "string") return undefined;
    await settings.set("icons.pack", result.value);
    return result.value;
  } catch {
    return undefined;
  }
}

/** Picks a registered icon pack, or none for the default icons. The
 * highlighted pack is previewed, and dismissing restores the setting from
 * before. */
export async function select(stored: string | undefined): Promise<string | undefined> {
  const before = (await settings.get("icons.pack")) ?? "";
  const names = await icons.listPacks();
  const picked = await workbench.showPicker({
    title: "Select icon pack",
    items: [
      { id: "", label: "Default icons", description: before === "" ? "selected" : "" },
      ...names.map((name) => ({
        id: name,
        label: name,
        description: before === name ? "selected" : "",
      })),
    ],
    onHighlight: async (id) => {
      if (id !== null) await settings.set("icons.pack", id);
    },
  });
  if (picked.kind !== "selected") {
    await settings.set("icons.pack", before);
    return stored;
  }
  await settings.set("icons.pack", picked.id);
  await storage.write(RECORD_KEY, picked.id);
  return picked.id;
}
