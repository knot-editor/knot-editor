import {
  activeBuffer,
  storageRead, storageWrite, storageDelete, storageList,
  historyRecent, historyRecord, historyOrder,
  recoveryList, recoveryRetain, recoveryRelease, recoveryRestore,
  observeWorkbench, closePresentation, openEditor, replaceWindows, captureLayout,
  openDocument, createUntitled, presentDocument, listDocuments, getDocument, setDocumentLanguage, listWindows, getWindow, openWindow, focusWindow, getWorkspace, openFolder, replaceWorkspaceRoots, pickFolder, readFileText, listFiles, searchFiles, isFileIgnored, statFile, watchFiles, openTerminal,
  invalidCommandArguments,
  unavailableCommand,
  keybinding,
  removeKeybinding,
  textBinding,
  removeTextBinding,
  registerKeymapPreset, getKeymapPreset, listKeymapPresets,
  platform,
  invokeCommand,
  registerCommand,
  registerViewCommand,
  selectedViewText,
  writeClipboardText,
  readClipboardText,
  showPicker,
  showInput,
  showConfirmation,
  showMessage,
  registerStatusItem,
  matchCandidates,
  registerCompletionProvider,
  registerView,
  registerLanguage,
  associateLanguage,
  listLanguages,
  getLanguage,
  listGrammars,
  defineSetting, setSettings, getSetting, resetSetting, observeSetting,
  registerTheme, getTheme, listThemes, defineColor,
  defineIcon, overrideIcon, associateIcons, registerIconPack, listIconPacks,
} from "knot:bootstrap";

export { platform };

export const keymaps = Object.freeze({
  bind: keybinding,
  unbind: removeKeybinding,
  bindText: textBinding,
  unbindText: removeTextBinding,
  registerPreset: registerKeymapPreset,
  getPreset: getKeymapPreset,
  listPresets: listKeymapPresets,
});

export const editor = Object.freeze({
  activeBuffer,
  registerCompletionProvider,
});

export const commands = Object.freeze({
  invalidArguments: invalidCommandArguments,
  invoke: invokeCommand,
  register: registerCommand,
  registerForView: registerViewCommand,
  unavailable: unavailableCommand,
});

export const workbench = Object.freeze({
  observe: observeWorkbench,
  close: closePresentation,
  editor: openEditor,
  replaceWindows,
  captureLayout,
  registerView,
  selectedText: selectedViewText,
  writeClipboardText,
  readClipboardText,
  showPicker,
  showInput,
  showConfirmation,
  showMessage,
  registerStatusItem,
});

export const completion = Object.freeze({ match: matchCandidates });
export const documents = Object.freeze({ list: listDocuments, get: getDocument, open: openDocument, createUntitled, present: presentDocument, setLanguage: setDocumentLanguage });
export const languages = Object.freeze({ register: registerLanguage, associate: associateLanguage, get: getLanguage, list: listLanguages, grammars: listGrammars });
export const settings = Object.freeze({ define: defineSetting, set: setSettings, get: getSetting, reset: resetSetting, observe: observeSetting });
export const terminals = Object.freeze({ open: openTerminal });
export const themes = Object.freeze({ register: registerTheme, get: getTheme, list: listThemes, defineColor });
export const icons = Object.freeze({ define: defineIcon, override: overrideIcon, associate: associateIcons, registerPack: registerIconPack, listPacks: listIconPacks });
export const windows = Object.freeze({ list: listWindows, get: getWindow, open: openWindow, focus: focusWindow });
export const files = Object.freeze({ readText: readFileText, list: listFiles, search: searchFiles, isIgnored: isFileIgnored, stat: statFile, watch: watchFiles });
export const workspaces = Object.freeze({ get: getWorkspace, openFolder, replaceRoots: replaceWorkspaceRoots, pickFolder });

export const history = Object.freeze({ recent: historyRecent, record: historyRecord, order: historyOrder });
export const storage = Object.freeze({ read: storageRead, write: storageWrite, delete: storageDelete, list: storageList });
export const recovery = Object.freeze({ list: recoveryList, retain: recoveryRetain, release: recoveryRelease, restore: recoveryRestore });
