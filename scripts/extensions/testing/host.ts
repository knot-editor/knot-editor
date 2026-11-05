// An in-memory Knot host for extension tests. Under `node --test`, extension
// code importing "knot" receives `api` (through ./knot.ts), while tests import
// "knot/testing" to arrange windows and documents, script prompt answers, run
// commands, emit application events, and inspect what the extension did.
//
// The fake implements behavior, not mocks, but only as much as tests need:
// anything else throws `not faked`. It does not reproduce host validation,
// scheduling, or durability; product tests cover the real boundary.
import type * as Knot from "knot";

type HostErrorCode = Knot.HostErrorCode;

/** Every value the "knot" module exports, without its private identity brands. */
type KnotApi = {
  [K in keyof typeof Knot as (typeof Knot)[K] extends symbol ? never : K]: (typeof Knot)[K];
};

export type FakeDocument = {
  id: Knot.DocumentId;
  title: string;
  kind: Knot.DocumentInfo["kind"];
  resourceUri: string | null;
  dirty: boolean;
  revision: number;
  text: string;
  language: string;
};

type FakeTab = {
  target: Knot.ViewTarget;
  document: Knot.DocumentId;
  view: Knot.EditorViewState;
  /** How the last `setSelection` revealed the caret, if any did. */
  reveal: "nearest" | "center" | "top" | "bottom" | null;
  keyContexts: Set<string>;
  /** Lines the viewport shows; the view's `top` is its first line. */
  viewportLines: number;
  /** The caret style the extension chose, or null for the default bar. */
  cursorStyle: Knot.CursorStyle | null;
  /** Close listeners of the long-lived handles bound to this tab. */
  closeListeners: Set<() => void | Promise<void>>;
  /** Selection listeners of the long-lived handles bound to this tab. The
   * fake runs each one per change; the host's coalescing is covered by
   * product tests. */
  selectionListeners: Set<() => void | Promise<void>>;
};

/** A `bind` or `bindText` call; text bindings have a null key. */
export type FakeBinding = {
  key: string | null;
  command: string | Knot.KeybindingCommand | null;
  view: string | null;
  context: string | null;
};

type FakeWindow = {
  target: Knot.WindowTarget;
  pane: Knot.PaneTarget;
  roots: Knot.WorkspaceRoot[];
  generation: number;
  tabs: FakeTab[];
  selected: number;
};

type FakeView = {
  contribution: Knot.ViewContribution;
  visible: boolean;
  text: string;
  invalidations: number;
};

type Answers = {
  showPicker: Knot.PickerResult;
  showInput: Knot.InputResult;
  showConfirmation: Knot.ConfirmationResult;
  pickFolder: Knot.FolderPickerResult;
};

type Prompt = { kind: keyof Answers; options: unknown };

type Handler = Knot.CommandHandler;

/** Everything the fake host holds. `reset()` replaces it. */
function emptyState() {
  return {
    nextId: 1,
    records: new Map<string, string>(),
    documents: new Map<Knot.DocumentId, FakeDocument>(),
    windows: [] as FakeWindow[],
    focused: null as Knot.WindowTarget | null,
    captures: new Map<Knot.CaptureId, Knot.RetainedCapture & { text: string }>(),
    views: new Map<string, FakeView>(),
    commands: new Map<string, Handler>(),
    commandOptions: new Map<string, Knot.CommandOptions>(),
    viewCommands: new Map<string, Handler>(),
    listeners: [] as ((event: Knot.ApplicationEvent) => void | Promise<void>)[],
    sequence: 0,
    launched: false,
    answers: new Map<keyof Answers, unknown[]>(),
    prompts: [] as Prompt[],
    pickerHighlights: [] as (string | null)[],
    inputValues: [] as string[],
    inputMessages: [] as (string | null)[],
    messages: [] as Knot.MessageOptions[],
    clipboard: null as string | null,
    selectedText: null as string | null,
    failures: new Map<string, HostErrorCode[]>(),
    replaced: [] as Knot.ReplaceWindowsOptions[],
    languages: [] as Knot.LanguageDefinition[],
    associations: [] as { id: string; patterns: Knot.LanguagePatterns }[],
    settingDefinitions: new Map<string, Knot.SettingDefinition>([
      ["font.size", { key: "font.size", type: "number", default: 14, description: "Text size in pixels" }],
      ["editor.tabSize", { key: "editor.tabSize", type: "integer", default: 4, description: "Columns per tab stop in editor views" }],
      ["editor.insertSpaces", { key: "editor.insertSpaces", type: "boolean", default: true, description: "Indent with spaces rather than tabs; a document whose text shows a clear style gets its own value" }],
      ["editor.autoClosingBrackets", { key: "editor.autoClosingBrackets", type: "boolean", default: true, description: "Insert the closer of a typed bracket or quote where the language allows, and step over or delete it with the opener" }],
      ["editor.autoSurround", { key: "editor.autoSurround", type: "boolean", default: true, description: "Wrap the selection when typing an opening bracket or quote over it" }],
      ["editor.softWrap", { key: "editor.softWrap", type: "boolean", default: false, description: "Wrap long lines at the viewport width" }],
      ["editor.lineNumbers", { key: "editor.lineNumbers", type: "string", default: "on", description: "Line numbers in the editor gutter" }],
      ["theme.mode", { key: "theme.mode", type: "string", default: "dark", description: "Which theme applies" }],
      ["theme.dark", { key: "theme.dark", type: "string", default: "One Dark", description: "The theme used in dark mode" }],
      ["theme.light", { key: "theme.light", type: "string", default: "One Light", description: "The theme used in light mode" }],
      ["keymap.preset", { key: "keymap.preset", type: "string", default: "knot", description: "The keymap preset that supplies default key bindings" }],
      ["statusBar.visible", { key: "statusBar.visible", type: "boolean", default: true, description: "Show the status bar below the workbench" }],
      ["statusBar.hiddenItems", { key: "statusBar.hiddenItems", type: "strings", default: [], description: "IDs of status bar items the bar skips" }],
    ]),
    settingValues: [] as FakeSettingValue[],
    settingListeners: [] as { key: string; listener: (key: Knot.SettingKey) => void | Promise<void> }[],
    themes: [] as Knot.ThemeDefinition[],
    keymapPresets: [] as Knot.KeymapPresetDefinition[],
    bindings: [] as FakeBinding[],
    colors: new Map<string, Knot.ColorDefinition>(),
    statusItems: [] as FakeStatusItem[],
  };
}

type FakeStatusItem = {
  id: string;
  value: Knot.StatusItemOptions;
  windows: Map<Knot.WindowTarget, Knot.StatusItemOptions>;
};

type FakeSettingValue = {
  key: string;
  language: string | null;
  view: string | null;
  document: string | null;
  value: Knot.JsonValue;
};

let state = emptyState();
// Documents with an open undo group. The fake keeps no edit history, so a
// group only records that one is open.
let undoGroups = new Set<Knot.DocumentId>();

