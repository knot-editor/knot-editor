// Exhaustive API coverage: every group is called with the option shapes
// `knot.d.ts` declares. This is a test fixture rather than an authoring sample
// (see activation.js), run by the installed-package startup test, so a declared
// option the host rejects or ignores fails it.
import {
  commands, completion, documents, editor as editors, files, history, homeUri, icons, keymaps,
  languages, menus, platform, processes, recovery, settings, storage, themes, windows,
  workbench, workspaces,
} from "knot";
import { defineContract, s } from "knot/contract";

/** @typedef {import("knot").CommandContext} CommandContext */

/** @param {unknown} condition @param {string} message @returns {asserts condition} */
function check(condition, message) {
  if (!condition) throw new Error(`coverage: ${message}`);
}

/** @param {() => Promise<unknown>} run @param {string} code */
async function rejectsWith(run, code) {
  try {
    await run();
  } catch (error) {
    check(error instanceof Error && "code" in error && error.code === code,
      `expected ${code}, got ${String(error)}`);
    return;
  }
  throw new Error(`coverage: expected ${code}, but the call resolved`);
}

async function exerciseSettings() {
  await settings.define({
    key: "coverage.integer", type: "integer", default: 1, description: "A count.",
  });
  await settings.define({
    key: "coverage.strings", type: "strings", default: ["a"], description: "Names.",
  });
  await settings.set("coverage.integer", 2);
  await settings.set("coverage.integer", 3, { language: "plaintext" });
  await settings.set({ "coverage.integer": 4, "coverage.strings": ["b"] }, { view: "editor" });
  check(await settings.get("coverage.integer") === 2, "unscoped setting");
  check(await settings.get("coverage.integer", { language: "plaintext" }) === 3, "scoped setting");
  let notified = 0;
  const observation = await settings.observe("coverage.integer", () => { notified += 1; });
  await settings.set("coverage.integer", 5);
  await settings.reset("coverage.integer", { language: "plaintext" });
  await settings.reset("coverage.integer", { view: "editor" });
  await settings.reset("coverage.integer");
  check(await settings.get("coverage.integer") === 1, "reset reveals the default");
  await observation[Symbol.asyncDispose]();
  check(notified >= 1, "setting observer");
  await rejectsWith(() => settings.set("coverage.integer", /** @type {never} */ ("text")), "SettingTypeMismatch");
}

async function exerciseThemesAndIcons() {
  await themes.defineColor({
    key: "coverage.accent", description: "An accent.",
    defaults: { dark: "#112233", light: "#445566ff" },
  });
  const theme = await themes.register({
    name: "Coverage",
    appearance: "dark",
    colors: { "background": "#101010", "coverage.accent": "#ff0000" },
    chrome: { cardRadius: 4, cardGap: 2, cardInset: 2, cardBorder: 1, tabBarHeight: 30,
      stripeWidth: 30, accentWidth: 2, tabRadius: 3, tabInset: 2 },
    syntax: { keyword: { color: "#ff00ff", fontStyle: "italic", fontWeight: 700,
      underline: true, strikethrough: true } },
  });
  check((await themes.get("Coverage"))?.appearance === "dark", "registered theme");
  check((await themes.list()).some((info) => info.name === "Coverage"), "theme list");
  await theme[Symbol.asyncDispose]();

  await icons.define("coverage.mark", { lucide: "star", color: "coverage.accent" });
  const override = await icons.override("coverage.mark", { svg: "<svg viewBox='0 0 1 1'/>", color: "original" });
  const associations = await icons.associate({
    files: { "COVERAGE": "coverage.mark" },
    extensions: { cov: "coverage.mark" },
    folders: { coverage: "coverage.mark" },
  });
  const pack = await icons.registerPack({ name: "coverage", icons: { "coverage.mark": { lucide: "circle" } } });
  check((await icons.listPacks()).includes("coverage"), "icon pack list");
  await Promise.all([override[Symbol.asyncDispose](), associations[Symbol.asyncDispose](), pack[Symbol.asyncDispose]()]);
}

