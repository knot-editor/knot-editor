// Word highlight: after the caret rests for a moment, highlights every
// whole-word occurrence of the word under it, or the other occurrences of a
// single-line selection, in the editor each window describes. Highlights are
// that editor's own, so other views of the buffer never show them.
import { settings, workbench } from "knot";
import type { Buffer, ByteRange, Disposable, EditorHandle, ViewTarget, WindowInfo, WindowTarget } from "knot";
import { SELECTION_COLOR, WORD_COLOR, decorations, selectionQuery, SELECTION_LIMIT } from "./highlight.js";
import { DELAY, SELECTION_ENABLED, WORD_ENABLED, defineSettings } from "./settings.js";

/** The editor one window describes and what it shows. */
type Follow = {
  target: ViewTarget | null;
  buffer: Buffer | null;
  handle: EditorHandle | null;
  disposables: Disposable[];
  timer: ReturnType<typeof setTimeout> | null;
  running: boolean;
  /** A change arrived while a run was in flight, joined into one more run. */
  pending: boolean;
  /** The color and text last painted, null when nothing is, and undefined
   * when an edit or a setting may have made it stale, so a caret that moves
   * within a word searches nothing. */
  shown: string | null | undefined;
};

/** What the caret or selection asks to highlight. */
type Wanted = {
  color: string;
  text: string;
  wholeWord: boolean;
  caseSensitive: boolean;
  skip: ByteRange | null;
};

let delay = 75;

function isGone(error: unknown) {
  const code = (error as { code?: string }).code;
  return code === "InvalidTarget" || code === "BufferClosed" || code === "Cancelled";
}

async function wanted(handle: EditorHandle, buffer: Buffer): Promise<Wanted | null> {
  const { anchor, caret } = await handle.selection();
  if (anchor === caret) {
    if (!(await settings.get(WORD_ENABLED))) return null;
    const word = await buffer.wordAt(caret);
    if (word === null || word.kind !== "word") return null;
    const { text } = await buffer.snapshot(word.range);
    return { color: WORD_COLOR, text, wholeWord: true, caseSensitive: true, skip: null };
  }
  if (!(await settings.get(SELECTION_ENABLED))) return null;
  const range = {
    startByteOffset: Math.min(anchor, caret),
    endByteOffset: Math.max(anchor, caret),
  };
  // A character takes at most four bytes.
  if (range.endByteOffset - range.startByteOffset > SELECTION_LIMIT * 4) return null;
  const text = selectionQuery((await buffer.snapshot(range)).text);
  // VS Code's default selected-text match mode follows Find: case-insensitive
  // and not whole-word.
  return text === null
    ? null
    : { color: SELECTION_COLOR, text, wholeWord: false, caseSensitive: false, skip: range };
}

/** Paints what the editor asks for, unless that is already painted. */
async function update(follow: Follow) {
  const { handle, buffer } = follow;
  if (handle === null || buffer === null) return;
  const want = await wanted(handle, buffer);
  const key = want === null ? null : `${want.color}\u0000${want.text}`;
  if (key === follow.shown) return;
  let items: ReturnType<typeof decorations> = [];
  let revision: number;
  if (want === null) {
    revision = (await buffer.snapshot({ startByteOffset: 0, endByteOffset: 0 })).revision;
  } else {
    const found = await buffer.find(want.text, {
      caseSensitive: want.caseSensitive,
      wholeWord: want.wholeWord,
    });
    items = decorations(found.matches, want.color, want.skip);
    revision = found.revision;
  }
  try {
    await handle.setDecorations(items, { ifRevision: revision });
    follow.shown = key;
  } catch (error) {
    // An edit raced the search; its own notification searches again.
    if ((error as { code?: string }).code !== "RevisionConflict") throw error;
  }
}

async function run(follow: Follow) {
  if (follow.running) {
    follow.pending = true;
    return;
  }
  follow.running = true;
  try {
    do {
      follow.pending = false;
      await update(follow);
    } while (follow.pending);
  } catch (error) {
    if (!isGone(error)) throw error;
  } finally {
    follow.running = false;
  }
}

/** Updates the highlights once the caret has rested for the delay: a change
 * while the timer waits restarts it. */
function schedule(follow: Follow) {
  if (follow.timer !== null) {
    follow.timer.refresh();
    return;
  }
  follow.timer = setTimeout(() => {
    follow.timer = null;
    return run(follow);
  }, delay);
}

async function release(follow: Follow) {
  if (follow.timer !== null) clearTimeout(follow.timer);
  follow.timer = null;
  for (const disposable of follow.disposables) await disposable.dispose();
  follow.disposables = [];
  const { handle, buffer } = follow;
  follow.handle = null;
  follow.buffer = null;
  follow.shown = null;
  if (handle === null) return;
  try {
    if (buffer !== null) {
      const { revision } = await buffer.snapshot({ startByteOffset: 0, endByteOffset: 0 });
      await handle.setDecorations([], { ifRevision: revision });
    }
  } catch (error) {
    if (!isGone(error) && (error as { code?: string }).code !== "RevisionConflict") throw error;
  }
  await handle.dispose();
}

export async function start() {
  await defineSettings();
  delay = (await settings.get(DELAY)) ?? 75;
  const follows = new Map<WindowTarget, Follow>();

  // Follows the window's status target, the editor its caret belongs to.
  const follow = async (info: WindowInfo) => {
    const window = info.target;
    const current: Follow = follows.get(window) ?? {
      target: null, buffer: null, handle: null, disposables: [], timer: null, running: false, pending: false, shown: null,
    };
    follows.set(window, current);
    if (current.target === info.statusTarget) return;
    await release(current);
    current.target = null;
    if (info.statusTarget !== null) {
      const tab = info.panes
        .flatMap((pane) => pane.tabs)
        .find((candidate) => candidate.target === info.statusTarget);
      if (tab?.buffer != null) {
        const buffer = tab.buffer;
        const handle = await workbench.editor(info.statusTarget);
        current.buffer = buffer;
        current.handle = handle;
        const changed = () => schedule(current);
        current.disposables.push(
          await handle.onDidChangeSelection(changed),
          await buffer.onDidChange(() => {
            current.shown = undefined;
            schedule(current);
          }),
        );
        schedule(current);
      }
    }
    current.target = info.statusTarget;
  };

  const observation = await workbench.observe(async (event) => {
    switch (event.type) {
      case "windowOpened":
      case "presentationsChanged":
        await follow(event.window);
        break;
      case "windowClosed": {
        const closed = follows.get(event.window);
        follows.delete(event.window);
        if (closed !== undefined) await release(closed);
        break;
      }
    }
  });
  for (const window of observation.snapshot.windows) await follow(window);

  // A setting change repaints at once: the delay applies to timers made after
  // it, and a toggled kind is shown or cleared.
  const changed = async () => {
    delay = (await settings.get(DELAY)) ?? 75;
    for (const current of follows.values()) {
      if (current.timer !== null) clearTimeout(current.timer);
      current.timer = null;
      current.shown = undefined;
      schedule(current);
    }
  };
  for (const key of [WORD_ENABLED, SELECTION_ENABLED, DELAY] as const) {
    await settings.observe(key, changed);
  }
}