/** Clears every record, window, registration, and scripted answer. */
export function reset() {
  state = emptyState();
  undoGroups = new Set();
}

function nextId(prefix: string) {
  return `${prefix}-${state.nextId++}`;
}

export function hostError(code: HostErrorCode, message: string = code): Knot.HostError {
  const error = new Error(message) as Error & { name: string; code: HostErrorCode };
  error.name = "KnotHostError";
  error.code = code;
  return error as Knot.HostError;
}

function notFaked(name: string): (...args: any[]) => never {
  return () => {
    throw new Error(`not faked: ${name}`);
  };
}

/** Makes the next call to `operation` (for example "storage.write") reject with `code`. */
export function failNext(operation: string, code: HostErrorCode) {
  const queued = state.failures.get(operation) ?? [];
  queued.push(code);
  state.failures.set(operation, queued);
}

function maybeFail(operation: string) {
  const code = state.failures.get(operation)?.shift();
  if (code !== undefined) throw hostError(code, `${operation} failed`);
}

const json = (value: Knot.JsonValue): Knot.JsonValue => JSON.parse(JSON.stringify(value));

// Records ------------------------------------------------------------------

/** Every stored record, parsed. */
export function records(): Record<string, Knot.JsonValue> {
  return Object.fromEntries([...state.records].map(([key, value]) => [key, JSON.parse(value)]));
}

export function record(key: string): Knot.JsonValue | undefined {
  const value = state.records.get(key);
  return value === undefined ? undefined : JSON.parse(value);
}

export function setRecord(key: string, value: Knot.JsonValue) {
  state.records.set(key, JSON.stringify(value));
}

// Documents and windows ----------------------------------------------------

const utf8 = new TextEncoder();
const byteLength = (text: string) => utf8.encode(text).length;

/** The UTF-16 index of a UTF-8 byte offset into `text`, or undefined when it
 * splits a scalar or is out of bounds. */
function utf16AtByte(text: string, offset: number): number | undefined {
  let bytes = 0;
  let index = 0;
  for (const character of text) {
    if (bytes === offset) return index;
    bytes += byteLength(character);
    index += character.length;
  }
  return bytes === offset ? index : undefined;
}

/** Byte ranges of each line without its `\n` or `\r\n` terminator. */
function lineRanges(text: string): Knot.ByteRange[] {
  const ranges: Knot.ByteRange[] = [];
  let start = 0;
  for (const line of text.split("\n")) {
    const length = byteLength(line);
    const end = start + length;
    ranges.push({ startByteOffset: start, endByteOffset: end });
    start = end + 1;
  }
  for (const range of ranges.slice(0, -1)) {
    const lineText = slice(text, range);
    if (lineText.endsWith("\r")) (range as { endByteOffset: number }).endByteOffset -= 1;
  }
  return ranges;
}

function slice(text: string, range: Knot.ByteRange): string {
  const start = utf16AtByte(text, range.startByteOffset);
  const end = utf16AtByte(text, range.endByteOffset);
  if (start === undefined || end === undefined || start > end) throw hostError("InvalidRange");
  return text.slice(start, end);
}

function snapshot(text: string, range: Knot.ByteRange, revision: number): Knot.TextSnapshot {
  return {
    text,
    range,
    revision,
    byteOffsetAtUtf16: (offset) => {
      if (offset < 0 || offset > text.length) throw new RangeError("UTF-16 offset is out of bounds");
      return byteLength(text.slice(0, offset));
    },
    utf16OffsetAtByte: (offset) => {
      const index = utf16AtByte(text, offset);
      if (index === undefined) throw new RangeError("byte offset splits a UTF-8 scalar or is out of bounds");
      return index;
    },
  };
}

type FakeMark = { offset: number | null };

/** The live marks of each document. Edits move them, as in the host. */
const marks = new WeakMap<FakeDocument, Set<FakeMark>>();

function marksOf(document: FakeDocument): Set<FakeMark> {
  let set = marks.get(document);
  if (set === undefined) marks.set(document, (set = new Set()));
  return set;
}

// A mark stays with the character after it: an edit ending at or before it
// shifts it, one that deletes the text it sat in removes it, and one that
// ends after it leaves it alone.
function moveMarks(document: FakeDocument, edit: Knot.TextEdit) {
  const { startByteOffset: start, endByteOffset: end } = edit.range;
  const delta = byteLength(edit.text) - (end - start);
  for (const mark of marksOf(document)) {
    if (mark.offset === null) continue;
    if (mark.offset >= end) mark.offset += delta;
    else if (mark.offset > start) mark.offset = null;
  }
}

// The fake buffer reads and edits text; change listeners and contributions are
// not faked. Edits leave view offsets alone.
const buffers = new WeakMap<FakeDocument, Knot.Buffer>();

function buffer(document: FakeDocument): Knot.Buffer {
  let proxy = buffers.get(document);
  if (proxy === undefined) buffers.set(document, (proxy = createBuffer(document)));
  return proxy;
}

function createBuffer(document: FakeDocument): Knot.Buffer {
  const fake: Omit<Knot.Buffer, keyof Knot.Buffer & symbol> = {
    snapshot: async (range) => {
      const whole = { startByteOffset: 0, endByteOffset: byteLength(document.text) };
      const actual = range ?? whole;
      return snapshot(slice(document.text, actual), actual, document.revision);
    },
    applyEdits: async (edits, { ifRevision }) => {
      if (ifRevision !== document.revision) throw hostError("RevisionConflict");
      let text = document.text;
      let changed = false;
      for (const edit of [...edits].sort((a, b) => b.range.startByteOffset - a.range.startByteOffset)) {
        const start = utf16AtByte(text, edit.range.startByteOffset);
        const end = utf16AtByte(text, edit.range.endByteOffset);
        if (start === undefined || end === undefined || start > end) throw hostError("InvalidRange");
        const next = text.slice(0, start) + edit.text + text.slice(end);
        changed ||= next !== text;
        text = next;
        moveMarks(document, edit);
      }
      if (changed) {
        document.text = text;
        document.revision += 1;
      }
      return { revision: document.revision };
    },
    onDidChange: notFaked("Buffer.onDidChange"),
    lineCount: async () => lineRanges(document.text).length,
    lineRange: async (line) => {
      const range = lineRanges(document.text)[line];
      if (range === undefined) throw hostError("InvalidRange");
      return range;
    },
    lineAt: async (offset) => {
      const index = utf16AtByte(document.text, offset);
      if (index === undefined) throw hostError("InvalidRange");
      return document.text.slice(0, index).split("\n").length - 1;
    },
    createMark: async (offset) => {
      if (document.text === "" || utf16AtByte(document.text, offset) === undefined) {
        throw hostError("InvalidRange");
      }
      const mark: FakeMark = { offset };
      marksOf(document).add(mark);
      return {
        position: async () => mark.offset,
        dispose: async () => {
          marksOf(document).delete(mark);
          mark.offset = null;
        },
      };
    },
    contributions: {
      replace: notFaked("Buffer.contributions.replace"),
      dispose: notFaked("Buffer.contributions.dispose"),
    },
  };
  return fake as unknown as Knot.Buffer;
}

