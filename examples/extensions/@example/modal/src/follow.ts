// Puts every editor the user activates into the `modal` context, using
// long-lived editor handles, and leaves it for an editor once its text is
// selected, so typing replaces the selection.
import { workbench } from "knot";
import type { Disposable, EditorHandle, ViewTarget } from "knot";

export async function follow(): Promise<Disposable> {
  const handles = new Map<ViewTarget, EditorHandle>();
  const attach = async (target: ViewTarget) => {
    if (handles.has(target)) return;
    let handle: EditorHandle;
    try {
      handle = await workbench.editor(target);
    } catch (error) {
      // Terminals and tools have no editor, and a tab may close meanwhile.
      if ((error as { code?: string }).code === "InvalidTarget") return;
      throw error;
    }
    handles.set(target, handle);
    // Drop per-editor state with the tab.
    handle.onDidClose(() => {
      handles.delete(target);
    });
    await handle.onDidChangeSelection(async () => {
      const { anchor, caret } = await handle.selection();
      if (anchor !== caret) await handle.setKeyContext("modal", false);
    });
    await handle.setKeyContext("modal", true);
  };
  const observation = await workbench.observe(async (event) => {
    if (event.type === "activeTargetChanged" && event.activeTarget !== null) {
      await attach(event.activeTarget);
    }
  });
  if (observation.snapshot.activeTarget !== null) await attach(observation.snapshot.activeTarget);
  return {
    async dispose() {
      await observation.dispose();
      await Promise.all([...handles.values()].map(async (handle) => {
        await handle.setKeyContext("modal", false);
        await handle.dispose();
      }));
      handles.clear();
    },
  };
}