async function exerciseLanguages() {
  const grammars = await languages.grammars();
  check(grammars.length > 0, "grammars");
  const language = await languages.register({
    id: "coverage-lang", name: "Coverage", grammar: grammars[0],
    fileExtensions: ["cov"], fileNames: ["COVERAGE"], interpreters: ["covrun"],
    queries: { highlights: "" },
    comments: { line: ["//"], block: ["/*", "*/"] },
    brackets: [{ open: "(", close: ")", autoClose: true, newline: false, notIn: ["string", "comment"] }],
    autoCloseBefore: ";",
  });
  const association = await languages.associate("coverage-lang", {
    fileExtensions: ["cvg"], fileNames: ["COVERAGE2"], interpreters: ["covrun2"],
  });
  check((await languages.get("coverage-lang"))?.name === "Coverage", "language get");
  check((await languages.list()).some((info) => info.id === "coverage-lang"), "language list");
  await association[Symbol.asyncDispose]();
  await language[Symbol.asyncDispose]();
}

async function exerciseStorage() {
  await storage.write("coverage", { nested: [1, "two", null, true] });
  const read = await storage.read("coverage");
  check(read.found, "storage read");
  check((await storage.list()).includes("coverage"), "storage list");
  await storage.delete("coverage");
  await storage.delete("coverage");
  await history.record("coverage", "one");
  await history.record("coverage", "two");
  check((await history.recent("coverage"))[0] === "two", "history order");
  await history.forget("coverage", "two");
  await history.forget("coverage");
  check((await history.recent("coverage")).length === 0, "history cleared");
}

async function exerciseMenusAndKeymaps() {
  const command = await commands.register("coverage.command", async () => {});
  const item = await menus.add("content", {
    command: "coverage.command", title: "Coverage", arguments: { source: "menu" },
    group: "9_coverage", order: 1, view: "editor", context: "normal",
  });
  const bar = await menus.add("menubar.go", { command: "coverage.command" });
  check((await commands.list()).some((info) => info.name === "coverage.command"), "command list");
  keymaps.bind("cmd-alt-c", { name: "coverage.command", arguments: { source: "key" } },
    { view: "editor", context: "normal" });
  keymaps.bind("cmd-alt-v", "coverage.command", null);
  keymaps.bindText("coverage.command", { view: "editor" });
  keymaps.unbindText({ view: "editor" });
  keymaps.unbind("cmd-alt-c", { view: "editor", context: "normal" });
  keymaps.unbind("cmd-alt-v");
  const preset = await keymaps.registerPreset({
    name: "coverage", title: "Coverage",
    bindings: [
      { key: "cmd-alt-p", command: "coverage.command" },
      { key: "cmd-alt-o", command: { name: "coverage.command", arguments: 1 }, view: "editor" },
      { key: "cmd-alt-i", command: null },
    ],
  });
  check((await keymaps.getPreset("coverage"))?.bindings.length === 3, "preset get");
  check((await keymaps.listPresets()).some((info) => info.name === "coverage"), "preset list");
  await Promise.all([item[Symbol.asyncDispose](), bar[Symbol.asyncDispose](), preset[Symbol.asyncDispose](), command[Symbol.asyncDispose]()]);
}

async function exerciseFilesAndProcesses() {
  const directory = `${homeUri ?? "file:///tmp"}/knot-coverage-${Date.now()}`;
  check(platform === "macos" || platform === "windows" || platform === "linux", "platform");
  await files.createDirectory(directory);
  const file = await files.createFile(`${directory}/a.txt`);
  await files.writeText(file, "hello\n");
  check(await files.readText(file) === "hello\n", "file text");
  check((await files.stat(file))?.kind === "file", "stat");
  check((await files.list(directory)).length === 1, "list");
  const found = await files.search([directory], "a", { limit: 5 });
  check(found.results.length === 1, "search");
  check((await files.search(directory, "zzz")).results.length === 0, "search miss");
  check(await files.isIgnored(file) === false, "isIgnored");
  const watcher = await files.watch(directory, () => {}, { recursive: true });
  const copy = await files.copy(file, `${directory}/b.txt`);
  const moved = await files.rename(copy, `${directory}/c.txt`);
  await files.delete(moved);
  await watcher[Symbol.asyncDispose]();
  await files.delete(directory, { permanent: true });

  const result = await processes.run({
    command: "cat", args: [], ...(homeUri === null ? {} : { cwd: homeUri }), env: { COVERAGE: "1", HOME: null },
    stdin: "piped",
  });
  check(result.stdout === "piped" && result.exitCode === 0, "process run");
  const child = await processes.spawn({ command: "cat" });
  child.onStderr(() => {});
  const echoed = new Promise((resolve) => child.onStdout(resolve));
  await child.write("spawned");
  check(await echoed === "spawned", "process stdout");
  await child.closeStdin();
  check((await child.exited).exitCode === 0, "process exit");
  await rejectsWith(() => child[Symbol.asyncDispose](), "InvalidTarget"); // released on exit
  const killed = await processes.spawn({ command: "cat" });
  await killed.kill("SIGTERM");
  check((await killed.exited).signal !== null, "process signal");
  await killed[Symbol.asyncDispose]();
}