function documentInfo(document: FakeDocument): Knot.DocumentInfo {
  const { text: _text, ...info } = document;
  const context = { language: document.language, view: "editor", document: document.id };
  return {
    ...info,
    buffer: buffer(document),
    indentation: {
      style: resolveSetting("editor.insertSpaces", context) === false ? "tabs" : "spaces",
      size: resolveSetting("editor.tabSize", context) as number,
    },
  };
}

/** Adds a live document. A URI makes it persisted unless a kind is given. */
export function addDocument(
  options: {
    uri?: string;
    kind?: FakeDocument["kind"];
    dirty?: boolean;
    text?: string;
    language?: string;
  } = {},
): Knot.DocumentId {
  const id = nextId("doc") as Knot.DocumentId;
  const resourceUri = options.uri ?? null;
  state.documents.set(id, {
    id,
    title: resourceUri?.split("/").pop() ?? "Untitled",
    kind: options.kind ?? (resourceUri === null ? "untitled" : "persisted"),
    resourceUri,
    dirty: options.dirty ?? false,
    revision: 0,
    text: options.text ?? "",
    language: options.language ?? "plaintext",
  });
  return id;
}

export function document(id: Knot.DocumentId): FakeDocument {
  const found = state.documents.get(id);
  if (found === undefined) throw new Error(`no document ${id}`);
  return found;
}

export function documentIds(): Knot.DocumentId[] {
  return [...state.documents.keys()];
}

function rootsFor(uris: readonly string[], previous: Knot.WorkspaceRoot[] = []) {
  const unique = [...new Set(uris)];
  return unique.map(
    (uri) =>
      previous.find((root) => root.uri === uri) ?? { id: nextId("root") as Knot.RootId, uri },
  );
}

/** Opens a window presenting the documents as tabs of one pane, the first selected. */
export function openWindow(
  options: { roots?: readonly string[]; documents?: readonly Knot.DocumentId[] } = {},
): Knot.WindowTarget {
  const target = { id: nextId("window") } as unknown as Knot.WindowTarget;
  const pane = { window: target } as unknown as Knot.PaneTarget;
  const window: FakeWindow = {
    target,
    pane,
    roots: rootsFor(options.roots ?? []),
    generation: 0,
    tabs: [],
    selected: 0,
  };
  state.windows.push(window);
  for (const id of options.documents ?? []) present(target, id);
  return target;
}

function fakeWindow(target: Knot.WindowTarget): FakeWindow {
  const found = state.windows.find((window) => window.target === target);
  if (found === undefined) throw hostError("InvalidTarget", "window is closed");
  return found;
}

/** Adds a tab for the document to the window and selects it. */
export function present(target: Knot.WindowTarget, id: Knot.DocumentId) {
  const window = fakeWindow(target);
  const view = { window: target, pane: window.pane } as unknown as Knot.ViewTarget;
  window.tabs.push({
    target: view,
    document: id,
    view: { anchor: 0, caret: 0, top: 0, scrollX: 0 },
    keyContexts: new Set(),
    cursorStyle: null,
    viewportLines: 10,
    closeListeners: new Set(),
    selectionListeners: new Set(),
    reveal: null,
  });
  window.selected = window.tabs.length - 1;
}

/** Closes one tab, running the close listeners of its editor handles. */
export async function closeTab(target: Knot.ViewTarget) {
  for (const window of state.windows) {
    const index = window.tabs.findIndex((tab) => tab.target === target);
    if (index < 0) continue;
    const [tab] = window.tabs.splice(index, 1);
    window.selected = Math.max(0, Math.min(window.selected, window.tabs.length - 1));
    const listeners = [...tab!.closeListeners];
    tab!.closeListeners.clear();
    for (const listener of listeners) await listener();
    return;
  }
  throw hostError("InvalidTarget");
}

/** The view target of the tab at `index` in the window. */
export function tabTarget(window: Knot.WindowTarget, index: number): Knot.ViewTarget {
  const tab = fakeWindow(window).tabs[index];
  if (tab === undefined) throw new Error(`the window has no tab ${index}`);
  return tab.target;
}

/** Closes a window; documents no longer presented anywhere close with it. */
export function closeWindow(target: Knot.WindowTarget) {
  state.windows = state.windows.filter((window) => window.target !== target);
  if (state.focused === target) state.focused = null;
  for (const id of [...state.documents.keys()]) {
    if (!state.windows.some((window) => window.tabs.some((tab) => tab.document === id))) {
      state.documents.delete(id);
    }
  }
}

export function setRoots(target: Knot.WindowTarget, roots: readonly string[]): Knot.WorkspaceInfo {
  const window = fakeWindow(target);
  window.roots = rootsFor(roots, window.roots);
  window.generation += 1;
  return workspaceInfo(window);
}

export function windowTargets(): Knot.WindowTarget[] {
  return state.windows.map((window) => window.target);
}

export function rootsOf(target: Knot.WindowTarget): string[] {
  return fakeWindow(target).roots.map((root) => root.uri);
}

export function documentsIn(target: Knot.WindowTarget): Knot.DocumentId[] {
  return fakeWindow(target).tabs.map((tab) => tab.document);
}

export function focusedWindow() {
  return state.focused;
}

function workspaceInfo(window: FakeWindow): Knot.WorkspaceInfo {
  return { window: window.target, roots: [...window.roots], generation: window.generation };
}

function windowInfo(window: FakeWindow): Knot.WindowInfo {
  const tabs = window.tabs.map((tab) => {
    const document = state.documents.get(tab.document);
    return {
      target: tab.target,
      kind: "document" as const,
      document: tab.document,
      buffer: document === undefined ? null : buffer(document),
      tool: null,
    };
  });
  const selected = window.tabs[window.selected]?.target ?? null;
  return {
    target: window.target,
    panes: [{ target: window.pane, tabs, selected: selected as Knot.ViewTarget }],
    currentTarget: selected,
    statusTarget: selected,
    workspace: workspaceInfo(window),
  };
}

// Recovery -----------------------------------------------------------------

export function captureIds(): Knot.CaptureId[] {
  return [...state.captures.keys()];
}

export function captureText(id: Knot.CaptureId): string | undefined {
  return state.captures.get(id)?.text;
}

// Prompts, messages, and views ---------------------------------------------

/** Queues the result of the next prompt of this kind. */
export function answer<K extends keyof Answers>(kind: K, result: Answers[K]) {
  const queued = state.answers.get(kind) ?? [];
  queued.push(result);
  state.answers.set(kind, queued);
}

/** Highlights the next picker reports to its `onHighlight`, in order,
 * before it settles with its scripted answer. */
export function highlight(...ids: (string | null)[]) {
  state.pickerHighlights.push(...ids);
}

/** Values the next input reports to its `onChange` after its initial value,
 * in order, before it settles with its scripted answer. */
export function type(...values: string[]) {
  state.inputValues.push(...values);
}

