// Solarized Dark and Light, from Ethan Schoonover's Solarized palette, with
// its canonical sixteen terminal colors.
import type { ThemeDefinition } from "knot";
import { syntax } from "../syntax.js";

const base03 = "#002b36";
const base02 = "#073642";
const base01 = "#586e75";
const base00 = "#657b83";
const base0 = "#839496";
const base1 = "#93a1a1";
const base2 = "#eee8d5";
const base3 = "#fdf6e3";
const yellow = "#b58900";
const orange = "#cb4b16";
const red = "#dc322f";
const magenta = "#d33682";
const violet = "#6c71c4";
const blue = "#268bd2";
const cyan = "#2aa198";
const green = "#859900";

const ansi = {
  "terminal.ansi.black": base02,
  "terminal.ansi.blue": blue,
  "terminal.ansi.brightBlack": base03,
  "terminal.ansi.brightBlue": base0,
  "terminal.ansi.brightCyan": base1,
  "terminal.ansi.brightGreen": base01,
  "terminal.ansi.brightMagenta": violet,
  "terminal.ansi.brightRed": orange,
  "terminal.ansi.brightWhite": base3,
  "terminal.ansi.brightYellow": base00,
  "terminal.ansi.cyan": cyan,
  "terminal.ansi.green": green,
  "terminal.ansi.magenta": magenta,
  "terminal.ansi.red": red,
  "terminal.ansi.white": base2,
  "terminal.ansi.yellow": yellow,
};

const accents = {
  "border.hover": blue,
  "error": red,
  "info": blue,
  "success": green,
  "text.accent": blue,
  "warning": yellow,
};

function solarizedSyntax(comment: string) {
  return syntax({
    comment,
    keyword: green,
    function: blue,
    special: red,
    type: yellow,
    string: cyan,
    constant: magenta,
    member: violet,
    link: blue,
  });
}

export const SOLARIZED_DARK: ThemeDefinition = {
  name: "Solarized Dark",
  appearance: "dark",
  colors: {
    ...ansi,
    ...accents,
    "background": "#00212b",
    "border": base02,
    "button.background": base02,
    "button.hover.background": "#0e4a59",
    "dropTarget.background": "#268bd240",
    "editor.background": base03,
    "editor.bar.background": "#00212b",
    "editor.bar.border": base02,
    "editor.bracketMatch": "#586e7560",
    "editor.cursor": base1,
    "editor.findMatch": "#e5c07b4d",
    "editor.findMatchCurrent": "#e5c07b99",
    "editor.foldPlaceholder": base01,
    "editor.foreground": base0,
    "editor.gutter.background": base03,
    "editor.lineHighlight": base02,
    "editor.lineNumber": base01,
    "editor.lineNumber.active": base1,
    "editor.markedText": "#83949699",
    "editor.selection": "#274642",
    "input.background": base03,
    "input.border": base02,
    "input.focused.border": blue,
    "list.group.background": base03,
    "list.hover.background": base02,
    "list.match.foreground": blue,
    "list.selected.background": "#005a6f",
    "list.selected.foreground": base2,
    "popover.background": "#00212b",
    "popover.border": base02,
    "scrim": "#00000080",
    "scrollbar.thumb": "#586e7580",
    "statusBar.background": "#00212b",
    "statusBar.border": base02,
    "statusBar.foreground": base01,
    "statusBar.item.hover.background": base02,
    "tab.active.background": base03,
    "tab.active.foreground": base1,
    "tab.bar.background": "#00212b",
    "tab.inactive.background": "#00212b",
    "tab.inactive.foreground": base01,
    "terminal.background": base03,
    "terminal.cursor": base1,
    "terminal.foreground": base0,
    "terminal.selection": "#274642",
    "icon": base1,
    "text": base1,
    "text.muted": base01,
  },
  syntax: solarizedSyntax(base01),
};

export const SOLARIZED_LIGHT: ThemeDefinition = {
  name: "Solarized Light",
  appearance: "light",
  colors: {
    ...ansi,
    ...accents,
    "background": base2,
    "border": "#ddd6c1",
    "button.background": "#e4ddc8",
    "button.hover.background": "#ddd6c1",
    "dropTarget.background": "#268bd233",
    "editor.background": base3,
    "editor.bar.background": base2,
    "editor.bar.border": "#ddd6c1",
    "editor.bracketMatch": "#93a1a14d",
    "editor.cursor": base01,
    "editor.findMatch": "#d19a6659",
    "editor.findMatchCurrent": "#d19a66b3",
    "editor.foldPlaceholder": base1,
    "editor.foreground": base00,
    "editor.gutter.background": base3,
    "editor.lineHighlight": base2,
    "editor.lineNumber": base1,
    "editor.lineNumber.active": base01,
    "editor.markedText": "#657b8399",
    "editor.selection": base2,
    "input.background": base3,
    "input.border": "#ddd6c1",
    "input.focused.border": blue,
    "list.group.background": base3,
    "list.hover.background": "#e4ddc8",
    "list.match.foreground": blue,
    "list.selected.background": "#dfca88",
    "list.selected.foreground": base01,
    "popover.background": base3,
    "popover.border": "#ddd6c1",
    "scrim": "#00000033",
    "scrollbar.thumb": "#93a1a180",
    "statusBar.background": base2,
    "statusBar.border": "#ddd6c1",
    "statusBar.foreground": base1,
    "statusBar.item.hover.background": "#e4ddc8",
    "tab.active.background": base3,
    "tab.active.foreground": base01,
    "tab.bar.background": base2,
    "tab.inactive.background": base2,
    "tab.inactive.foreground": base1,
    "terminal.background": base3,
    "terminal.cursor": base01,
    "terminal.foreground": base00,
    "terminal.selection": base2,
    "icon": base01,
    "text": base01,
    "text.muted": base1,
  },
  syntax: solarizedSyntax(base1),
};