async function exerciseDocumentsAndWorkbench() {
  const created = await documents.createUntitled();
  const id = created.document.id;
  check((await documents.list()).some((info) => info.id === id), "document list");
  check((await documents.get(id))?.id === id, "document get");
  await documents.setLanguage(id, "coverage-explicit");
  await documents.setLanguage(id, null);
  check((await windows.list()).length > 0, "windows");
  const windowInfo = (await windows.list())[0];
  await documents.present(id, { target: windowInfo.target });
  check((await windows.get(windowInfo.target))?.target === windowInfo.target, "window get");
  await workspaces.get(windowInfo.target);
  const matches = await completion.match("ab", ["abc", "xyz", "a_b"], { styles: ["subsequence"] });
  check(matches.length >= 1, "completion match");

  const item = await workbench.registerStatusItem("coverage.status", {
    text: "Coverage", tooltip: "Tip", color: "info", side: "right",
    placement: "statusBar", priority: 3,
    command: { name: "coverage.status-command", arguments: { source: "status" } },
  });
  await item.update({ text: "Updated", side: "left", placement: "toolbar" });
  await item.update(null, { window: windowInfo.target });
  await item[Symbol.asyncDispose]();
  const noticeId = await workbench.showNotice({
    message: "Coverage", title: "Coverage", severity: "warning", autoDismiss: false,
    actions: [{ title: "Run", command: { name: "coverage.status-command" } }],
  });
  await workbench.markNotificationsRead([noticeId]);
  await workbench.dismissNotification(noticeId);
  await workbench.clearNotifications();
  await workbench.writeClipboardText("coverage");
  check(await workbench.readClipboardText() === "coverage", "clipboard");
}

/**
 * The buffer and editor of a command's tab, with every declared option.
 * @param {CommandContext} context
 */