/** The message each `onChange` call returned, in order; null when it
 * returned nothing. */
export function inputMessages(): readonly (string | null)[] {
  return state.inputMessages;
}

function prompt<K extends keyof Answers>(kind: K, options: unknown): Answers[K] {
  state.prompts.push({ kind, options });
  const result = state.answers.get(kind)?.shift();
  if (result === undefined) throw new Error(`no scripted answer for ${kind}`);
  return result as Answers[K];
}

export function prompts(): readonly Prompt[] {
  return state.prompts;
}

/** Messages shown so far, as `title: message` strings. */
export function messages(): string[] {
  return state.messages.map((options) => options.message ?? options.title);
}

/**
 * The items a window's status bar shows, in display order: each ID's latest
 * registration with the window's own value in place of the shared one, minus
 * the IDs in `statusBar.hiddenItems`.
 */
export function statusItems(target: Knot.WindowTarget): (Knot.StatusItemOptions & { id: string })[] {
  const hidden = (resolveSetting("statusBar.hiddenItems", settingScope(undefined)) ?? []) as string[];
  const latest = new Map<string, FakeStatusItem>();
  for (const item of state.statusItems) latest.set(item.id, item);
  return [...latest.values()]
    .filter((item) => !hidden.includes(item.id))
    .sort((a, b) => state.statusItems.indexOf(a) - state.statusItems.indexOf(b))
    .map((item) => ({ id: item.id, ...(item.windows.get(target) ?? item.value) }))
    .filter((item) => item.text !== "")
    .sort((a, b) => (b.priority ?? 0) - (a.priority ?? 0));
}

export function clipboard() {
  return state.clipboard;
}

export function setSelectedText(text: string | null) {
  state.selectedText = text;
}

export function view(id: string): FakeView {
  const found = state.views.get(id);
  if (found === undefined) throw new Error(`no view ${id}`);
  return found;
}

/** The tree view's children of `parentId`, as presented to `window`. */
export async function treeItems(
  id: string,
  window: Knot.WindowTarget,
  parentId: string | null = null,
): Promise<Knot.TreeItem[]> {
  const content = view(id).contribution.content;
  if (content.kind !== "tree") throw new Error(`${id} is not a tree`);
  const instance = `${id}@window` as Knot.ViewId;
  return [...(await content.provider.getChildren(parentId, { window, instance }))];
}

// Commands and events ------------------------------------------------------

class InvalidArguments extends Error {}
class Unavailable extends Error {}

/** The selected document tab of a window, which a command run there captures. */
function selectedTab(target: Knot.WindowTarget): FakeTab {
  const tab = fakeWindow(target).tabs[fakeWindow(target).selected];
  if (tab === undefined) throw new Error("the window has no selected document");
  return tab;
}

/** The view state of the window's selected document editor. */
export function editorView(window: Knot.WindowTarget = state.windows[0]!.target): Knot.EditorViewState {
  return selectedTab(window).view;
}

/** How the last `setSelection` in the window's selected editor revealed its caret. */
export function editorReveal(window: Knot.WindowTarget = state.windows[0]!.target) {
  return selectedTab(window).reveal;
}

/** The key contexts set on the window's selected document editor, sorted. */
export function keyContexts(window: Knot.WindowTarget = state.windows[0]!.target): string[] {
  return [...selectedTab(window).keyContexts].sort();
}

/** Sets how many lines the window's selected document editor shows. */
export function setViewportLines(lines: number, window: Knot.WindowTarget = state.windows[0]!.target) {
  selectedTab(window).viewportLines = lines;
}

/** The caret style set on the window's selected document editor, or "bar". */
export function cursorStyle(window: Knot.WindowTarget = state.windows[0]!.target): Knot.CursorStyle {
  return selectedTab(window).cursorStyle ?? "bar";
}

/** Whether the window's selected document has an open undo group. */
export function undoGroupOpen(window: Knot.WindowTarget = state.windows[0]!.target): boolean {
  return undoGroups.has(selectedTab(window).document);
}

/** Every `bind` and `bindText` call so far, in order. */
export function bindings(): readonly FakeBinding[] {
  return state.bindings;
}

/** Sets the view state of the window's selected document editor. */
export function setEditorView(
  view: Partial<Knot.EditorViewState>,
  window: Knot.WindowTarget = state.windows[0]!.target,
) {
  const tab = selectedTab(window);
  tab.view = { ...tab.view, ...view };
}

function lineOf(text: string, byteOffset: number): number {
  const index = utf16AtByte(text, byteOffset) ?? 0;
  return text.slice(0, index).split("\n").length - 1;
}

// The first line the viewport can show without scrolling past the text.
function clampTop(tab: FakeTab, top: number): number {
  const count = lineRanges(document(tab.document).text).length;
  return Math.max(0, Math.min(top, count - tab.viewportLines));
}

// The fake editor does not clamp offsets or check character boundaries
// beyond the end of the text.
function editor(tab: FakeTab): Knot.Editor {
  const live = () => {
    if (!state.windows.some((window) => window.tabs.includes(tab))) throw hostError("InvalidTarget");
    return tab;
  };
  return {
    selection: async () => {
      const { anchor, caret } = live().view;
      return { anchor, caret };
    },
    setSelection: async ({ anchor, caret }, options) => {
      const tab = live();
      const length = byteLength(document(tab.document).text);
      if (anchor > length || caret > length) throw hostError("InvalidRange");
      tab.view = { ...tab.view, anchor, caret };
      tab.reveal = options?.reveal ?? "nearest";
      const reveal = tab.reveal;
      if (reveal === "top" || reveal === "bottom") {
        const line = lineOf(document(tab.document).text, caret);
        tab.view = { ...tab.view, top: clampTop(tab, reveal === "top" ? line : line - tab.viewportLines + 1) };
      }
      await notifySelection(tab);
    },
    visibleLines: async () => {
      const tab = live();
      const count = lineRanges(document(tab.document).text).length;
      const first = Math.min(tab.view.top, count - 1);
      return { first, last: Math.max(first, Math.min(first + tab.viewportLines, count) - 1) };
    },
    scrollBy: async (lines) => {
      if (!Number.isSafeInteger(lines)) throw new TypeError("lines must be a safe integer");
      const tab = live();
      tab.view = { ...tab.view, top: clampTop(tab, tab.view.top + lines) };
    },
    setKeyContext: async (name, active) => {
      const { keyContexts } = live();
      if (active) keyContexts.add(name);
      else keyContexts.delete(name);
    },
    setCursorStyle: async (style) => {
      if (style !== null && style !== "bar" && style !== "block" && style !== "underline") {
        throw new TypeError("cursor style must be bar, block, underline or null");
      }
      live().cursorStyle = style;
    },
    beginUndoGroup: async () => {
      const { document } = live();
      if (undoGroups.has(document)) throw hostError("OperationInProgress");
      undoGroups.add(document);
      return { dispose: async () => void undoGroups.delete(document) };
    },
    viewState: async () => ({ ...live().view }),
    restoreViewState: async (view) => {
      live().view = { ...view };
      await notifySelection(tab);
    },
  };
}

