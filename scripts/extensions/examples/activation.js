// API version 1 authoring example.
import { commands, documents, editor, workbench, keymaps, providers } from "knot";

/** @type {Set<import("knot").Disposable>} */
const bufferOwnership = new Set();
let active = true;

const command = await commands.register("guide.annotate", async (context) => {
  if (!active) return;
  context.signal.throwIfAborted();
  const buffer = await editor.activeBuffer(context.target);
  if (buffer === null || !active) return;
  const snapshot = await buffer.snapshot();
  if (!active) return;
  bufferOwnership.add(buffer.contributions);
  const endByteOffset = snapshot.byteOffsetAtUtf16(snapshot.text.length);
  await buffer.contributions.replace(endByteOffset === 0 ? [] : [{
    range: { startByteOffset: 0, endByteOffset },
    decoration: { color: "info", style: "wavy" },
    gutter: "info",
    command: "guide.annotate",
  }], { ifRevision: snapshot.revision });

  if (!active) return;
  const subscription = await buffer.onDidChange(async (event) => {
    // Explicit buffer access works even without a command context.
    await event.buffer.snapshot();
    // Cleanup is an ownership barrier, so self-disposal is safe.
    await subscription[Symbol.asyncDispose]();
    bufferOwnership.delete(subscription);
  });
  bufferOwnership.add(subscription);
  if (!active) await subscription[Symbol.asyncDispose]();
});

const child = await commands.register("guide.compose", async () => {
  const outcome = await commands.invoke("guide.annotate", { origin: "guide" });
  if (outcome.kind === "handlerFailure") throw new Error(outcome.message);
});

// Native and JavaScript children share captured routing and awaited outcomes.
const nativeChild = await commands.register("guide.select-all", async () => {
  const outcome = await commands.invoke("editor.select-all");
  if (outcome.kind !== "completed") {
    throw new Error(`Select All did not complete: ${outcome.kind}`);
  }
});

const tree = await workbench.registerView({
  id: "guide.outline",
  title: "Guide",
  content: {
    kind: "tree",
    provider: {
      getChildren(parentId) {
        return parentId === null ? [{
          id: "root", label: "Guide", collapsibleState: "none", kind: "item",
          description: null, icon: "symbol", command: "guide.annotate",
          statusText: "new", badge: "success",
          actions: [{ label: "Annotate", command: "guide.annotate", "arguments": { source: "row" } }],
        }] : [];
      },
    },
  },
  actions: [{ label: "Refresh", command: "guide.annotate", arguments: { source: "frame" } }],
});
await tree.invalidate();

const log = await workbench.registerView({
  id: "guide.log",
  title: "Guide log",
  content: { kind: "text" },
  placement: { location: "bottomLeft", visible: false },
});
await log.setText("guide: activated\n");
await log.setText("guide: ready\n");

const viewCommand = await commands.registerForView("guide.outline", "copy", async ({ target }) => {
  const selected = await workbench.selectedTreeItem(target);
  if (selected !== null) await workbench.writeClipboardText(selected);
});

/** @type {import("knot").CompletionProvider} */
const guideCompletion = {
  async provideCompletions(request) {
    const document = await documents.get(request.document);
    if (document === null || document.revision !== request.revision) return [];
    return [{ label: `${request.prefix}Guide`, insertText: `${request.prefix}Guide` }];
  },
};
const completion = await providers.register("editor.completion", guideCompletion);

// These mutations commit together at the end of this V8 turn.
keymaps.bind("cmd-g", { name: "guide.compose", arguments: { source: "key" } });
keymaps.bind("cmd-c", "copy", { view: "guide.outline" });
keymaps.bind("cmd-alt-g", null, { view: "editor" });
keymaps.unbind("cmd-alt-g", { view: "editor" });

// Called by an author-controlled feature toggle, not a promised unload hook.
export async function deactivateFeature() {
  active = false;
  keymaps.unbind("cmd-g");
  keymaps.unbind("cmd-c", { view: "guide.outline" });
  await Promise.all([
    ...Array.from(bufferOwnership, (owner) => owner[Symbol.asyncDispose]()),
    command[Symbol.asyncDispose](), child[Symbol.asyncDispose](), nativeChild[Symbol.asyncDispose](), viewCommand[Symbol.asyncDispose](),
    completion[Symbol.asyncDispose](), tree[Symbol.asyncDispose](), log[Symbol.asyncDispose](),
  ]);
}
