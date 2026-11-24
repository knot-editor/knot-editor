import type * as Declared from "knot";
import {
  activeBuffer, createBuffer,
  storageRead, storageWrite, storageDelete, storageList,
  historyRecent, historyRecord, historyForget, historyOrder,
  recoveryList, recoveryRetain, recoveryRelease, recoveryRestore,
  observeWorkbench, trackDocuments, closePresentation, openDiff, registerComposite, openEditor, replaceWindows, captureLayout,
  openDocument, createUntitled, presentDocument, reloadDocument, saveDocument, listDocuments, getDocument, setDocumentLanguage, listWindows, getWindow, findView, openWindow, focusWindow, getWorkspace, openFolder, replaceWorkspaceRoots, pickFolder, readFileText, listFiles, searchFiles, isFileIgnored, statFile, writeFileText, createFile, createDirectory, renameFile, copyFile, revealFile, deleteFile, watchFiles, runProcess, spawnProcess, openTerminal,
  invalidCommandArguments,
  unavailableCommand,
  keybinding,
  removeKeybinding,
  textBinding,
  removeTextBinding,
  registerKeymapPreset, getKeymapPreset, listKeymapPresets,
  platform,
  invokeCommand,
  listCommands,
  registerCommand,
  registerViewCommand,
  selectedViewText,
  selectedTreeItem,
  writeClipboardText,
  readClipboardText,
  showPicker,
  showInput,
  showConfirmation,
  showNotice,
  markNotificationsRead,
  dismissNotification,
  clearNotifications,
  registerStatusItem,
  addMenuItem,
  createDecorationSource,
  matchCandidates,
  completionContract, pasteContract,
  registerView,
  lastFocusedDocument,
  registerLanguage,
  associateLanguage,
  listLanguages,
  getLanguage,
  listGrammars,
  defineSetting, setSettings, getSetting, resetSetting, observeSetting,
  registerTheme, getTheme, listThemes, defineColor,
  defineIcon, overrideIcon, associateIcons, registerIconPack, listIconPacks,
  homeUri,
  diff,
} from "knot:bootstrap";

export { platform, homeUri, diff };
diff satisfies typeof Declared.diff;
platform satisfies typeof Declared.platform;
homeUri satisfies typeof Declared.homeUri;

export const keymaps = Object.freeze({
  bind: keybinding,
  unbind: removeKeybinding,
  bindText: textBinding,
  unbindText: removeTextBinding,
  registerPreset: registerKeymapPreset,
  getPreset: getKeymapPreset,
  listPresets: listKeymapPresets,
}) satisfies typeof Declared.keymaps;

export const editor = Object.freeze({
  activeBuffer,
}) satisfies typeof Declared.editor;

export const commands = Object.freeze({
  invalidArguments: invalidCommandArguments,
  invoke: invokeCommand,
  list: listCommands,
  register: registerCommand,
  registerForView: registerViewCommand,
  unavailable: unavailableCommand,
}) satisfies typeof Declared.commands;

export const workbench = Object.freeze({
  observe: observeWorkbench,
  close: closePresentation,
  openDiff,
  registerComposite,
  editor: openEditor,
  replaceWindows,
  captureLayout,
  registerView,
  lastFocusedDocument,
  selectedText: selectedViewText,
  selectedTreeItem,
  writeClipboardText,
  readClipboardText,
  showPicker,
  showInput,
  showConfirmation,
  showNotice,
  markNotificationsRead,
  dismissNotification,
  clearNotifications,
  registerStatusItem,
}) satisfies typeof Declared.workbench;

export const decorations = Object.freeze({ createSource: createDecorationSource }) satisfies typeof Declared.decorations;
export const menus = Object.freeze({ add: addMenuItem }) satisfies typeof Declared.menus;
export const completion = Object.freeze({ match: matchCandidates }) satisfies typeof Declared.completion;
export { completionContract, pasteContract };
export const documents = Object.freeze({ list: listDocuments, get: getDocument, open: openDocument, createUntitled, present: presentDocument, reload: reloadDocument, save: saveDocument, setLanguage: setDocumentLanguage, track: trackDocuments }) satisfies typeof Declared.documents;
export const languages = Object.freeze({ register: registerLanguage, associate: associateLanguage, get: getLanguage, list: listLanguages, grammars: listGrammars }) satisfies typeof Declared.languages;
export const settings = Object.freeze({ define: defineSetting, set: setSettings, get: getSetting, reset: resetSetting, observe: observeSetting }) satisfies typeof Declared.settings;
export const terminals = Object.freeze({ open: openTerminal }) satisfies typeof Declared.terminals;
export const themes = Object.freeze({ register: registerTheme, get: getTheme, list: listThemes, defineColor }) satisfies typeof Declared.themes;
export const icons = Object.freeze({ define: defineIcon, override: overrideIcon, associate: associateIcons, registerPack: registerIconPack, listPacks: listIconPacks }) satisfies typeof Declared.icons;
export const windows = Object.freeze({ list: listWindows, get: getWindow, findView, open: openWindow, focus: focusWindow }) satisfies typeof Declared.windows;
export const files = Object.freeze({ readText: readFileText, list: listFiles, search: searchFiles, isIgnored: isFileIgnored, stat: statFile, writeText: writeFileText, createFile, createDirectory, rename: renameFile, copy: copyFile, reveal: revealFile, delete: deleteFile, watch: watchFiles }) satisfies typeof Declared.files;
export const processes = Object.freeze({ run: runProcess, spawn: spawnProcess }) satisfies typeof Declared.processes;
export const workspaces = Object.freeze({ get: getWorkspace, openFolder, replaceRoots: replaceWorkspaceRoots, pickFolder }) satisfies typeof Declared.workspaces;

export const buffers = Object.freeze({ create: createBuffer }) satisfies typeof Declared.buffers;
export const history = Object.freeze({ recent: historyRecent, record: historyRecord, forget: historyForget, order: historyOrder }) satisfies typeof Declared.history;
export const storage = Object.freeze({ read: storageRead, write: storageWrite, delete: storageDelete, list: storageList }) satisfies typeof Declared.storage;
export const recovery = Object.freeze({ list: recoveryList, retain: recoveryRetain, release: recoveryRelease, restore: recoveryRestore }) satisfies typeof Declared.recovery;

// Fails the build when the facade's exports and the public declaration
// diverge in either direction.
// (Values of a symbol type are the declaration's private brands.)
type ExportNames<T> = { [K in keyof T]: T[K] extends symbol ? never : K }[keyof T];
type Matches<A, B> = [A] extends [B] ? ([B] extends [A] ? true : never) : never;
const exportsMatch: Matches<ExportNames<typeof import("./knot")>, ExportNames<typeof Declared>> = true;
void exportsMatch;