async function notifySelection(tab: FakeTab) {
  for (const listener of [...tab.selectionListeners]) await listener();
}

function editorHandle(target: Knot.ViewTarget): Knot.EditorHandle {
  const tab = state.windows.flatMap((window) => window.tabs).find((candidate) => candidate.target === target);
  if (tab === undefined) throw hostError("InvalidTarget");
  const listeners = new Set<() => void | Promise<void>>();
  const selectionListeners = new Set<() => void | Promise<void>>();
  let disposed = false;
  const live = editor(tab);
  const guard =
    <A extends unknown[], R>(call: (...args: A) => Promise<R>) =>
    async (...args: A) => {
      if (disposed) throw hostError("InvalidTarget");
      return call(...args);
    };
  return {
    selection: guard(live.selection),
    setSelection: guard(live.setSelection),
    setKeyContext: guard(live.setKeyContext),
    visibleLines: guard(live.visibleLines),
    scrollBy: guard(live.scrollBy),
    setCursorStyle: guard(live.setCursorStyle),
    beginUndoGroup: guard(live.beginUndoGroup),
    viewState: guard(live.viewState),
    restoreViewState: guard(live.restoreViewState),
    onDidClose(listener) {
      tab.closeListeners.add(listener);
      listeners.add(listener);
      return {
        dispose: async () => {
          tab.closeListeners.delete(listener);
          listeners.delete(listener);
        },
      };
    },
    async onDidChangeSelection(listener) {
      if (disposed) throw hostError("InvalidTarget");
      tab.selectionListeners.add(listener);
      selectionListeners.add(listener);
      return {
        dispose: async () => {
          tab.selectionListeners.delete(listener);
          selectionListeners.delete(listener);
        },
      };
    },
    async dispose() {
      disposed = true;
      for (const listener of listeners) tab.closeListeners.delete(listener);
      for (const listener of selectionListeners) tab.selectionListeners.delete(listener);
      listeners.clear();
      selectionListeners.clear();
    },
  };
}

function context(window: Knot.WindowTarget, args: Knot.JsonValue, tool: string | null): Knot.CommandContext {
  const signal = {
    aborted: false,
    reason: undefined,
    addEventListener() {},
    removeEventListener() {},
    throwIfAborted() {},
  };
  const target = { window: "window", windowTarget: window, view: null, tool } as unknown as Knot.CommandTarget;
  const tab = tool === null ? fakeWindow(window).tabs[fakeWindow(window).selected] : undefined;
  const presented = tab === undefined ? undefined : state.documents.get(tab.document);
  return {
    target,
    buffer: presented === undefined ? null : buffer(presented),
    editor: tab === undefined || presented === undefined ? null : editor(tab),
    arguments: args,
    signal,
  };
}

/**
 * Runs a command as if invoked in `window`, through the view-claimed handler
 * when `view` names one, and otherwise in the window's selected document
 * editor, if any. Resolves with "unavailable" when the handler reports it;
 * other handler errors propagate to the test.
 */
export async function run(
  name: string,
  options: { window?: Knot.WindowTarget; arguments?: Knot.JsonValue; view?: string } = {},
): Promise<"completed" | "unavailable"> {
  const window = options.window ?? state.windows[0]?.target;
  if (window === undefined) throw new Error("run needs a window");
  const viewHandler =
    options.view === undefined ? undefined : state.viewCommands.get(`${options.view}:${name}`);
  const handler = viewHandler ?? state.commands.get(name);
  if (handler === undefined) throw new Error(`no command ${name}`);
  try {
    await handler(context(window, options.arguments ?? null, options.view ?? null));
  } catch (error) {
    if (error instanceof Unavailable) return "unavailable";
    throw error;
  }
  return "completed";
}

export function commandNames(): string[] {
  return [...state.commands.keys()];
}

/** Delivers an application event to every observer, awaiting each in turn. */
export async function emit(change: Knot.ApplicationChange) {
  state.sequence += 1;
  for (const listener of state.listeners) await listener({ ...change, sequence: state.sequence });
}

/** Marks startup complete and delivers the launched event. */
export async function launch() {
  state.launched = true;
  await emit({ type: "launched" });
}

export function replaced(): readonly Knot.ReplaceWindowsOptions[] {
  return state.replaced;
}

// Session replacement ------------------------------------------------------

function restoreEntry(entry: Knot.SessionEntry): { id: Knot.DocumentId | null; outcome: Knot.SessionEntryOutcome } {
  if (entry.document != null && state.documents.has(entry.document)) {
    return { id: entry.document, outcome: { kind: "live", document: entry.document } };
  }
  if (entry.capture != null) {
    const capture = state.captures.get(entry.capture);
    if (capture !== undefined) {
      const id = addDocument({
        ...(capture.resourceUri === null ? {} : { uri: capture.resourceUri }),
        kind: capture.kind,
        dirty: true,
        text: capture.text,
      });
      return { id, outcome: { kind: "restored", document: id, resource: capture.resourceUri === null ? "untitled" : "unchanged" } };
    }
  }
  if (entry.uri != null) {
    const live = [...state.documents.values()].find((document) => document.resourceUri === entry.uri);
    if (live !== undefined) return { id: live.id, outcome: { kind: "live", document: live.id } };
    const id = addDocument({ uri: entry.uri });
    return { id, outcome: { kind: "opened", document: id } };
  }
  return { id: null, outcome: { kind: "missing" } };
}

/**
 * A minimal commit: closes the released windows, closes suspended documents
 * that are no longer presented, and opens each requested window with its
 * entries resolved live first, then from captures, then by URI.
 */
function replaceWindows(options: Knot.ReplaceWindowsOptions): Knot.ReplaceWindowsResult {
  maybeFail("workbench.replaceWindows");
  state.replaced.push(options);
  const suspended = new Set(options.suspend.map((entry) => entry.document));
  state.windows = state.windows.filter((window) => !(options.release ?? []).includes(window.target));
  for (const id of suspended) {
    if (!state.windows.some((window) => window.tabs.some((tab) => tab.document === id))) {
      state.documents.delete(id);
    }
  }
  const opened = options.windows.map((request) => {
    const target = openWindow({ roots: request.roots });
    const entries = request.entries.map((entry) => {
      const { id, outcome } = restoreEntry(entry);
      if (id !== null) present(target, id);
      return outcome;
    });
    return { window: target, entries, layout: request.layout == null ? null : ("applied" as const) };
  });
  const focus = options.focus == null ? undefined : opened[options.focus];
  if (focus !== undefined) state.focused = focus.window;
  return { windows: opened, protected: true };
}

// Languages ----------------------------------------------------------------

/** The grammars compiled into Knot, as `languages.grammars()` lists them. */
const GRAMMARS = [
  "bash", "c", "cpp", "css", "go", "html", "java", "javascript", "json", "markdown",
  "markdown_inline", "python", "query", "rust", "toml", "tsx", "typescript", "yaml",
];