export async function exerciseEditor(context) {
  const { buffer, editor } = context;
  check(buffer !== null && editor !== null, "a command in an editor");
  const first = await buffer.snapshot();
  await buffer.applyEdits([{ range: { startByteOffset: 0, endByteOffset: 0 }, text: "alpha beta\nalpha\n" }],
    { ifRevision: first.revision });
  const text = await buffer.snapshot(null);
  const whole = await buffer.snapshot({ startByteOffset: 0, endByteOffset: 5 });
  check(whole.text === "alpha" && text.text.length > 0, "snapshot range");
  check(text.utf16OffsetAtByte(text.byteOffsetAtUtf16(3)) === 3, "offset conversion");
  check(await buffer.lineCount() >= 2, "line count");
  const range = await buffer.lineRange(0);
  check(range.endByteOffset > range.startByteOffset, "line range");
  check((await buffer.lines(0, 1)).length === 2, "lines");
  check(await buffer.lineAt(12) === 1, "line at");
  check((await buffer.wordAt(1))?.kind === "word", "word at");
  const found = await buffer.find("ALPHA", {
    caseSensitive: false, wholeWord: true, regex: false, perLine: true,
    range: { startByteOffset: 0, endByteOffset: 17 }, limit: 10,
  });
  check(found.matches.length === 2, "find");
  check((await buffer.find("al.ha", { regex: true })).matches.length === 2, "regex find");
  const replaced = await buffer.replace("beta", "gamma", {
    caseSensitive: true, wholeWord: true, regex: false, perLine: true, limit: 1,
    ifRevision: found.revision,
  });
  check(replaced.count === 1, "replace");
  const mark = await buffer.createMark(2);
  check(await mark.position() === 2, "mark");
  await mark[Symbol.asyncDispose]();
  const subscription = await buffer.onDidChange(() => {});
  await subscription[Symbol.asyncDispose]();
  const current = await buffer.snapshot();
  await buffer.contributions.replace([
    { range: { startByteOffset: 0, endByteOffset: 5 }, decoration: { color: "info", style: "background" },
      gutter: { color: "warning", style: "dot" }, command: "coverage.command" },
    { range: { startByteOffset: 6, endByteOffset: 7 }, decoration: null, gutter: null, command: null },
  ], { ifRevision: current.revision });
  await buffer.contributions.replace([], { ifRevision: current.revision });

  const selection = await editor.selection();
  check(selection.caret >= 0, "selection");
  check((await editor.selection({ lines: 1 })).lines.length >= 1, "selection with lines");
  check((await editor.preparePaste("x", [0, 1])).length >= 0, "prepare paste");
  await editor.preparePaste("x");
  for (const reveal of /** @type {const} */ (["nearest", "center", "always-center", "top", "bottom"])) {
    await editor.setSelection({ anchor: 0, caret: 0 }, { reveal });
  }
  await editor.setSelection({ anchor: 0, caret: 2 });
  const viewport = await editor.viewport();
  await editor.rowAt(viewport.first);
  await editor.rowOf(0, 0);
  await editor.cellsBefore(0, 2);
  await editor.columnAtCells(0, 2);
  await editor.fold({ start: 0, end: 1 });
  await editor.folds();
  await editor.unfold({ start: 0, end: 1 });
  await editor.foldRegions();
  await editor.scrollBy(1);
  await editor.setKeyContext("coverage", true);
  await editor.setKeyContext("coverage", false);
  const decorated = await buffer.snapshot();
  await editor.setDecorations([{ range: { startByteOffset: 0, endByteOffset: 2 },
    decoration: { color: "warning", style: "wavy" } }], { ifRevision: decorated.revision });
  for (const style of /** @type {const} */ (["bar", "block", "underline", null])) {
    await editor.setCursorStyle(style);
  }
  await editor.openBar("coverage", {
    fields: [{ id: "query", placeholder: "Find", history: ["one", "two"] }],
    toggles: [{ id: "case", icon: "symbol", tooltip: "Case", active: true,
      command: { name: "coverage.command", arguments: { toggle: "case" } } }],
    buttons: [{ id: "next", icon: { file: "a.rs" }, tooltip: "Next",
      command: { name: "coverage.command" } }],
    message: "Searching",
  });
  await editor.openBar("coverage", { fields: [{ id: "query" }], message: null });
  await editor.focusBarField("coverage", "query", { text: "typed" });
  check((await editor.barFields("coverage")).query === "typed", "bar fields");
  await editor.focusBarField("coverage", "query");
  await editor.closeBar("coverage");
  const group = await editor.beginUndoGroup();
  await group[Symbol.asyncDispose]();
  const recording = await editor.record();
  await recording.stop();
  const state = await editor.viewState();
  await editor.restoreViewState(state);
  // An editor is neither a text frame nor a tree view.
  await rejectsWith(() => workbench.selectedText(context.target), "UnsupportedOperation");
  await rejectsWith(() => workbench.selectedTreeItem(context.target), "UnsupportedOperation");
  check((await editor.selection()).caret >= 0, "selection after the edits");
  const view = (await windows.list())[0]?.currentTarget ?? null;
  check(view !== null, "a focused tab");
  const handle = await workbench.editor(view);
  const closed = handle.onDidClose(() => {});
  const barListener = await handle.onDidEditBar(() => {});
  const selectionListener = await handle.onDidChangeSelection(() => {});
  const linesListener = await handle.onDidChangeSelection(() => {}, { lines: 2 });
  await Promise.all([closed[Symbol.asyncDispose](), barListener[Symbol.asyncDispose](), selectionListener[Symbol.asyncDispose](),
    linesListener[Symbol.asyncDispose]()]);
  await handle[Symbol.asyncDispose]();
  check(await editorActiveBuffer(context) !== null, "active buffer");
}

/** @param {CommandContext} context */
function editorActiveBuffer(context) {
  return editors.activeBuffer(context.target);
}

