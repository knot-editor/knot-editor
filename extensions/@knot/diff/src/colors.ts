// The dimmer colors of staged changes: the gutter marks of a version control
// package and the bands of a diff view, each a faded form of its diff color.
import { themes } from "knot";

export async function defineStagedColors() {
  const colors = [
    { key: "diff.added.staged", description: "Staged added lines, in the gutter.", dark: "#5fb86566", light: "#36965066" },
    { key: "diff.modified.staged", description: "Staged changed lines, in the gutter.", dark: "#548af766", light: "#3574f066" },
    { key: "diff.removed.staged", description: "Staged removed lines, in the gutter.", dark: "#f7546466", light: "#db5c5c66" },
    { key: "diff.added.background.staged", description: "Staged added lines in a diff view, behind the text.", dark: "#4caf5018", light: "#4caf501c" },
    { key: "diff.modified.background.staged", description: "Staged changed lines in a diff view, behind the text.", dark: "#3574f016", light: "#3574f012" },
    { key: "diff.removed.background.staged", description: "Staged removed lines in a diff view, behind the text.", dark: "#f4433616", light: "#f4433612" },
  ];
  for (const { key, description, dark, light } of colors) {
    await themes.defineColor({ key, description, defaults: { dark, light } });
  }
}