/** The names a highlight capture resolves to, as `src/app/theme.rs` lists
 * them; queries should use these exactly so no capture is silently ignored. */
export const CAPTURE_NAMES: readonly string[] = [
  "attribute", "attribute.builtin", "boolean", "character", "character.special", "comment",
  "comment.documentation", "comment.error", "comment.note", "comment.todo", "comment.warning",
  "constant", "constant.builtin", "constant.macro", "constructor", "diff.delta", "diff.minus",
  "diff.plus", "function", "function.builtin", "function.call", "function.macro",
  "function.method", "function.method.call", "keyword", "keyword.conditional",
  "keyword.conditional.ternary", "keyword.coroutine", "keyword.debug", "keyword.directive",
  "keyword.directive.define", "keyword.exception", "keyword.function", "keyword.import",
  "keyword.modifier", "keyword.operator", "keyword.repeat", "keyword.return", "keyword.type",
  "label", "markup.heading", "markup.heading.1", "markup.heading.2", "markup.heading.3",
  "markup.heading.4", "markup.heading.5", "markup.heading.6", "markup.italic", "markup.link",
  "markup.link.label", "markup.link.url", "markup.list", "markup.list.checked",
  "markup.list.unchecked", "markup.math", "markup.quote", "markup.raw", "markup.raw.block",
  "markup.strikethrough", "markup.strong", "markup.underline", "module", "module.builtin",
  "number", "number.float", "operator", "property", "punctuation.bracket",
  "punctuation.delimiter", "punctuation.special", "string", "string.documentation",
  "string.escape", "string.regexp", "string.special", "string.special.path",
  "string.special.symbol", "string.special.url", "tag", "tag.attribute", "tag.builtin",
  "tag.delimiter", "type", "type.builtin", "type.definition", "variable", "variable.builtin",
  "variable.member", "variable.parameter", "variable.parameter.builtin",
];

/** The color keys native code defines, as the native table lists them. */
export const NATIVE_COLOR_KEYS: readonly string[] = [
  "background", "border", "border.hover", "button.background", "button.hover.background",
  "dropTarget.background", "editor.background", "editor.bracketMatch", "editor.cursor", "editor.foreground",
  "editor.gutter.background", "editor.lineHighlight", "editor.lineNumber", "editor.lineNumber.active",
  "editor.markedText", "editor.selection", "error", "info", "list.group.background",
  "list.hover.background", "list.match.foreground", "list.selected.background", "list.selected.foreground",
  "popover.background", "popover.border", "scrim", "scrollbar.thumb",
  "statusBar.background", "statusBar.border", "statusBar.foreground", "statusBar.item.hover.background", "success",
  "tab.active.background", "tab.active.foreground", "tab.bar.background",
  "tab.inactive.background", "tab.inactive.foreground", "terminal.ansi.black",
  "terminal.ansi.blue", "terminal.ansi.brightBlack", "terminal.ansi.brightBlue",
  "terminal.ansi.brightCyan", "terminal.ansi.brightGreen", "terminal.ansi.brightMagenta",
  "terminal.ansi.brightRed", "terminal.ansi.brightWhite", "terminal.ansi.brightYellow",
  "terminal.ansi.cyan", "terminal.ansi.green", "terminal.ansi.magenta", "terminal.ansi.red",
  "terminal.ansi.white", "terminal.ansi.yellow", "terminal.background", "terminal.cursor",
  "terminal.foreground", "terminal.selection", "text", "text.accent", "text.muted", "warning",
];

/** Every live theme registration, in registration order. */
export function themeDefinitions(): readonly Knot.ThemeDefinition[] {
  return state.themes;
}

/** Every live keymap preset registration, in registration order. */
export function keymapPresetDefinitions(): readonly Knot.KeymapPresetDefinition[] {
  return state.keymapPresets;
}

/** Every live extension color definition, in definition order. */
export function colorDefinitions(): readonly Knot.ColorDefinition[] {
  return [...state.colors.values()];
}

/** Every live association, in the order it was made. The fake does not detect. */
export function languageAssociations(): readonly { id: string; patterns: Knot.LanguagePatterns }[] {
  return state.associations;
}

/** Every live language registration, queries included, in registration order. */
export function languageDefinitions(): readonly Knot.LanguageDefinition[] {
  return state.languages;
}

function languageInfo(definition: Knot.LanguageDefinition): Knot.LanguageInfo {
  return {
    id: definition.id,
    name: definition.name,
    grammar: definition.grammar,
    fileExtensions: definition.fileExtensions ?? [],
    fileNames: definition.fileNames ?? [],
    interpreters: definition.interpreters ?? [],
  };
}

// Fills pattern lists and optional queries, as `languages.get` returns them.
function registeredLanguage(definition: Knot.LanguageDefinition): Knot.LanguageDefinition {
  return {
    ...definition,
    ...languageInfo(definition),
    queries: {
      highlights: definition.queries.highlights,
      injections: definition.queries.injections ?? "",
      indents: definition.queries.indents ?? "",
    },
  };
}

// Settings -----------------------------------------------------------------

const settingTypes: Record<Knot.SettingType, (value: unknown) => boolean> = {
  boolean: (value) => typeof value === "boolean",
  integer: (value) => Number.isSafeInteger(value),
  number: (value) => Number.isFinite(value),
  string: (value) => typeof value === "string",
  strings: (value) => Array.isArray(value) && value.every((item) => typeof item === "string"),
};

function settingScope(scope: Knot.SettingsScope | undefined) {
  return { language: scope?.language ?? null, view: scope?.view ?? null, document: scope?.document ?? null };
}

function sameScope(entry: FakeSettingValue, scope: ReturnType<typeof settingScope>) {
  return entry.language === scope.language && entry.view === scope.view && entry.document === scope.document;
}

// The matching entry with the most facts wins, and the latest write wins a tie.
function resolveSetting(key: string, context: ReturnType<typeof settingScope>): Knot.JsonValue | undefined {
  const definition = state.settingDefinitions.get(key);
  if (definition === undefined) return undefined;
  let best: FakeSettingValue | undefined;
  let bestFacts = -1;
  for (const entry of state.settingValues) {
    if (entry.key !== key) continue;
    if (entry.language !== null && entry.language !== context.language) continue;
    if (entry.view !== null && entry.view !== context.view) continue;
    if (entry.document !== null && entry.document !== context.document) continue;
    const count = Number(entry.language !== null) + Number(entry.view !== null) + Number(entry.document !== null);
    if (count >= bestFacts) {
      best = entry;
      bestFacts = count;
    }
  }
  return best === undefined ? definition.default : best.value;
}

async function notifySetting(key: string) {
  for (const entry of [...state.settingListeners]) {
    if (entry.key === key) await entry.listener(key as Knot.SettingKey);
  }
}

/** The definitions packages made, keyed by setting key. */
export function settingDefinitions(): ReadonlyMap<string, Knot.SettingDefinition> {
  return state.settingDefinitions;
}