async function exerciseViewsAndDocuments() {
  const window = (await windows.list())[0];
  check(window !== undefined, "an application window");
  const target = window.target;
  const tree = await workbench.registerView({
    id: "coverage.tree", title: "Coverage",
    content: { kind: "tree", provider: { getChildren: (parentId) => parentId === null ? [{
      id: "row", label: "Row", description: "d", detail: "detail", detailMaxLines: 2,
      icon: { folder: "src" }, collapsibleState: "collapsed", command: "coverage.command",
      arguments: { row: 1 }, kind: "item", muted: true, bold: true, statusText: "s",
      badge: "warning", actions: [{ label: "A", icon: "action.refresh", command: "coverage.command",
        arguments: null, menuOnly: false, showOnHover: true }],
      draggable: true, dropTarget: true, resource: `${homeUri ?? "file:///tmp"}/x`,
    }, { id: "group", label: "Group", collapsibleState: "expanded", kind: "group",
      description: null, detail: null, detailMaxLines: null, icon: null, command: null }] : [] } },
    drop: "coverage.command",
    actions: [{ label: "Refresh", icon: "action.refresh", command: "coverage.command" }],
    placement: { location: "rightBottom", order: 5, visible: true },
  });
  await tree.show(); await tree.show(target); await tree.hide(target); await tree.toggle(); await tree.toggle(target);
  await tree.focus(); await tree.focus(target);
  await tree.setBadge("info"); await tree.setBadge("error"); await tree.setBadge(null);
  await tree.invalidate(); await tree.invalidate(null); await tree.invalidate("row", target);
  await tree.reveal(["row"]); await tree.reveal(["row"], target);
  const container = await workbench.registerView({
    id: "coverage.container", title: "Container", newTab: "coverage.command",
    placement: { location: "bottomLeft", order: 1, visible: false },
  });
  const text = await workbench.registerView({
    id: "coverage.text", title: "Text", content: { kind: "text" }, language: "plaintext", gutter: true,
    placement: { location: "bottomRight" },
  });
  await text.setText("one"); await text.setText("two", target);
  const viewCommand = await commands.registerForView("coverage.tree", "copy", async () => {});
  const options = await commands.register("coverage.options", async () => {},
    { title: "Options", description: null, palette: false });
  const described = await commands.register("coverage.described", async () => {},
    { description: "Described" });
  check((await commands.list()).length >= 0, "view commands");
  const outcome = await commands.invoke("coverage.options", { value: [1, "a", null] });
  check(outcome.kind === "completed", "invoke");
  await Promise.all([viewCommand[Symbol.asyncDispose](), options[Symbol.asyncDispose](), described[Symbol.asyncDispose](),
    tree[Symbol.asyncDispose](), text[Symbol.asyncDispose](), container[Symbol.asyncDispose]()]);

  const events = [];
  const observation = await workbench.observe((event) => { events.push(event); });
  check(observation.watermark >= 0, "observation");
  const layout = await workbench.captureLayout(target);
  check(layout !== null, "layout");
  await workbench.lastFocusedDocument(target);
  await observation[Symbol.asyncDispose]();
  check((await recovery.list()).length >= 0, "recovery");

  const directory = `${homeUri ?? "file:///tmp"}/knot-coverage-docs-${Date.now()}`;
  await files.createDirectory(directory);
  const path = await files.writeText(`${directory}/doc.txt`, "disk\n");
  const opened = await documents.open(path, { target, preview: true, focus: false });
  const id = opened.document.id;
  await files.writeText(path, "changed\n");
  await documents.reload(id);
  await documents.save(id);
  await documents.save(id, { uri: `${directory}/copy.txt` });
  await workbench.close(opened.presentation.target, { discard: true });
  await files.delete(directory, { permanent: true });
}

/** Runs every group; rejects at the first declared option the host refuses. */
// Every schema kind in one contract, served and bound by this package. Its
// own client's calls reject, so this covers registration and binding; the
// cross-package call is covered by the contract product tests.
async function exerciseContracts() {
  const shapes = defineContract("coverage", {
    shapes: s.fn(
      s.object({
        text: s.string(), count: s.int(), ratio: s.number(), flag: s.boolean(), nothing: s.null(),
        free: s.json(), mode: s.enum(["a", "b"]), one: s.literal(1), list: s.array(s.string()),
        pair: s.tuple([s.string(), s.int()]), map: s.record(s.boolean()),
        either: s.union([s.string(), s.int()]), maybe: s.nullable(s.string()), absent: s.optional(s.string()),
      }),
      s.void(),
    ),
    ping: s.fn(s.void(), s.string()),
  });
  const registration = await shapes.serve({ shapes() {}, ping: () => "pong" });
  const client = await shapes.client();
  await rejectsWith(() => client.ping(), "UnsupportedOperation");
  await registration[Symbol.asyncDispose]();
  await rejectsWith(() => shapes.client(), "ContractNotFound");
}

export async function exerciseApi() {
  const groups = [
    exerciseSettings, exerciseThemesAndIcons, exerciseLanguages, exerciseStorage,
    exerciseMenusAndKeymaps, exerciseFilesAndProcesses, exerciseDocumentsAndWorkbench,
    exerciseViewsAndDocuments, exerciseContracts,
  ];
  for (const group of groups) {
    try {
      await group();
    } catch (error) {
      const code = error instanceof Error && "code" in error ? ` ${String(error.code)}` : "";
      throw new Error(`${group.name}${code}: ${String(error)}`);
    }
  }
}