// The "knot" module --------------------------------------------------------

const LAYOUT: Knot.WindowLayout = {
  version: 1,
  geometry: null,
  root: null,
  focused: null,
  dockSizes: { left: 0, right: 0, bottom: 0 },
};

export const api: KnotApi = {
  documents: {
    list: async () => [...state.documents.values()].map(documentInfo),
    get: async (id) => {
      const document = state.documents.get(id);
      return document === undefined ? null : documentInfo(document);
    },
    open: notFaked("documents.open"),
    createUntitled: notFaked("documents.createUntitled"),
    present: notFaked("documents.present"),
    setLanguage: async (id, language) => {
      const document = state.documents.get(id);
      if (document === undefined) throw hostError("DocumentNotFound");
      // The fake neither detects nor checks registration.
      document.language = language ?? "plaintext";
    },
  },
  languages: {
    // Queries are not compiled; product tests cover InvalidQuery.
    register: async (definition) => {
      if (!GRAMMARS.includes(definition.grammar)) throw hostError("UnknownGrammar");
      state.languages.push(definition);
      return {
        dispose: async () => {
          state.languages = state.languages.filter((candidate) => candidate !== definition);
        },
      };
    },
    associate: async (id, patterns) => {
      const association = { id, patterns };
      state.associations.push(association);
      return {
        dispose: async () => {
          state.associations = state.associations.filter((candidate) => candidate !== association);
        },
      };
    },
    get: async (id) => {
      const definition = [...state.languages].reverse().find((candidate) => candidate.id === id);
      return definition === undefined ? null : registeredLanguage(definition);
    },
    // Later registrations win, as in the native registry.
    list: async () => {
      const byId = new Map(state.languages.map((definition) => [definition.id, definition]));
      // Code-unit order, as the native registry sorts IDs, not locale order.
      return [...byId.values()]
        .map(languageInfo)
        .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
    },
    grammars: async () => GRAMMARS,
  },
  // Values are kept in write order, so the latest entry wins a tie. The fake
  // keeps mismatching pending values rather than reporting them.
  settings: {
    define: async (definition) => {
      if (state.settingDefinitions.has(definition.key)) throw hostError("DuplicateSetting");
      state.settingDefinitions.set(definition.key, definition);
      const accepts = settingTypes[definition.type];
      state.settingValues = state.settingValues.filter(
        (entry) => entry.key !== definition.key || accepts(entry.value),
      );
    },
    set: async (keyOrValues: string | Partial<Knot.Settings>, valueOrScope?: unknown, scope?: Knot.SettingsScope) => {
      const values = (typeof keyOrValues === "string"
        ? { [keyOrValues]: valueOrScope }
        : keyOrValues) as Record<string, Knot.JsonValue>;
      const facts = settingScope(typeof keyOrValues === "string" ? scope : (valueOrScope as Knot.SettingsScope));
      for (const [key, value] of Object.entries(values)) {
        const definition = state.settingDefinitions.get(key);
        if (definition !== undefined && !settingTypes[definition.type](value)) {
          throw hostError("SettingTypeMismatch");
        }
      }
      for (const [key, value] of Object.entries(values)) {
        state.settingValues = state.settingValues.filter((entry) => entry.key !== key || !sameScope(entry, facts));
        state.settingValues.push({ key, ...facts, value: json(value) });
        await notifySetting(key);
      }
    },
    get: async (key, facts) => resolveSetting(key, settingScope(facts)) as never,
    reset: async (key, scope) => {
      const facts = settingScope(scope);
      const before = state.settingValues.length;
      state.settingValues = state.settingValues.filter((entry) => entry.key !== key || !sameScope(entry, facts));
      if (state.settingValues.length !== before) await notifySetting(key);
    },
    observe: async (key, listener) => {
      const entry = { key, listener };
      state.settingListeners.push(entry);
      return {
        dispose: async () => {
          state.settingListeners = state.settingListeners.filter((candidate) => candidate !== entry);
        },
      };
    },
  },
  // The fake neither validates colors nor selects a theme.
  themes: {
    register: async (definition) => {
      const unknown = Object.keys(definition.syntax ?? {}).find((name) =>
        !CAPTURE_NAMES.some((capture) => capture === name || capture.startsWith(`${name}.`)));
      if (unknown !== undefined) throw hostError("UnknownCapture");
      state.themes.push(definition);
      return {
        dispose: async () => {
          state.themes = state.themes.filter((candidate) => candidate !== definition);
        },
      };
    },
    // Later registrations win, as in the native registry.
    get: async (name) => {
      const definition = [...state.themes].reverse().find((candidate) => candidate.name === name);
      return definition === undefined
        ? null
        : { ...definition, colors: definition.colors ?? {}, syntax: definition.syntax ?? {} };
    },
    list: async () => {
      const byName = new Map(state.themes.map((definition) => [definition.name, definition.appearance]));
      return [...byName]
        .map(([name, appearance]) => ({ name, appearance }))
        .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    },
    defineColor: async (definition) => {
      if (NATIVE_COLOR_KEYS.includes(definition.key) || state.colors.has(definition.key)) {
        throw hostError("DuplicateColor");
      }
      state.colors.set(definition.key, definition);
    },
  },
  windows: {
    list: async () => state.windows.map(windowInfo),
    get: async (target) => {
      const window = state.windows.find((candidate) => candidate.target === target);
      return window === undefined ? null : windowInfo(window);
    },
    open: async (options = {}) => {
      maybeFail("windows.open");
      const target = openWindow({ roots: options.roots ?? [] });
      state.focused = target;
      return windowInfo(fakeWindow(target));
    },
    focus: async (target) => {
      fakeWindow(target);
      state.focused = target;
    },
  },
  workspaces: {
    get: async (target) => {
      const window = state.windows.find((candidate) => candidate.target === target);
      return window === undefined ? null : workspaceInfo(window);
    },
    openFolder: notFaked("workspaces.openFolder"),
    replaceRoots: async (roots, options = {}) => {
      maybeFail("workspaces.replaceRoots");
      const target = options.target ?? state.windows[0]?.target;
      if (target === undefined) throw hostError("ContextUnavailable");
      return setRoots(target, roots);
    },
    pickFolder: async (options) => prompt("pickFolder", options),
  },
  terminals: {
    open: notFaked("terminals.open"),
  },
  storage: {
    read: async (key) => {
      maybeFail("storage.read");
      const value = state.records.get(key);
      return value === undefined ? { found: false } : { found: true, value: JSON.parse(value) };
    },
    write: async (key, value) => {
      maybeFail("storage.write");
      state.records.set(key, JSON.stringify(json(value)));
    },
    delete: async (key) => {
      maybeFail("storage.delete");
      state.records.delete(key);
    },
    list: async () => [...state.records.keys()].sort(),
  },
  recovery: {
    list: async () => [...state.captures.values()].reverse().map(({ text: _text, ...capture }) => capture),
    retain: async (id) => {
      maybeFail("recovery.retain");
      const document = state.documents.get(id);
      if (document === undefined) throw hostError("DocumentNotFound");
      const capture = {
        id: nextId("capture") as Knot.CaptureId,
        document: id,
        title: document.title,
        kind: document.kind,
        resourceUri: document.resourceUri,
        capturedAt: Date.now(),
        text: document.text,
      };
      state.captures.set(capture.id, capture);
      const { text: _text, ...retained } = capture;
      return retained;
    },
    release: async (id) => {
      maybeFail("recovery.release");
      return state.captures.delete(id);
    },
    restore: notFaked("recovery.restore"),
  },
  workbench: {
    observe: async (listener) => {
      state.listeners.push(listener);
      return {
        snapshot: {
          documents: [...state.documents.values()].map(documentInfo),
          windows: state.windows.map(windowInfo),
          activeWindow: state.focused,
          activeTarget:
            state.windows.find((window) => window.target === state.focused)?.tabs[
              state.windows.find((window) => window.target === state.focused)!.selected
            ]?.target ?? null,
          launched: state.launched,
        },
        watermark: state.sequence,
        dispose: async () => {
          state.listeners = state.listeners.filter((candidate) => candidate !== listener);
        },
      };
    },
    close: notFaked("workbench.close"),
    editor: async (target) => editorHandle(target),
    replaceWindows: async (options) => replaceWindows(options),
    captureLayout: async (target) => {
      fakeWindow(target);
      return LAYOUT;
    },
    registerView: async (contribution) => {
      if (state.views.has(contribution.id)) throw hostError("ViewIdInUse");
      const registered: FakeView = {
        contribution,
        visible: contribution.placement?.visible ?? true,
        text: "",
        invalidations: 0,
      };
      state.views.set(contribution.id, registered);
      return {
        id: contribution.id,
        show: async () => {
          registered.visible = true;
        },
        hide: async () => {
          registered.visible = false;
        },
        focus: async () => {
          registered.visible = true;
        },
        invalidate: async () => {
          registered.invalidations += 1;
        },
        setText: async (text) => {
          registered.text = text;
        },
        dispose: async () => {
          state.views.delete(contribution.id);
        },
      };
    },
    showPicker: async (options) => {
      const result = prompt("showPicker", options);
      const highlights = state.pickerHighlights;
      state.pickerHighlights = [];
      if (options.onHighlight !== undefined) {
        for (const id of highlights) await options.onHighlight(id);
      }
      return result;
    },
    showInput: async (options) => {
      const result = prompt("showInput", options);
      const values = [options.value ?? "", ...state.inputValues];
      state.inputValues = [];
      if (options.onChange !== undefined) {
        for (const value of values) {
          const message = await options.onChange(value);
          state.inputMessages.push(typeof message === "string" ? message : null);
        }
      }
      return result;
    },
    showConfirmation: async (options) => prompt("showConfirmation", options),
    showMessage: async (options) => {
      state.messages.push(options);
    },
    registerStatusItem: async (id, options) => {
      const entry: FakeStatusItem = { id, value: { ...options }, windows: new Map() };
      state.statusItems.push(entry);
      let live = true;
      return {
        id,
        update: async (next: Knot.StatusItemOptions | null, scope?: { readonly window: Knot.WindowTarget }) => {
          if (!live || !state.statusItems.includes(entry)) throw hostError("StatusItemRegistrationNotFound");
          if (scope === undefined) {
            entry.value = { ...next! };
          } else if (next === null) {
            entry.windows.delete(scope.window);
          } else {
            entry.windows.set(scope.window, { ...next });
          }
        },
        dispose: async () => {
          live = false;
          state.statusItems = state.statusItems.filter((candidate) => candidate !== entry);
        },
      };
    },
    selectedText: async () => state.selectedText,
    readClipboardText: async () => state.clipboard,
    writeClipboardText: async (text) => {
      state.clipboard = text;
    },
  },
  commands: {
    register: async (name, handler, options) => {
      if (state.commands.has(name)) throw hostError("CommandNameInUse");
      state.commandOptions.set(name, options ?? {});
      state.commands.set(name, handler);
      return {
        dispose: async () => {
          state.commands.delete(name);
          state.commandOptions.delete(name);
        },
      };
    },
    registerForView: async (viewKind, name, handler) => {
      state.viewCommands.set(`${viewKind}:${name}`, handler);
      return {
        dispose: async () => {
          state.viewCommands.delete(`${viewKind}:${name}`);
        },
      };
    },
    invoke: async (name, args = null) => {
      const handler = state.commands.get(name);
      const window = state.windows[0]?.target;
      if (handler === undefined || window === undefined) return { kind: "unavailable" };
      try {
        await handler(context(window, args, null));
        return { kind: "completed" };
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        if (error instanceof Unavailable) return { kind: "unavailable" };
        return error instanceof InvalidArguments
          ? { kind: "invalidArgument", message }
          : { kind: "handlerFailure", message };
      }
    },
    invalidArguments: (message) => {
      throw new InvalidArguments(message);
    },
    unavailable: () => {
      throw new Unavailable("command unavailable");
    },
  },
  editor: {
    activeBuffer: notFaked("editor.activeBuffer"),
    registerCompletionProvider: notFaked("editor.registerCompletionProvider"),
  },
  // The fake records bindings without resolving or validating them, selects
  // no preset, and does not know the native Knot preset's bindings.
  keymaps: {
    bind: (key, command, options) => {
      state.bindings.push({ key, command, view: options?.view ?? null, context: options?.context ?? null });
    },
    unbind: notFaked("keymaps.unbind"),
    bindText: (command, options) => {
      state.bindings.push({ key: null, command, view: options?.view ?? null, context: options?.context ?? null });
    },
    unbindText: notFaked("keymaps.unbindText"),
    registerPreset: async (definition) => {
      if (definition.name === "knot") throw new TypeError("the knot keymap preset cannot be replaced");
      state.keymapPresets.push(definition);
      return {
        dispose: async () => {
          state.keymapPresets = state.keymapPresets.filter((candidate) => candidate !== definition);
        },
      };
    },
    // Later registrations win, as in the native registry.
    getPreset: async (name) => {
      if (name === "knot") return notFaked("keymaps.getPreset(\"knot\")")();
      const definition = [...state.keymapPresets].reverse().find((candidate) => candidate.name === name);
      return definition === undefined
        ? null
        : {
          name: definition.name,
          title: definition.title ?? definition.name,
          bindings: definition.bindings.map((binding) => ({
            key: binding.key,
            command: binding.command === null
              ? null
              : typeof binding.command === "string"
              ? { name: binding.command, arguments: null }
              : { name: binding.command.name, arguments: binding.command.arguments ?? null },
            view: binding.view ?? null,
          })),
        };
    },
    listPresets: async () => {
      const byName = new Map([["knot", "Knot"]]);
      for (const definition of state.keymapPresets) byName.set(definition.name, definition.title ?? definition.name);
      return [...byName]
        .map(([name, title]) => ({ name, title }))
        .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    },
  },
  platform: "macos",
};
