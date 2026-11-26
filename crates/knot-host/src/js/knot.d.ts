/**
 * Public API version 1 reference for module "knot".
 * See docs/extension-api/README.md for package admission and authoring.
 *
 * Offsets, revisions and generations are nonnegative safe integers. Recognized
 * fields are validated without coercion; unknown option/data fields are ignored.
 * Invalid types raise TypeError; numeric limits and coordinates raise RangeError.
 * Host failures are Error instances with a stable code (see HostError).
 * Accepted requests settle once at native acknowledgement. Cancellation before
 * mutation prevents it; rejection after a commit does not imply rollback.
 * Public snapshots, contexts, proxies and callback payloads are read-only.
 */
/**
 * Writes to the standard error of the process running Knot, prefixed with the
 * level. Strings print as they are; other values print as JSON.
 */
interface Console {
  debug(...values: unknown[]): void;
  log(...values: unknown[]): void;
  info(...values: unknown[]): void;
  warn(...values: unknown[]): void;
  error(...values: unknown[]): void;
}
declare var console: Console;

/**
 * Node-compatible timers. Each callback runs as its own turn once its delay
 * has passed and every earlier queued callback has run, so a delay is a
 * minimum. A delay below 1 or above 2^31 - 1 ms becomes 1 ms; extra
 * arguments pass to the callback. An interval waits again only after its
 * callback returns. Unloading the extension cancels every timer. `ref()`
 * and `unref()` only change `hasRef()`: there is no process to keep alive.
 * These declarations merge with `@types/node` in package tests.
 */
declare namespace NodeJS {
  interface Timeout {
    /** Cancels the timeout. */
    close(): this;
    hasRef(): boolean;
    ref(): this;
    unref(): this;
    /** Restarts the delay, reactivating a timeout that already fired;
     * a cleared timer stays cleared. */
    refresh(): this;
    /** The numeric ID, which `clearTimeout` and `clearInterval` accept. */
    [Symbol.toPrimitive](): number;
  }
  interface Immediate {
    hasRef(): boolean;
    ref(): this;
    unref(): this;
  }
}
declare function setTimeout<TArgs extends any[]>(
  callback: (...args: TArgs) => void, delay?: number, ...args: TArgs): NodeJS.Timeout;
declare function setInterval<TArgs extends any[]>(
  callback: (...args: TArgs) => void, delay?: number, ...args: TArgs): NodeJS.Timeout;
/** Runs the callback in a later turn, without waiting for time to pass. */
declare function setImmediate<TArgs extends any[]>(
  callback: (...args: TArgs) => void, ...args: TArgs): NodeJS.Immediate;
declare function clearTimeout(timeout: NodeJS.Timeout | string | number | undefined): void;
declare function clearInterval(timeout: NodeJS.Timeout | string | number | undefined): void;
declare function clearImmediate(immediate: NodeJS.Immediate | undefined): void;

declare module "knot" {
  const windowIdentity: unique symbol;
  const viewIdentity: unique symbol;
  const targetAuthority: unique symbol;
  const bufferIdentity: unique symbol;
  /** Opaque live window identity. Do not fabricate, serialize or persist it. */
  export type WindowId = string & { readonly [windowIdentity]: true };
  /** Opaque live extension-addressable view identity; not a native handle. */
  export type ViewId = string & { readonly [viewIdentity]: true };

  const documentIdentity: unique symbol;
  const liveWindowIdentity: unique symbol;
  const livePaneIdentity: unique symbol;
  const presentationIdentity: unique symbol;
  const viewNodeIdentity: unique symbol;
  const rootIdentity: unique symbol;
  /** Application-allocated identity, preserved across Save As and presentations.
   * Reopening allocates a new identity. A document restored from automatic
   * recovery keeps its identity across restart, so a session record may store
   * it; other identities are restart-local. */
  export type DocumentId = string & { readonly [documentIdentity]: true };
  /** Root identity retained while its normalized URI remains installed. */
  export type RootId = string & { readonly [rootIdentity]: true };
  /** Discoverable live targets have no command invocation authority.
   * Do not fabricate, copy, serialize or persist target objects. */
  export interface WindowTarget { readonly [liveWindowIdentity]: true; }
  export interface PaneTarget {
    readonly [livePaneIdentity]: true;
    readonly window: WindowTarget;
  }
  /** Exact tab presentation. Replacement, pane/window transfer, or closure
   * invalidates this target; it cannot select a replacement presentation. */
  export interface ViewTarget {
    readonly [presentationIdentity]: true;
    readonly window: WindowTarget;
    readonly pane: PaneTarget;
  }
  /** Exact view inside a tab: the tab's root view or any view nested in it.
   * Closure of the view or its tab invalidates this target. */
  export interface ViewNodeTarget {
    readonly [viewNodeIdentity]: true;
    readonly window: WindowTarget;
  }
  /** One view of a tab's view tree. */
  export interface ViewNodeInfo {
    readonly target: ViewNodeTarget;
    /** `editor`, `terminal`, `tree`, `text`, `composite`, or `popover`. */
    readonly kind: "editor" | "terminal" | "tree" | "text" | "composite" | "popover";
    /** The views this one contains, in layout order; empty for a leaf. An
     * editor's children are the popovers floating over it, whose own
     * children are their layouts' members. */
    readonly children: readonly ViewNodeInfo[];
    /** An editor's document; null for a buffer outside document identity and
     * for other views. */
    readonly document: DocumentId | null;
    /** An editor's buffer, otherwise null. */
    readonly buffer: Buffer | null;
    /** An editor's language ID, otherwise null. */
    readonly language: string | null;
    /** Whether an editor refuses edits, otherwise null. */
    readonly readOnly: boolean | null;
  }
  export interface DocumentInfo {
    readonly id: DocumentId;
    readonly title: string;
    readonly kind: "untitled" | "destination" | "persisted";
    readonly resourceUri: string | null;
    readonly dirty: boolean;
    readonly revision: number;
    readonly buffer: Buffer;
    /** Language ID, set explicitly or detected from the file name and first
     * line. "plaintext" when nothing matched or the ID is not registered; a
     * kept explicit ID reappears once something registers it. Changes
     * publish documentChanged. */
    readonly language: string;
    /** The indentation `editor.insertSpaces` and `editor.tabSize` resolve to
     * for this document. Writes to either publish documentChanged. */
    readonly indentation: { readonly style: "spaces" | "tabs"; readonly size: number };
  }
  export interface PresentationInfo {
    readonly target: ViewTarget;
    readonly kind: "document" | "terminal" | "composite" | "tool";
    /** The tab's root view and, beneath it, the views it contains. A composite
     * view lists its views in layout order. */
    readonly root: ViewNodeInfo;
    readonly document: DocumentId | null;
    readonly buffer: Buffer | null;
    /** Stable contribution ID for a tool presentation, otherwise null. */
    readonly tool: string | null;
    /** A terminal's current directory as a file URI, otherwise null. */
    readonly directory: string | null;
  }
  export interface PaneInfo {
    readonly target: PaneTarget;
    readonly tabs: readonly PresentationInfo[];
    readonly selected: ViewTarget;
  }
  export interface WorkspaceRoot { readonly id: RootId; readonly uri: string; }
  export interface WorkspaceInfo {
    readonly window: WindowTarget;
    readonly roots: readonly WorkspaceRoot[];
    readonly generation: number;
  }
  export interface WindowInfo {
    readonly target: WindowTarget;
    readonly panes: readonly PaneInfo[];
    /** Actual focused tab surface; null for focus outside tabs. */
    readonly currentTarget: ViewTarget | null;
    /** The editor tab a status display describes: the focused editor, else the
     * last focused editor while its tab is shown; null while a terminal has
     * focus or before any editor was. */
    readonly statusTarget: ViewTarget | null;
    /** The focused leaf view of the focused tab, such as the editor in a
     * composite view that has focus; null for focus outside tabs. */
    readonly focusedView: ViewNodeTarget | null;
    /** The editor view a status display describes, at any depth: the focused
     * editor view, else the last focused one while its tab is shown; null
     * while a terminal has focus or before any editor was. */
    readonly statusView: ViewNodeTarget | null;
    readonly workspace: WorkspaceInfo;
  }
  /** Current metadata queries do not grant future target validity. Valid during
   * activation, including before any window exists. */
  export interface DocumentDestinationOptions {
    /** Captured at request admission. Omission uses the command's window, or the
     * active window outside a command; missing context rejects. */
    readonly target?: WindowTarget | PaneTarget | ViewTarget;
  }
  export interface OpenDocumentOptions extends DocumentDestinationOptions {
    /** Open a document not already shown in the destination window as that
     * pane's preview tab, replacing its current preview in place. A preview
     * becomes a regular tab when edited, pinned, double-clicked, dragged, or
     * opened again without `preview`. With the `workbench.previewTabs`
     * setting off, a preview request opens a regular tab. */
    readonly preview?: boolean;
    /** Move focus to the opened tab. Defaults to true; false opens the tab
     * and leaves the focus where it was. */
    readonly focus?: boolean;
  }
  export interface DocumentPresentation {
    readonly document: DocumentInfo;
    readonly presentation: PresentationInfo;
  }
  export interface TrackedDocument {
    onDidChange(listener: (now: DocumentInfo, before: DocumentInfo) => void | Promise<void>): Disposable;
    onDidClose(listener: () => void | Promise<void>): Disposable;
  }
  export const documents: Readonly<{
    list(): Promise<readonly DocumentInfo[]>;
    get(id: DocumentId): Promise<DocumentInfo | null>;
    /** File-only Open through the registered provider. Missing files become dirty
     * destinations; directories reject. Reuses an existing tab in the destination
     * window, otherwise creates one in the captured pane. Focus changes during I/O
     * never retarget the request. Resolves after commit without awaiting observers. */
    open(uri: string, options?: OpenDocumentOptions): Promise<DocumentPresentation>;
    /** Creates a clean, empty ordinary document together with its native tab. */
    createUntitled(options?: DocumentDestinationOptions): Promise<DocumentPresentation>;
    /** Presents a shared live document, reusing its tab in the destination window.
     * Rejects DocumentNotFound after final closure. */
    present(id: DocumentId, options?: DocumentDestinationOptions): Promise<DocumentPresentation>;
    /**
     * Follows every open document. The listener runs for each document already
     * open, then for each one opened later, and may register `onDidChange` and
     * `onDidClose` on the tracked document. Callbacks are delivered in
     * workbench-observation order, one at a time. `onDidChange` reports a
     * change of kind, resource, dirty state, or language with the document
     * now and before; revision-only changes are silent, so read text through
     * the buffer. `onDidClose` fires when the document closes or when the
     * returned disposable ends tracking. Does not wait for `launched`.
     */
    track(listener: (document: DocumentInfo, tracked: TrackedDocument) => void | Promise<void>): Promise<Disposable>;
    /** Replaces the text of a document that has a file with the file's, as one
     * line-diff edit that Undo takes back, and marks it clean. Resolves once the
     * reload commits.
     * @throws HostError with DocumentNotFound, ResourceNotFound for a document
     * without a file, ProviderFailure, or Cancelled when the document changed
     * while the file was read. */
    reload(id: DocumentId): Promise<void>;
    /** Saves the document as Save and Save As do, without a dialog. With `uri`
     * the document is written to that file, replacing what is there, and
     * becomes that file's document; without it, it is saved to its own file.
     * Resolves once the text is written. Rejects for a document without a file
     * when `uri` is omitted.
     * @throws TypeError for a non-text `uri`; HostError with DocumentNotFound,
     * InvalidUri, ResourceNotFound, ResourceAlreadyExists when another open
     * document is that file, ResourceWrongKind, or ProviderFailure. */
    save(id: DocumentId, options?: { readonly uri?: string }): Promise<void>;
    /** Makes a language ID the document's explicit language, or returns it to
     * detection with null. An unregistered ID is accepted and kept, reporting
     * and painting as "plaintext" until a registration provides it. Open, Save As, and restore re-detect
     * only a document whose language is not explicit.
     * @throws TypeError for a non-text language; HostError with
     * DocumentNotFound or Cancelled. */
    setLanguage(id: DocumentId, language: string | null): Promise<void>;
  }>;

  export interface TerminalOptions extends DocumentDestinationOptions {
    /** Existing local directory URI the shell starts in. Omission starts it in
     * the home directory; choosing a better one is the caller's policy. */
    readonly cwd?: string;
    /** Where the terminal opens. "editor" (the default) opens it in the last
     * focused center pane. A container view's ID opens it beside the captured
     * terminal when that is in the container, else in the container's last
     * focused pane, which it shows. */
    readonly location?: "editor" | (string & {});
  }
  export const terminals: Readonly<{
    /** Opens and focuses a terminal tab where `location` says. Resolves after
     * commit.
     * @throws TypeError for a non-text cwd or location; HostError with InvalidUri for a
     * malformed or non-file URI, ResourceNotFound, ResourceWrongKind,
     * ProviderFailure, InvalidTarget, ContextUnavailable, or Cancelled. */
    open(options?: TerminalOptions): Promise<PresentationInfo>;
  }>;

  /** A language as registered: its ID and the patterns detection matches. */
  export interface LanguageInfo {
    /** LSP languageId, such as "rust", "typescriptreact", or "shellscript". */
    readonly id: string;
    /** Display name, such as "TypeScript JSX". */
    readonly name: string;
    /** Compiled-in grammar ID; see languages.grammars. */
    readonly grammar: string;
    /** Extensions without the leading dot, matched case-insensitively against
     * the end of a file name, so "d.ts" is possible. The longest match wins,
     * then the latest registration claiming it. */
    readonly fileExtensions: readonly string[];
    /** Exact file names, such as "Makefile"; they win over extensions. */
    readonly fileNames: readonly string[];
    /** Shebang programs, such as "python3"; "#!/usr/bin/env" is looked
     * through, and a versioned program also matches its unversioned name. */
    readonly interpreters: readonly string[];
  }
  /** File patterns a language claims for detection; see LanguageInfo. */
  export interface LanguagePatterns {
    readonly fileExtensions?: readonly string[];
    readonly fileNames?: readonly string[];
    readonly interpreters?: readonly string[];
  }
  /** Comment tokens, without a trailing space. */
  export interface LanguageComments {
    /** Every line comment token. Commenting inserts the first and one space;
     * uncommenting strips the longest token that matches and one following
     * space, so Rust lists "//", "///", and "//!". */
    readonly line?: readonly string[];
    /** Block comment delimiters. A language with no line comment comments
     * by wrapping each line in them. */
    readonly block?: readonly [open: string, close: string];
  }
  /** A bracket pair for matching, auto-closing, and surrounding. A pair whose
   * open equals its close, such as quotes, never takes part in matching. */
  export interface BracketPair {
    readonly open: string;
    readonly close: string;
    /** Whether typing open inserts close; defaults to true. */
    readonly autoClose?: boolean;
    /** Whether Enter between the pair opens an indented middle line;
     * defaults to true unless open equals close. */
    readonly newline?: boolean;
    /** Syntax at the cursor where auto-closing is suppressed: inside a node
     * the highlight query captures as `@string` or `@comment`, or a dotted
     * child of either. */
    readonly notIn?: readonly ("string" | "comment")[];
  }
  export interface LanguageDefinition extends LanguagePatterns {
    readonly id: string;
    readonly name: string;
    readonly grammar: string;
    /** Tree-sitter queries using nvim-treesitter capture names. Injections
     * name languages by ID, or by a registered file extension such as "py".
     * Indents use nvim-treesitter's `@indent.*` captures and properties, plus
     * `(#set! indent.after_end)`, which limits an `@indent.dedent` to lines
     * after its node, and may add the `has-parent?`, `not-has-parent?`,
     * `kind-eq?`, and `not-kind-eq?` node predicates. Typing reindents a line
     * when a closing capture's match reaches the caret. A language without
     * an indents query keeps the previous line's indentation. Folds capture
     * the nodes whose lines fold as `@fold`: a node spanning several lines is
     * a region from its first line through its last, and a last line that
     * starts with a closing bracket stays visible. A language without a folds
     * query folds by indentation. */
    readonly queries: Readonly<{
      highlights: string;
      injections?: string;
      indents?: string;
      folds?: string;
    }>;
    readonly comments?: LanguageComments;
    /** One list for matching, auto-closing, and surrounding. Matching pairs
     * the bracket tokens of one syntax node; typing an opener inserts its
     * closer, and typing over a selection wraps it. */
    readonly brackets?: readonly BracketPair[];
    /** Characters before which auto-closing applies, besides whitespace and
     * the end of the line. */
    readonly autoCloseBefore?: string;
  }
  /**
   * Syntax highlighting languages. The latest registration of an ID defines
   * the language, and the latest registration or association claiming a
   * pattern decides detection; disposal or teardown reveals the previous one. Personal config
   * replaces a package's language from post-init, which runs after packages
   * start. Every change re-detects open documents whose language is not
   * explicit. Register in activation, before startup restores windows, so
   * restored documents highlight from their first frame.
   */
  export const languages: Readonly<{
    /**
     * Register a lifecycle-owned language. Queries compile against the grammar
     * before resolution.
     * @throws TypeError for invalid fields, including empty comment or
     * bracket tokens and tokens with surrounding whitespace; HostError with
     * UnknownGrammar, InvalidQuery (details: query "highlights",
     * "injections", "indents", or "folds", message, and the 0-indexed row and column), or
     * Cancelled. */
    register(definition: LanguageDefinition): Promise<Disposable>;
    /**
     * Claim file patterns for a language ID without defining the language, so
     * personal config can redirect detection, such as "mdx" files to
     * "markdown", without replacing a package's registration. The claim takes
     * part in detection like a registration's: the latest claim of a pattern
     * wins, and disposal or teardown reveals the previous one. An unregistered
     * ID paints as "plaintext". Associations do not appear in list.
     * @throws TypeError for invalid fields; HostError with Cancelled. */
    associate(id: string, patterns: LanguagePatterns): Promise<Disposable>;
    /** The registration that defines `id`, queries and editing data
     * included, so a derived language spreads it and changes a key. Pattern
     * lists and every query are present; editing fields are as registered. */
    get(id: string): Promise<LanguageDefinition | null>;
    /** The language each registered ID resolves to, sorted by ID. */
    list(): Promise<readonly LanguageInfo[]>;
    /** Every compiled-in grammar ID, sorted. */
    grammars(): Promise<readonly string[]>;
  }>;

  /**
   * The value type of every defined setting key. Native keys are declared
   * here; a package declares its own keys by merging into this interface:
   * `declare module "knot" { interface Settings { "lsp.timeout.launch": number } }`.
   */
  export interface Settings {
    /** Text size in pixels of editors and text frames, and of terminals
     * when read in the { view: "terminal" } scope, so a terminal-scoped
     * value sizes terminals separately. Native default 14; views use at
     * least 6 and at most 72. */
    "font.size": number;
    /** Primary font family of editors, text frames, text inputs, and
     * terminals; scope it to { view: "terminal" } or a language to vary it.
     * Text inputs read the unscoped value. Native default is the platform's
     * code font, "Menlo" on macOS. */
    "font.family": string;
    /** Families tried for glyphs the primary family lacks. Native default
     * other common code fonts, then the platform's Arabic, CJK, and emoji
     * fonts. */
    "font.fallbacks": readonly string[];
    /** Base font weight; bold text stays at 700. Native default 400; views
     * use at least 100 and at most 900. */
    "font.weight": number;
    /** Row height of editors, text frames, and terminals as a multiple of
     * the font's ascent plus descent, rounded up to whole physical pixels.
     * Native default 1.2; views use at least 0.8 and at most 3. */
    "font.lineHeight": number;
    /** Whether ligatures form; false turns off the calt and liga OpenType
     * features. Native default true. */
    "font.ligatures": boolean;
    /** OpenType features applied after font.ligatures: "ss01" turns one on,
     * "-calt" off, and "cv05=2" sets a value. Malformed entries are ignored.
     * Native default empty. */
    "font.features": readonly string[];
    /** Columns per tab stop in editor views. Native default 4; editors use
     * at least 1 and at most 32. */
    "editor.tabSize": number;
    /** Whether indenting inserts spaces rather than a tab. A document whose
     * own indentation is detected from its text when it opens follows that
     * instead. Native default true. */
    "editor.insertSpaces": boolean;
    /** Whether typing an opening bracket or quote inserts its closer where
     * the language's bracket pairs allow, and typing that closer or Backspace
     * between the pair steps over or deletes it while the cursor stays
     * inside. Native default true. */
    "editor.autoClosingBrackets": boolean;
    /** Whether typing an opening bracket or quote over a selection wraps the
     * selection in the pair. Native default true. */
    "editor.autoSurround": boolean;
    /** Whether editor views wrap long lines at the viewport width. Toggle
     * Soft Wrap writes it scoped to one document, and the bundled Markdown
     * package sets it for { language: "markdown" }. Native default false. */
    "editor.softWrap": boolean;
    /** Whether editor views scroll until the last row is the first visible
     * one, rather than stopping with it at the bottom edge. Native default
     * true. */
    "editor.scrollBeyondLastLine": boolean;
    /** Line numbers in the editor gutter: "on", "off", or "relative", which
     * numbers other lines by their distance from the cursor's line. Native
     * default "on"; other text means "on". */
    "editor.lineNumbers": "on" | "off" | "relative";
    /** Whether the gutter has a fold lane. Native default true. */
    "editor.folding": boolean;
    /** When the fold lane shows chevrons on foldable lines: "mouseover", while
     * the pointer is over the gutter, or "always". A collapsed fold always
     * shows one. Native default "mouseover"; other text means "mouseover". */
    "editor.showFoldingControls": "mouseover" | "always";
    /** Whether saving trims trailing spaces and tabs from every line except
     * the cursor's: "all" or "off". Native default "all". */
    "files.trimTrailingWhitespace": "all" | "off";
    /** Whether saving ends a non-empty file with a line terminator. Native
     * default true. */
    "files.insertFinalNewline": boolean;
    /** Which theme applies: theme.dark's, theme.light's, or the one matching
     * the system appearance. Native default "dark"; other text means "dark". */
    "theme.mode": "dark" | "light" | "system";
    /** The theme name used in dark mode. Native default "Knot Islands Dark". */
    "theme.dark": string;
    /** The theme name used in light mode. Native default "Knot Islands Light". */
    "theme.light": string;
    /** Corner radius of the center and dock cards. From 0 to 24; native default -1 follows the theme. */
    "chrome.cardRadius": number;
    /** Gap between cards, which is also their resize handle. From 0 to 24; native default -1 follows the theme. */
    "chrome.cardGap": number;
    /** Margin around the cards at the window edges. From 0 to 24; native default -1 follows the theme. */
    "chrome.cardInset": number;
    /** Width of the card border, painted in the `border` color. From 0 to 4; native default -1 follows the theme. */
    "chrome.cardBorder": number;
    /** Height of tab bars and dock headers. From 20 to 48; native default -1 follows the theme. */
    "chrome.tabBarHeight": number;
    /** Width of the tool stripes. From 24 to 64; native default -1 follows the theme. */
    "chrome.stripeWidth": number;
    /** Font weight of a dock header's tab titles. From 100 to 900; native default -1 follows the theme. */
    /** Thickness of the accent line under the active tab. From 0 to 6; native default -1 follows the theme. */
    "chrome.accentWidth": number;
    /** Corner radius of a tab, so the active tab can be a rounded pill. From 0 to 12; native default -1 follows the theme. */
    "chrome.tabRadius": number;
    /** Space above and below the tabs inside the tab bar. From 0 to 12; native default -1 follows the theme. */
    "chrome.tabInset": number;
    /** The icon pack that restyles icons; empty uses the default icons.
     * Native default "". */
    "icons.pack": string;
    /** The keymap preset that supplies default bindings. Native default
     * "knot"; an unregistered name selects "knot". */
    "keymap.preset": string;
    /** Whether documents browsed from the explorer or search open as preview
     * tabs that the next preview replaces. Native default true. */
    "workbench.previewTabs": boolean;
    /** Where the bottom side sits: "contained" under the center between the
     * left and right sides, or "full" across the window width under them.
     * Native default "contained". */
    "workbench.bottomDockLayout": "contained" | "full";
    /** Whether the left and right window edges show stripes of tool
     * buttons. Native default true. */
    "workbench.toolStripes": boolean;
    /** Whether windows show the status bar. Native default true. */
    "statusBar.visible": boolean;
    /** IDs of status items the bar skips. Hidden items keep running, so a
     * package that registered one is unaffected. Native default empty. */
    "statusBar.hiddenItems": readonly string[];
    /** Whether notices pop up as cards in the window; with false they are
     * only recorded. Native default true. */
    "notifications.popups": boolean;
    /** Where popups stack from. Native default "bottomRight". */
    "notifications.position": "bottomRight" | "bottomLeft" | "topRight" | "topLeft" | "topCenter";
    /** Seconds before a popup that dismisses itself hides. Native default 8. */
    "notifications.timeout": number;
    /** How many popups show at once; the rest collapse into a count. Native
     * default 3. */
    "notifications.maxVisible": number;
    /** Whether new terminals report mouse input to the running program.
     * Native default true. */
    "terminal.mouseReportingEnabled": boolean;
    /** The cursor shape new terminals start with. Native default "block";
     * a program's own shape still wins. */
    "terminal.cursorStyle": "block" | "bar" | "underline" | "hollow";
    /** Gitignore-style patterns of entries that `files.list` never returns.
     * Native default `.git`, `.hg`, `.svn`, `.jj`, `CVS`,
     * `.DS_Store`, and `Thumbs.db`. */
    "files.hidden": readonly string[];
    /** Gitignore-style patterns of entries that `files.list` marks `ignored`,
     * in addition to the repository ignore files. Native default empty. */
    "files.ignored": readonly string[];
    /** Whether `.gitignore`, `.ignore`, `.git/info/exclude`, and the global
     * Git excludes file mark entries ignored. Native default true. */
    "files.respectGitignore": boolean;
    /** Whether Knot watches open documents for changes made outside it, and
     * rechecks them when the application becomes active. Native default true. */
    "files.watchDocuments": boolean;
    /** Whether a clean document whose file changed outside Knot reloads at
     * once. When false it is marked and, once focused, asks Reload or Keep
     * like a document with unsaved changes. Native default true. */
    "files.autoReload": boolean;
    /** How pickers and completion lists match, in order of preference:
     * "prefix", "substring", "flex" (fuzzy), or "orderless" (space-separated
     * parts in any order). A candidate matches if any style matches it, and
     * earlier styles rank first. Unknown names are ignored. Native default
     * ["prefix", "flex"]. */
    "completion.styles": readonly string[];
  }
  export type SettingKey = keyof Settings & string;
  /** "integer" accepts safe integers; "number" accepts any finite number;
   * "strings" accepts an array of strings. */
  export type SettingType = "boolean" | "integer" | "number" | "string" | "strings";
  export interface SettingDefinition<K extends SettingKey = SettingKey> {
    /** Flat dotted name saying what the key sets, such as "lsp.timeout.launch". */
    readonly key: K;
    readonly type: SettingType;
    /** Applies when no value matches a context; must be of `type`. */
    readonly default: Settings[K];
    /** Shown by a future settings UI. */
    readonly description: string;
  }
  /**
   * Facts that must all match where a value applies, or that describe the
   * context of a read. `language` is a document's effective language ID;
   * `view` is a native surface kind ("editor", "terminal", "bar") or a contributed
   * view ID, as for keybindings; `document` is a document's ID, whose values
   * the application drops when the document closes.
   */
  export interface SettingsScope {
    readonly language?: string | null;
    readonly view?: string | null;
    readonly document?: DocumentId | null;
  }
  /**
   * One application-wide store of settings shared by personal config and every
   * package. A write replaces the value for exactly its key and scope, and a
   * later write replaces an earlier one whoever made it, so personal overrides
   * belong in post-init. Values live in memory only: config and packages write
   * them again on every launch, and teardown and Restart Extension remove none.
   *
   * A read resolves the matching value with the most scope facts, ties going
   * to the latest write; with none, the definition's default applies. A value
   * may be written before its key is defined: define validates it, drops a
   * value of the wrong type with a note in the extension report, and keys never
   * defined stay inert. Writes resolve after the store commits.
   * @throws TypeError for invalid keys, values, scopes or definitions.
   */
  export const settings: Readonly<{
    /** Define a lifecycle-owned key. Teardown removes the definition and keeps
     * its values, so a restarted package defines it again.
     * @throws HostError with DuplicateSetting when native code or a live
     * lifecycle defined the key, or Cancelled. */
    define<K extends SettingKey>(definition: SettingDefinition<K>): Promise<void>;
    /** Write one value, or several at once; a value of the wrong type for a
     * defined key rejects the whole call and writes nothing.
     * @throws HostError with SettingTypeMismatch (details: key, expected) or
     * Cancelled. */
    set<K extends SettingKey>(key: K, value: Settings[K], scope?: SettingsScope): Promise<void>;
    set(values: Partial<Settings>, scope?: SettingsScope): Promise<void>;
    /** The value for a context, or the default when none matches; undefined
     * for a key that is not defined. */
    get<K extends SettingKey>(key: K, facts?: SettingsScope): Promise<Settings[K] | undefined>;
    /** Remove the value for exactly this key and scope, revealing the next
     * match. Resetting a missing value succeeds. */
    reset(key: SettingKey, scope?: SettingsScope): Promise<void>;
    /** Call `listener` with the key after every write or reset of it, in any
     * scope, including this caller's own. Call get for the contexts that
     * matter. Notifications reach one listener in order, and it stops with
     * its lifecycle. */
    observe(key: SettingKey, listener: (key: SettingKey) => void | Promise<void>): Promise<Disposable>;
  }>;
  /** Which defaults fill the colors a theme leaves out. */
  export type ThemeAppearance = "dark" | "light";
  /**
   * The name of a theme color, native or defined with
   * `themes.defineColor`, such as "error", "text.muted", or "diff.add". A key
   * nobody defines paints as interface text, so a contribution may name a key
   * whose definition arrives later.
   */
  export type ColorKey = string;
  /**
   * Colors by key, each "#rrggbb" or "#rrggbbaa". Native keys are declared
   * here; a package declares the keys it defines by merging into this
   * interface. A theme may set any key: one nobody defines is kept and stays
   * inert until something defines it.
   */
  export interface ThemeColors {
    readonly [key: string]: string | undefined;
    /** Window and pane background. */
    readonly "background"?: string;
    /** Dividers and panel borders. */
    readonly "border"?: string;
    /** A resizable divider under the pointer. */
    readonly "border.hover"?: string;
    /** Inline action buttons. */
    readonly "button.background"?: string;
    /** An inline action button under the pointer. */
    readonly "button.hover.background"?: string;
    /** The cards of the center and the docks. */
    readonly "card.background"?: string;
    /** The outline of the focused pane. */
    readonly "card.focusedBorder"?: string;
    /** Lines a text added, in the gutter and in diffs. */
    readonly "diff.added"?: string;
    /** Lines a diff view's other side lacks, behind the text. */
    readonly "diff.added.background"?: string;
    /** Lines a text changed, in the gutter and in diffs. */
    readonly "diff.modified"?: string;
    /** Lines a diff view's sides differ in, behind the text. */
    readonly "diff.modified.background"?: string;
    /** Lines a text removed, in the gutter and in diffs. */
    readonly "diff.removed"?: string;
    /** Lines only a diff view's original side has, behind the text. */
    readonly "diff.removed.background"?: string;
    /** Where a dragged tab would land. */
    readonly "dropTarget.background"?: string;
    /** Text editor background. */
    readonly "editor.background"?: string;
    /** Bars extensions open above an editor's text, such as Find. */
    readonly "editor.bar.background"?: string;
    /** The line below each editor bar. */
    readonly "editor.bar.border"?: string;
    /** Background of the bracket at the cursor and its partner. */
    readonly "editor.bracketMatch"?: string;
    /** Text cursor. */
    readonly "editor.cursor"?: string;
    /** Background of each text match of an open Find bar. */
    readonly "editor.findMatch"?: string;
    /** Background of the match Find has selected. */
    readonly "editor.findMatchCurrent"?: string;
    /** Background of the range Find in Selection limits its search to. */
    readonly "editor.findScope"?: string;
    /** The placeholder after the first line of a collapsed fold. */
    readonly "editor.foldPlaceholder"?: string;
    /** Unhighlighted editor text. */
    readonly "editor.foreground"?: string;
    /** The editor gutter beside the text. */
    readonly "editor.gutter.background"?: string;
    /** Background of the cursor's line while nothing is selected. */
    readonly "editor.lineHighlight"?: string;
    /** Line numbers. */
    readonly "editor.lineNumber"?: string;
    /** The cursor line's number. */
    readonly "editor.lineNumber.active"?: string;
    /** Underline of text being composed by an input method. */
    readonly "editor.markedText"?: string;
    /** Selected text. */
    readonly "editor.selection"?: string;
    /** Background of the other occurrences of the selected text. */
    readonly "editor.selectionHighlight"?: string;
    /** Background of the occurrences of the word at the cursor. */
    readonly "editor.wordHighlight"?: string;
    /** Errors and failures. */
    readonly "error"?: string;
    /** Icons that choose no color. */
    readonly "icon"?: string;
    /** Informational markers. */
    readonly "info"?: string;
    /** Text fields in editor bars. */
    readonly "input.background"?: string;
    /** Border of a text field. */
    readonly "input.border"?: string;
    /** Border of the text field with focus. */
    readonly "input.focused.border"?: string;
    /** Group headers in trees. */
    readonly "list.group.background"?: string;
    /** A list or tree row under the pointer. */
    readonly "list.hover.background"?: string;
    /** Characters a filter query matched in a list row. */
    readonly "list.match.foreground"?: string;
    /** The selected list or tree row. */
    readonly "list.selected.background"?: string;
    /** Text of the selected list or tree row. */
    readonly "list.selected.foreground"?: string;
    /** Notification popup cards. */
    readonly "notification.background"?: string;
    /** Border of notification popup cards. */
    readonly "notification.border"?: string;
    /** The message of a notification popup. */
    readonly "notification.description.foreground"?: string;
    /** The icon of an error notification. */
    readonly "notification.error"?: string;
    /** The title of a notification popup. */
    readonly "notification.foreground"?: string;
    /** The icon of an informational notification. */
    readonly "notification.info"?: string;
    /** The icon of a warning notification. */
    readonly "notification.warning"?: string;
    /** Pickers, the command palette, completion, and reports. */
    readonly "popover.background"?: string;
    /** Border of popovers. */
    readonly "popover.border"?: string;
    /** Dims the window behind a modal surface. */
    readonly "scrim"?: string;
    /** Scrollbar thumb. */
    readonly "scrollbar.thumb"?: string;
    /** The status bar below the workbench. */
    readonly "statusBar.background"?: string;
    /** The line above the status bar. */
    readonly "statusBar.border"?: string;
    /** Status bar items that choose no color. */
    readonly "statusBar.foreground"?: string;
    /** A status bar item that runs a command, under the pointer. */
    readonly "statusBar.item.hover.background"?: string;
    /** The button of the focused tool window in a stripe. */
    readonly "stripe.selected.background"?: string;
    /** The line between groups of tool buttons in a stripe. */
    readonly "stripe.separator"?: string;
    /** The button of a tool window that shows without focus. */
    readonly "stripe.shown.background"?: string;
    /** Success markers. */
    readonly "success"?: string;
    /** The line under the active tab. */
    readonly "tab.accent"?: string;
    /** The active tab. */
    readonly "tab.active.background"?: string;
    readonly "tab.active.border"?: string;
    /** Title of the active tab. */
    readonly "tab.active.foreground"?: string;
    readonly "tab.active.unfocused.background"?: string;
    readonly "tab.active.unfocused.border"?: string;
    /** The tab bar behind the tabs. */
    readonly "tab.bar.background"?: string;
    /** Inactive tabs. */
    readonly "tab.inactive.background"?: string;
    /** Titles of inactive tabs. */
    readonly "tab.inactive.foreground"?: string;
    /** ANSI black. */
    readonly "terminal.ansi.black"?: string;
    /** ANSI blue. */
    readonly "terminal.ansi.blue"?: string;
    /** ANSI bright black. */
    readonly "terminal.ansi.brightBlack"?: string;
    /** ANSI bright blue. */
    readonly "terminal.ansi.brightBlue"?: string;
    /** ANSI bright cyan. */
    readonly "terminal.ansi.brightCyan"?: string;
    /** ANSI bright green. */
    readonly "terminal.ansi.brightGreen"?: string;
    /** ANSI bright magenta. */
    readonly "terminal.ansi.brightMagenta"?: string;
    /** ANSI bright red. */
    readonly "terminal.ansi.brightRed"?: string;
    /** ANSI bright white. */
    readonly "terminal.ansi.brightWhite"?: string;
    /** ANSI bright yellow. */
    readonly "terminal.ansi.brightYellow"?: string;
    /** ANSI cyan. */
    readonly "terminal.ansi.cyan"?: string;
    /** ANSI green. */
    readonly "terminal.ansi.green"?: string;
    /** ANSI magenta. */
    readonly "terminal.ansi.magenta"?: string;
    /** ANSI red. */
    readonly "terminal.ansi.red"?: string;
    /** ANSI white. */
    readonly "terminal.ansi.white"?: string;
    /** ANSI yellow. */
    readonly "terminal.ansi.yellow"?: string;
    /** Terminal background and default cell background. */
    readonly "terminal.background"?: string;
    /** Terminal cursor. */
    readonly "terminal.cursor"?: string;
    /** Default terminal text. */
    readonly "terminal.foreground"?: string;
    /** Selected terminal cells. */
    readonly "terminal.selection"?: string;
    /** Default interface text. */
    readonly "text"?: string;
    /** Interface text that acts, such as buttons and links. */
    readonly "text.accent"?: string;
    /** Secondary interface text. */
    readonly "text.muted"?: string;
    /** The main toolbar above the workbench. */
    readonly "toolbar.background"?: string;
    /** Warnings. */
    readonly "warning"?: string;
    /** The window behind the cards, the toolbar, and the status bar. */
    readonly "window.background"?: string;
  }
  export interface ThemeSyntaxStyle {
    /** "#rrggbb" or "#rrggbbaa". */
    readonly color?: string;
    readonly fontStyle?: "normal" | "italic";
    /** From 100 to 900; 700 is bold. */
    readonly fontWeight?: number;
    readonly underline?: boolean;
    readonly strikethrough?: boolean;
  }
  /**
   * The shape and weight of the workbench chrome, in pixels at 100% zoom
   * except the weight. A value a theme leaves out keeps its Islands default,
   * and a `chrome.*` setting overrides the theme's value.
   */
  export interface ThemeChrome {
    /** Corner radius of the center and dock cards, 0 to 24. */
    readonly cardRadius?: number;
    /** Gap between cards, which is also their resize handle, 0 to 24. */
    readonly cardGap?: number;
    /** Margin around the cards at the window edges, 0 to 24. */
    readonly cardInset?: number;
    /** Width of the card border in the `border` color, 0 to 4. */
    readonly cardBorder?: number;
    /** Height of tab bars and dock headers, 20 to 48. */
    readonly tabBarHeight?: number;
    /** Width of the tool stripes, 24 to 64. */
    readonly stripeWidth?: number;
    /** Thickness of the accent line under the active tab, 0 to 6. */
    readonly accentWidth?: number;
    /** Corner radius of a tab, so the active tab can be a rounded pill, 0 to 12. */
    readonly tabRadius?: number;
    /** Space above and below the tabs inside the tab bar, 0 to 12. */
    readonly tabInset?: number;
  }
  export interface ThemeDefinition {
    readonly name: string;
    readonly appearance: ThemeAppearance;
    readonly colors?: ThemeColors;
    readonly chrome?: ThemeChrome;
    /** Styles by capture name (nvim-treesitter's) or a dotted prefix of
     * one, such as "markup"; a capture takes its longest styled prefix. */
    readonly syntax?: Readonly<Record<string, ThemeSyntaxStyle>>;
  }
  export interface ThemeInfo {
    readonly name: string;
    readonly appearance: ThemeAppearance;
  }
  export interface ColorDefinition {
    /** Flat dotted name, such as "diff.add". */
    readonly key: string;
    /** Shown by a future settings UI. */
    readonly description: string;
    /** For each appearance, "#rrggbb", "#rrggbbaa", or another key whose
     * color to use. */
    readonly defaults: Readonly<{ dark: string; light: string }>;
  }
  /**
   * Themes and color keys. The latest registration of a name defines that
   * theme, and disposal or teardown reveals the previous one. The theme.mode,
   * theme.dark, and theme.light settings select the theme; it is resolved
   * when startup completes and again whenever the selection or, in "system"
   * mode, the system appearance changes, so register themes in activation.
   * A color the theme leaves out takes its key's default for the theme's
   * appearance; an unregistered selection uses the defaults alone.
   */
  export const themes: Readonly<{
    /** Register a lifecycle-owned theme.
     * @throws TypeError for invalid fields or colors; HostError with
     * UnknownCapture (details: capture) or Cancelled. */
    register(definition: ThemeDefinition): Promise<Disposable>;
    /** The registration of `name` that applies, with every field present. */
    get(name: string): Promise<ThemeDefinition | null>;
    /** Every registered theme, sorted by name. */
    list(): Promise<readonly ThemeInfo[]>;
    /** Define a lifecycle-owned color key; teardown removes it.
     * @throws TypeError for invalid fields; HostError with DuplicateColor
     * when native code or a live lifecycle defined the key, or Cancelled. */
    defineColor(definition: ColorDefinition): Promise<void>;
  }>;
  /**
   * What an icon draws: a builtin Lucide glyph by its kebab-case name
   * (`{ lucide: "folder-open" }`) or an SVG document (`{ svg }`). `color`
   * is a color key, which paints the icon in that single color, or
   * "original", which keeps an SVG's own colors; without it the surface's
   * icon color applies.
   */
  export type IconDefinition =
    | { readonly lucide: string; readonly color?: ColorKey }
    | { readonly svg: string; readonly color?: ColorKey | "original" };
  export interface IconPack {
    /** The value of the `icons.pack` setting that selects this pack. */
    readonly name: string;
    /** Icon definitions by name; a name the pack leaves out falls back to
     * the default level. */
    readonly icons: Readonly<Record<string, IconDefinition>>;
  }
  /** Icon names for the files and folders a surface shows. A file name or
   * extension is claimed by the name of an icon, such as "file.rust", which
   * need not be defined by whoever claims it. */
  export interface IconAssociations {
    /** Exact file names, such as "Cargo.toml". */
    readonly files?: Readonly<Record<string, string>>;
    /** Extensions without the dot, any case, such as "rs" or "d.ts". The
     * longest extension of a file wins. */
    readonly extensions?: Readonly<Record<string, string>>;
    /** Exact folder names, such as "src". A folder icon name may have a
     * `.open` variant, such as "folder.src.open", used when expanded. */
    readonly folders?: Readonly<Record<string, string>>;
  }
  /**
   * Icons are identified by flat dotted names such as "file", "folder.open",
   * or "symbol". For one name the first of these that defines it wins: an
   * override, the active pack (the `icons.pack` setting), the defaults (the
   * native ones, then names packages `define`), and then a generic fallback
   * icon, so an unknown name is not an error. Registrations and selection
   * repaint every window; the latest registration of an override or pack
   * name wins, and disposal or teardown reveals the previous one.
   */
  export const icons: Readonly<{
    /** Define a lifecycle-owned default icon for a name your package
     * introduces; a pack or override may still restyle it.
     * @throws TypeError for an invalid name or definition, an unknown Lucide
     * glyph, or `"original"` with `lucide`; HostError with DuplicateIcon
     * (details: name) when the name already has a default, or Cancelled. */
    define(name: string, definition: IconDefinition): Promise<void>;
    /** Replace one icon above every pack, mainly for personal config.
     * @throws TypeError as define; HostError with Cancelled. */
    override(name: string, definition: IconDefinition): Promise<Disposable>;
    /** Claim icon names for file names, extensions, and folder names. A file
     * paints its first name that has a definition: the name claimed for its
     * file name, for its extension, `file.<languageId>` for its language,
     * then "file"; a folder paints its claimed name, then "folder", in the
     * `.open` form of each when expanded. The latest claim of a pattern wins,
     * and disposal or teardown reveals the previous one.
     * @throws TypeError for invalid names; HostError with Cancelled. */
    associate(associations: IconAssociations): Promise<Disposable>;
    /** Register a lifecycle-owned icon pack; the latest registration of a
     * name defines it.
     * @throws TypeError as define; HostError with Cancelled. */
    registerPack(pack: IconPack): Promise<Disposable>;
    /** Every registered pack name, sorted. */
    listPacks(): Promise<readonly string[]>;
  }>;
  /** Where a view sits in a window: the view itself, the views above it, and
   * its tab and pane. */
  export interface ViewNodeLocation {
    readonly view: ViewNodeInfo;
    /** The views containing `view`, its parent first and the tab's root last;
     * empty for a root. */
    readonly parents: readonly ViewNodeInfo[];
    readonly tab: PresentationInfo;
    readonly pane: PaneInfo;
  }
  /** Only application windows; diagnostic and startup-error windows are excluded.
   * get returns null after closure; fabricated/command targets reject InvalidTarget. */
  export const windows: Readonly<{
    list(): Promise<readonly WindowInfo[]>;
    get(target: WindowTarget): Promise<WindowInfo | null>;
    /** Finds a view in a window's information, whatever its depth: a
     * `ViewNodeTarget` from the window or a tab's `ViewTarget`, which
     * locates the tab's root view. Reads `window` only, so the result is as
     * current as it is; null when the window shows no such view, including a
     * target of another window. */
    findView(window: WindowInfo, target: ViewNodeTarget | ViewTarget): ViewNodeLocation | null;
    /** Opens a focused blank window, as New Blank Window does, then applies the
     * roots like workspaces.replaceRoots. When the roots are rejected the window
     * is closed again.
     * @throws HostError with ResourceNotFound, ResourceWrongKind, InvalidUri,
     * ProviderFailure, InvalidTarget, or Cancelled. */
    open(options?: OpenWindowOptions): Promise<WindowInfo>;
    /** Activates the window and focuses its active tab.
     * @throws HostError with InvalidTarget for a closed window, or Cancelled. */
    focus(target: WindowTarget): Promise<void>;
  }>;
  export interface OpenWindowOptions {
    /** Ordered roots for the new window; omission or an empty list opens it
     * without roots. */
    readonly roots?: readonly string[];
  }
  /** Window-owned ordered roots; empty roots are valid. Closed windows return null. */
  export interface WorkspaceDestinationOptions {
    /** Omission captures the command's window or the active window at admission. */
    readonly target?: WindowTarget;
  }
  export type FileKind = "file" | "directory";
  export interface FileEntry {
    readonly uri: string;
    /** The entry's name within its directory. */
    readonly name: string;
    readonly kind: FileKind;
    /** Whether the ignore policy ignores the entry: `.gitignore` and friends,
     * or a `files.ignored` pattern, or a directory above it that is ignored.
     * Explorers dim ignored entries; searches skip them. */
    readonly ignored: boolean;
  }
  /** A file `files.search` matched. */
  export interface FileSearchHit {
    readonly uri: string;
    /** The searched directory the file was found in, as normalized. */
    readonly directory: string;
    /** The path below `directory`, with `/` separators. */
    readonly path: string;
    /** The last segment of `path`. */
    readonly name: string;
    /** UTF-16 ranges of `path` that matched, sorted and disjoint. */
    readonly ranges: readonly (readonly [start: number, end: number])[];
  }
  export interface FileSearchResult {
    /** The best matches, best first. */
    readonly results: readonly FileSearchHit[];
    /** Whether more files matched than `results` holds. */
    readonly more: boolean;
  }
  export interface FileStat {
    readonly kind: FileKind;
  }
  export interface CompletionMatch {
    /** The candidate's index in the array passed to `match`. */
    readonly index: number;
    /** UTF-16 ranges of the candidate that matched, sorted and disjoint. */
    readonly ranges: readonly (readonly [start: number, end: number])[];
  }
  /** The editor's shared matcher, which pickers use too. */
  export const completion: Readonly<{
    /** Returns the candidates that match `query`, best first, by the styles of
     * the `completion.styles` setting or by `options.styles`: a candidate
     * matches if any style matches it, ordered by the first style that matches,
     * then by that style's score, then by the order given. A query of only
     * whitespace matches every candidate, in order, with no ranges. */
    match(
      query: string,
      candidates: readonly string[],
      options?: { readonly styles?: readonly string[] },
    ): Promise<readonly CompletionMatch[]>;
  }>;
  /** Reads, lists, stats, and changes stored resources through the registered resource
   * providers. Any URI with a registered provider is readable, independently of
   * workspace roots. These operations see only what is stored: an open
   * document's unsaved edits are ignored. `list`, `search`, and `isIgnored`
   * under an open workspace root answer from an in-memory index that follows
   * disk changes after the file watch's coalescing delay (about 100 ms); the
   * answers are otherwise the same as reading the disk. */
  export const files: Readonly<{
    /** Reads the stored file and decodes it as UTF-8, dropping a leading byte
     * order mark. It never reads an open document's buffer, so unsaved edits are
     * not visible; use `documents` and `Buffer.snapshot` for live text.
     * @throws HostError with InvalidUri, ResourceNotFound, ResourceWrongKind for
     * a directory, InvalidUtf8, or ProviderFailure. */
    readText(uri: string): Promise<string>;
    /** Lists exactly one directory, in unspecified order. Callers sort and
     * filter. Entries the `files.hidden` setting names (`.git`, `.DS_Store`, ...)
     * are omitted, and entries the repository ignore files or `files.ignored`
     * match are returned with `ignored` set. Entries a listing cannot represent
     * (broken symlinks, special files, and names that are not UTF-8) are skipped.
     * @throws HostError with InvalidUri, ResourceNotFound, ResourceWrongKind for
     * a file, or ProviderFailure. */
    list(uri: string): Promise<readonly FileEntry[]>;
    /** Matches `query` against the paths of the files below a directory, or
     * below each of a list of directories, at any depth, and resolves the best
     * `options.limit` (default 100, at most 1000) with whether others matched
     * too. Several directories are ranked as one search, so a strong match in a
     * later one outranks a weak match in an earlier one. Matching uses the editor's shared
     * matcher and the `completion.styles` setting. A file whose name matches
     * ranks above one that matches only through its directories; within each
     * group the matcher's order applies, then the order of the directories,
     * then path order. A query of only
     * whitespace matches every file, so the first files in path order come
     * back. Hidden and ignored files are left out, symlinked directories are
     * not followed, and directories that cannot be read are skipped. Searching
     * an ignored or hidden directory resolves empty.
     * @throws TypeError for a limit that is not a positive integer; HostError
     * with InvalidUri, ResourceNotFound, ResourceWrongKind for a file, or
     * ProviderFailure. */
    search(
      uri: string | readonly string[],
      query: string,
      options?: { readonly limit?: number },
    ): Promise<FileSearchResult>;
    /** Resolves the kind of the resource, or null when it does not exist.
     * Symlinks report their target's kind.
     * @throws HostError with InvalidUri or ProviderFailure. */
    stat(uri: string): Promise<FileStat | null>;
    /** Writes `text` to a file, replacing it or creating it and any missing
     * parent directories, and resolves its normalized URI. No document is
     * affected.
     * @throws HostError with InvalidUri, ResourceWrongKind for a directory or a
     * file parent, or ProviderFailure. */
    writeText(uri: string, text: string): Promise<string>;
    /** Creates an empty file, and any missing parent directories, and resolves
     * its normalized URI. Nothing is overwritten.
     * @throws HostError with InvalidUri, ResourceAlreadyExists when anything
     * exists at `uri`, ResourceWrongKind when a parent is a file, or
     * ProviderFailure. */
    createFile(uri: string): Promise<string>;
    /** Creates a directory, and any missing parents, and resolves its
     * normalized URI.
     * @throws HostError with InvalidUri, ResourceAlreadyExists when anything
     * exists at `uri`, ResourceWrongKind when a parent is a file, or
     * ProviderFailure. */
    createDirectory(uri: string): Promise<string>;
    /** Moves a file or a directory tree to `to`, creating its missing parents,
     * and resolves the normalized destination. Nothing is overwritten: delete
     * the destination first to replace it. Open documents at or under `from`,
     * clean or modified, follow to the new URI, keeping their text, history,
     * and views; their titles and languages are re-detected. Renames made
     * outside Knot are not followed: the old file vanishes and its document
     * becomes a deleted one.
     * @throws HostError with InvalidUri for a destination inside `from` or in
     * another scheme, ResourceNotFound, ResourceAlreadyExists when anything
     * exists at `to`, ResourceWrongKind when a parent of `to` is a file, or
     * ProviderFailure. */
    rename(from: string, to: string): Promise<string>;
    /** Copies a file or a directory tree to `to`, creating its missing
     * parents, and resolves the normalized destination. Nothing is
     * overwritten, and the copy of a file keeps its permissions.
     * @throws HostError with InvalidUri for a destination inside `from` or in
     * another scheme, ResourceNotFound, ResourceAlreadyExists when anything
     * exists at `to`, ResourceWrongKind when a parent of `to` is a file, or
     * ProviderFailure. */
    copy(from: string, to: string): Promise<string>;
    /** Shows the file or folder in the operating system's file manager (Finder
     * on macOS), selecting it.
     * @throws HostError with InvalidUri, or ProviderFailure for a resource that
     * is not a local file. */
    reveal(uri: string): Promise<void>;
    /** Deletes a file or a directory tree. By default the entry moves to the
     * operating system's trash; with `permanent` it is removed outright. A
     * provider without a trash, such as the in-memory one, always removes.
     * Open documents at or under `uri` keep their text and become deleted
     * documents, dirty and associated with the missing file, so saving
     * recreates it. Nothing is confirmed and no document is closed.
     * @throws HostError with InvalidUri, ResourceNotFound, or ProviderFailure,
     * including when the platform cannot trash the entry. */
    delete(uri: string, options?: { readonly permanent?: boolean }): Promise<void>;
    /** Whether the resource is hidden or ignored by the same policy `list`
     * applies, including through an ignored or hidden directory above it. A
     * missing resource is judged by its name alone.
     * @throws HostError with InvalidUri or ProviderFailure. */
    isIgnored(uri: string): Promise<boolean>;
    /** Calls `listener` with the URIs that changed under `uri`, in batches that
     * gather about 100 ms of changes and name each URI once. A batch does not
     * say whether an entry was created, changed, or removed: stat or list what
     * you care about. Without `recursive` a directory reports itself and its
     * direct children, and a file reports itself; with it, everything below.
     * When the platform drops events, the batch names `uri` itself, so rescan.
     * Entries the `files.hidden` setting names are not reported unless
     * `includeHidden` is set, as for a watch of a directory like `.git`;
     * ignored entries always are. Watching sees every change on disk, including Knot's own
     * saves, and independently of open documents. Notifications reach one
     * listener in order, and it stops with its lifecycle or when it is disposed. Resolves
     * once the watch runs, so a read after it misses no later change.
     * @throws HostError with InvalidUri, ResourceNotFound when `uri` does not
     * exist, or ProviderFailure when the provider cannot watch. */
    watch(
      uri: string,
      listener: (uris: readonly string[]) => void | Promise<void>,
      options?: { readonly recursive?: boolean; readonly includeHidden?: boolean },
    ): Promise<Disposable>;
  }>;
  /** What a process is started with. */
  export interface ProcessOptions {
    /** The program to run. A name without a path separator is looked up on the
     * `PATH` of the environment the process gets. */
    readonly command: string;
    readonly args?: readonly string[];
    /** A file URI; the user's home directory by default. */
    readonly cwd?: string;
    /** Merged over the environment Knot inherited from the user's login shell;
     * `null` unsets a variable. */
    readonly env?: Readonly<Record<string, string | null>>;
  }
  export interface ProcessRunOptions extends ProcessOptions {
    /** Written to the process's standard input, which is then closed. */
    readonly stdin?: string;
  }
  export type ProcessSignal =
    "SIGHUP" | "SIGINT" | "SIGQUIT" | "SIGKILL" | "SIGTERM" | "SIGUSR1" | "SIGUSR2";
  /** How a process ended: `exitCode` is null when a signal ended it. */
  export interface ProcessExit {
    readonly exitCode: number | null;
    readonly signal: string | null;
  }
  export interface ProcessResult extends ProcessExit {
    readonly stdout: string;
    readonly stderr: string;
  }
  /** A running process started with `processes.spawn`. Once the process has
   * exited and both streams have been read to their ends, Knot releases the
   * handle and later calls reject with InvalidTarget. */
  export interface ProcessHandle {
    /** Calls `listener` with each chunk of standard output, in order. The
     * process reads ahead only as listeners finish, so a slow listener stalls
     * it on its pipe; output that arrives before the first listener attaches
     * is held until one does. Output is UTF-8 text, invalid bytes replaced.
     * Listeners attached to the same stream receive the same chunks. */
    onStdout(listener: (text: string) => void | Promise<void>): Disposable;
    onStderr(listener: (text: string) => void | Promise<void>): Disposable;
    /** Queues text for standard input.
     * @throws HostError with ProcessInputClosed after `closeStdin`. */
    write(text: string): Promise<void>;
    closeStdin(): Promise<void>;
    /** Signals the process's whole group, `SIGTERM` by default. */
    kill(signal?: ProcessSignal): Promise<void>;
    /** Releases the handle, killing the process group when it still runs.
     * Later calls with the handle reject with InvalidTarget. */
    [Symbol.asyncDispose](): Promise<void>;
    /** Resolves when the process exits. A process that writes more than its
     * pipe holds does not exit until its output is read. */
    readonly exited: Promise<ProcessExit>;
    /** Resolves once standard output has been read to its end and every
     * listener has finished with its last chunk. `exited` can resolve before
     * this, so await both before treating the output as complete. Output is
     * only read while a listener is attached, so this stays pending until
     * `onStdout` is called. */
    readonly stdoutEnded: Promise<void>;
    /** Like `stdoutEnded`, for standard error and `onStderr`. */
    readonly stderrEnded: Promise<void>;
  }
  /**
   * Runs programs on the user's computer, which a package declares with the
   * `process` entitlement in `knot.jsonc`. The user decides on the entitlement
   * before the package loads, so a package that declared it can always use
   * these. Each process starts in its own process group, and the group is
   * killed when the package unloads or restarts and when Knot quits. Cancelling a command does not kill its
   * processes: call `kill` from the command's `signal` abort listener.
   */
  export const processes: Readonly<{
    /** Runs the program to completion and collects its output.
     * @throws HostError with EntitlementNotDeclared, ProgramNotFound,
     * WorkingDirectoryNotFound, InvalidUri, or ProcessFailed. */
    run(options: ProcessRunOptions): Promise<ProcessResult>;
    /** Starts the program and returns a handle to stream its output and
     * control it. Rejects like `run`. */
    spawn(options: ProcessOptions): Promise<ProcessHandle>;
  }>;
  export const workspaces: Readonly<{
    get(target: WindowTarget): Promise<WorkspaceInfo | null>;
    /** Validates an existing directory and replaces this window's roots. Missing
     * roots reject ResourceNotFound, files reject ResourceWrongKind. */
    openFolder(uri: string, options?: WorkspaceDestinationOptions): Promise<WorkspaceInfo>;
    /** Validates an ordered list of directory URIs before committing roots and
     * tree together. Duplicates after normalization keep their first position;
     * nested and overlapping roots are allowed. Empty roots clear; retained
     * normalized roots keep their IDs, so adding, removing, and reordering are
     * replacements of the whole list. Documents remain open. A newer Open/root
     * request or root generation invalidates pending work. */
    replaceRoots(roots: readonly string[], options?: WorkspaceDestinationOptions): Promise<WorkspaceInfo>;
    /** Presents the native directory prompt and resolves a normalized, existing
     * directory URI, or dismissal. The selected path is normalized through the
     * registered providers, so a missing directory rejects ResourceNotFound and
     * a file rejects ResourceWrongKind. Omission captures the command's window or
     * active window at admission.
     * @throws HostError with OperationInProgress when another extension
     * interaction owns the window, InvalidTarget, ContextUnavailable, Cancelled,
     * InvalidUri, ResourceNotFound, ResourceWrongKind, or ProviderFailure. */
    pickFolder(options?: WorkspaceDestinationOptions): Promise<FolderPickerResult>;
  }>;

  export interface ApplicationSnapshot {
    readonly documents: readonly DocumentInfo[];
    readonly windows: readonly WindowInfo[];
    readonly activeWindow: WindowTarget | null;
    readonly activeTarget: ViewTarget | null;
    /** The notice history, oldest first. */
    readonly notifications: readonly Notice[];
    /** True once startup has committed its restored windows and launch
     * requests. Extensions activate before any window exists; wait for the
     * launched event before reconciling state against startup windows. */
    readonly launched: boolean;
  }
  export type ApplicationChange =
    | { readonly type: "documentOpened" | "documentChanged"; readonly document: DocumentInfo }
    /** The document's file changed, as Save As and a rename do; `from` is the
     * URI it had, null for an untitled document. Follows a `documentChanged`. */
    | { readonly type: "documentMoved"; readonly document: DocumentInfo; readonly from: string | null }
    | { readonly type: "documentClosed"; readonly document: DocumentId }
    | { readonly type: "windowOpened" | "presentationsChanged"; readonly window: WindowInfo }
    | { readonly type: "windowClosed"; readonly window: WindowTarget }
    | { readonly type: "workspaceChanged"; readonly workspace: WorkspaceInfo }
    | { readonly type: "activeTargetChanged"; readonly activeWindow: WindowTarget | null; readonly activeTarget: ViewTarget | null }
    /** The window's captured layout changed: cursor, selection, scroll, focused
     * pane, dock size, window bounds, or structure. Reported about a second
     * after changes settle; structural changes also report presentationsChanged. */
    | { readonly type: "viewStateChanged"; readonly window: WindowTarget }
    /** A notice was added, read, dismissed, or cleared; carries the whole history. */
    | { readonly type: "notificationsChanged"; readonly notifications: readonly Notice[] }
    /** Sent once, when launched becomes true. */
    | { readonly type: "launched" };
  export type ApplicationEvent = ApplicationChange & { readonly sequence: number };
  export interface WorkbenchObservation extends Disposable {
    readonly snapshot: ApplicationSnapshot;
    readonly watermark: number;
  }

  /**
   * Host-issued capture, valid only in its originating command invocation tree.
   * Contextual methods validate both authority and the live presentation.
   * Copying this object or retaining it after invocation does not grant access.
   */
  export interface CommandTarget {
    readonly [targetAuthority]: true;
    /** Originating window identity, captured before palette/focus changes. */
    readonly window: WindowId;
    /** Live target for the same window, accepted by window-targeted operations. */
    readonly windowTarget: WindowTarget;
    /** The tab presentation the command was captured on, or null when no tab
     * was focused. It identifies that exact tab; it never follows a later
     * focus change. */
    readonly viewTarget: ViewTarget | null;
    /** The view with focus inside that tab when the command was captured: an
     * editor when `CommandContext.editor` is set, so the command acts on that
     * exact editor and never on the side that gains focus later. Null when no
     * tab was focused. */
    readonly focusedView: ViewNodeTarget | null;
    /** The views above `focusedView`, its parent first and the tab's root view
     * last; empty when the focused view is the root. A composite view's
     * package finds its instance here. */
    readonly viewChain: readonly ViewNodeTarget[];
    /** Captured extension-addressable view, or null when none is available. */
    readonly view: ViewId | null;
    /** Stable contributed view ID, or null for non-contributed surfaces. */
    readonly tool: string | null;
  }
  /**
   * JSON-compatible data: finite numbers, dense arrays and plain string-keyed
   * records recursively. Cycles, undefined, functions, symbols, bigint,
   * nonplain objects and custom toJSON conversion are rejected.
   */
  export type JsonValue = null | boolean | number | string | readonly JsonValue[]
    | { readonly [key: string]: JsonValue };

  /** Missing keys are distinct from stored JSON null. */
  export type StorageRead = { readonly found: false }
    | { readonly found: true; readonly value: JsonValue };
  /** Application-global private records for this package or personal config.
   * Available during activation, without a window or command context. Data survives
   * reload, package updates and uninstall. Keys are exact strings, including empty.
   * TypeError for invalid JSON/types; StorageUnavailable includes database/cause
   * diagnostics. Cancellation may reject after a write has committed.
   */
  export const storage: {
    readonly read: (key: string) => Promise<StorageRead>;
    /** Atomically replace one record. Success acknowledges commit. Concurrent
     * calls serialize; last committed write wins. Read/modify/write is not atomic.
     */
    readonly write: (key: string, value: JsonValue) => Promise<void>;
    /** Deleting a missing key succeeds. */
    readonly delete: (key: string) => Promise<void>;
    /** All keys in deterministic binary order. */
    readonly list: () => Promise<readonly string[]>;
  };

  /**
   * Recently chosen items by name, kept in this package's storage so they
   * survive restarts. A name is any text you choose, private to the package.
   * Order a picker's items with `order` and record the choice with `record`;
   * the picker keeps the order it is given for an empty query and for matches
   * that are otherwise equal.
   */
  export const history: Readonly<{
    /** The recorded IDs, most recent first, at most 50. */
    recent(name: string): Promise<readonly string[]>;
    /** Moves the ID to the front of the named list. Read/modify/write is not
     * atomic, as for `storage`. */
    record(name: string, id: string): Promise<void>;
    /** Removes the ID from the named list, or the whole list when no ID is
     * given. Read/modify/write is not atomic, as for `storage`. */
    forget(name: string, id?: string): Promise<void>;
    /** The items with recorded ones first, most recent first, followed by the
     * rest in their given order. The key defaults to `item.id`. */
    order<T extends { readonly id: string }>(name: string, items: readonly T[]): Promise<readonly T[]>;
    order<T>(name: string, items: readonly T[], key: (item: T) => string): Promise<readonly T[]>;
  }>;

  const captureIdentity: unique symbol;
  /** Application-generated retained capture identity. Safe to persist, for
   * example in a session record; it addresses only this package's captures. */
  export type CaptureId = string & { readonly [captureIdentity]: true };
  export interface RetainedCapture {
    readonly id: CaptureId;
    /** The document captured; restart reuse of the identity is not guaranteed. */
    readonly document: DocumentId;
    readonly title: string;
    readonly kind: "untitled" | "destination" | "persisted";
    readonly resourceUri: string | null;
    /** Capture time in milliseconds since the Unix epoch. */
    readonly capturedAt: number;
  }
  /** Restored document and its presentation. "restored" reports how the
   * resource relates to the capture: a changed file keeps the capture-time
   * version, so Save reaches the conflict prompt; a missing file becomes a dirty
   * destination. "preserved" means a live document (owner, null once closed)
   * already holds the identity or URI; the text opened as a separate untitled
   * document instead. */
  export type RecoveryRestore = DocumentPresentation & Readonly<
    | { outcome: "restored"; resource: "untitled" | "unchanged" | "missing" | "changed" }
    | { outcome: "preserved"; owner: DocumentId | null }>;
  /**
   * Durable document-text captures retained by this package or personal config.
   * Available during activation. The owner is the caller's canonical identity,
   * never an argument: another owner's capture is indistinguishable from a
   * missing one. Captures are immutable and survive unload, failure, restart and
   * uninstall until released; the native File › Recover Unsaved Documents… listing
   * shows them with their owner and can restore or discard them. Automatic
   * unsaved-document protection is independent and unaffected.
   * RecoveryUnavailable includes a cause diagnostic.
   */
  export const recovery: Readonly<{
    /** This owner's captures, newest first. Reconcile it with your own records
     * to find and release captures a cancelled call left unreferenced. */
    list(): Promise<readonly RetainedCapture[]>;
    /** Capture an open document's current text, clean or dirty. Resolves only
     * after the capture is durably committed, so a record referencing the ID may
     * be committed next. Each call yields a new ID; earlier captures of the same
     * document remain. Cancellation may reject after the capture committed.
     * @throws HostError with DocumentNotFound, RecoveryUnavailable or Cancelled. */
    retain(document: DocumentId): Promise<RetainedCapture>;
    /** Delete a capture. Release an old capture only after the record that
     * replaced it is durable. Resolves false when this owner holds no such ID. */
    release(id: CaptureId): Promise<boolean>;
    /** Restore a capture as an ordinary document with an empty undo history and
     * present it like documents.present. Never writes a resource or replaces a
     * live document. The capture stays retained until released.
     * @throws HostError with CaptureNotFound, InvalidTarget, ContextUnavailable,
     * Cancelled, RecoveryUnavailable, or the documents.open resource codes when
     * the file baseline cannot be read. */
    restore(id: CaptureId, options?: DocumentDestinationOptions): Promise<RecoveryRestore>;
  }>;

  /** Stable host failure codes; names, messages and stacks are diagnostics.
   * Checked against native handler descriptors and common transport failures. */
  export type HostErrorCode = "ContextUnavailable" | "InvalidTarget" | "OwnershipDisposed"
    | "Cancelled" | "UnsupportedOperation" | "BufferClosed" | "BufferReadOnly"
    | "InvalidRange" | "InvalidEditBatch" | "RevisionConflict" | "ContributionSetNotFound"
    | "ViewNotFound" | "ViewIdInUse" | "InvalidViewId"
    | "ViewRegistrationNotFound" | "ContentKindMismatch" | "TreeItemNotFound"
    | "ProviderRegistrationNotFound"
    | "ContractNotFound" | "ContractViolation" | "PersistKeyInUse"
    | "CommandNameInUse" | "CommandNotFound"
    | "InvalidUri" | "DocumentNotFound" | "DocumentUnsaved" | "OperationInProgress" | "PersistenceFailed" | "ResourceNotFound" | "ResourceWrongKind" | "ResourceAlreadyExists" | "InvalidUtf8" | "ProviderFailure" | "StorageUnavailable"
    | "CaptureNotFound" | "RecoveryUnavailable" | "SessionConflict"
    | "UnknownGrammar" | "InvalidQuery" | "LanguageRegistrationNotFound"
    | "DuplicateSetting" | "SettingTypeMismatch"
    | "UnknownCapture" | "DuplicateColor" | "ThemeRegistrationNotFound"
    | "DuplicateIcon" | "IconRegistrationNotFound"
    | "StatusItemRegistrationNotFound" | "DecorationSourceNotFound" | "MenuItemRegistrationNotFound"
    | "BarNotFound"
    | "KeymapPresetRegistrationNotFound"
    | "EntitlementNotDeclared" | "ProgramNotFound" | "WorkingDirectoryNotFound"
    | "ProcessFailed" | "ProcessInputClosed";
  /**
   * Native operation failure. Narrow a caught Error by name === "KnotHostError"
   * and its string code property, then dispatch on code. No error constructor
   * is exported. Author-side TypeError/RangeError are separate errors.
   */
  export interface HostError extends Error {
    readonly name: "KnotHostError";
    /** Stable category for handling a failure independently of its message. */
    readonly code: HostErrorCode;
    /** Optional handler-specific JSON diagnostics; may be null. Narrow fields
     * before inspecting them. Use code for categories; message is diagnostic. */
    readonly details?: JsonValue;
  }
  /**
   * A lifecycle-owned registration, subscription or contribution set; native IDs
   * are hidden and must not be persisted. Unload/failed activation/shutdown release
   * ownership independently of extension awaits or final JavaScript callbacks.
   *
   * Release it with `await handle[Symbol.asyncDispose]()`, or bind it with
   * `await using handle = await ...` to release it when the scope exits. If the
   * release fails at scope exit, the failure is thrown after the body; when the
   * body threw too, both arrive as a `SuppressedError` whose `error` is the
   * release failure and `suppressed` the body error.
   */
  export interface Disposable extends AsyncDisposable {
    /**
     * Mark inactive immediately and release native ownership.
     * Resolution acknowledges release, not completion of an executing callback;
     * awaiting disposal from that callback is safe. No new callback is admitted.
     * Repeated/concurrent calls share one promise and perform no second mutation.
     * Failure is retained for repeated calls; the object stays inactive and
     * lifecycle teardown releases any remaining ownership. No automatic retry.
     * Pending provider work is cancelled; an already-running callback may finish
     * or be cancelled under its own callback contract.
     *
     * @throws HostError for cancellation or stale native ownership. Command,
     * tree, completion and language registrations use their resource-specific
     * not-found codes; disposing a subscription the host already released
     * succeeds. Contribution cleanup
     * can fail with BufferClosed; an absent contribution set is already released.
     */
    [Symbol.asyncDispose](): Promise<void>;
  }
  /**
   * Half-open absolute UTF-8 byte range [startByteOffset, endByteOffset).
   * Endpoints must be ordered, within the buffer and on Unicode scalar boundaries.
   * Offsets are not JavaScript string indices or grapheme positions.
   */
  export interface ByteRange {
    readonly startByteOffset: number;
    readonly endByteOffset: number;
  }
  /** Replacement in the precommit buffer coordinates of one edit batch. */
  export interface TextEdit { readonly range: ByteRange; readonly text: string; }
  /** Optimistic concurrency: the current buffer revision must match exactly. */
  export interface RevisionOptions {
    /** Required; never defaults to the current revision. */
    readonly ifRevision: number;
  }
  /** Immutable text and revision captured together for the requested range. */
  export interface TextSnapshot {
    /** JavaScript UTF-16 text for range. */
    readonly text: string;
    /** Absolute byte range in the original buffer. */
    readonly range: ByteRange;
    /** Buffer revision when this snapshot was captured. */
    readonly revision: number;
    /**
     * Convert a UTF-16 code-unit index in text to a relative UTF-8 byte offset.
     * Add range.startByteOffset to obtain a whole-buffer coordinate.
     * @throws TypeError for a nonnumber; RangeError for invalid numeric values,
     * out-of-bounds offsets or an index splitting a surrogate pair.
     */
    byteOffsetAtUtf16(offset: number): number;
    /**
     * Convert a byte offset relative to text to a relative UTF-16 index.
     * @throws TypeError for a nonnumber; RangeError for invalid numeric values,
     * out-of-bounds offsets or an offset splitting a UTF-8 scalar.
     */
    utf16OffsetAtByte(offset: number): number;
  }
  /** One committed edit batch, delivered to a listener in order. */
  export interface BufferChange {
    readonly buffer: Buffer;
    /** Revision used by the original batch. */
    readonly beforeRevision: number;
    readonly revision: number;
    /** Ordered replacements in beforeRevision coordinates. */
    readonly edits: readonly TextEdit[];
  }
  /**
   * What a decoration draws: `background` behind the range's text on each
   * row, `line` as a band across every line the range touches (a range ending
   * at column 0 of a later line leaves that line out, and an empty range
   * paints its own line), or a straight `underline` or `wavy` underline.
   */
  export type DecorationStyle = "background" | "line" | "underline" | "wavy";
  /** How a contribution paints its range, in the color of a theme key. */
  export interface Decoration {
    readonly color: ColorKey;
    readonly style: DecorationStyle;
  }
  /**
   * What a gutter mark draws: a `dot` on the range's first line in the marker
   * lane; a `bar` along every line of the range, wrapped rows included, in the
   * bar lane (a range ending at column 0 of a later line leaves that line
   * out); or a `deletion` wedge on the boundary above the range's start line,
   * in the bar lane, below the last line at the end of a text. A zero-width
   * range is the usual form of a deletion.
   */
  export type GutterStyle = "dot" | "bar" | "deletion";
  /** How a contribution marks the gutter, in the color of a theme key. */
  export interface GutterMark {
    readonly color: ColorKey;
    readonly style: GutterStyle;
  }
  /**
   * The command a click on a contribution runs: a name, or a name with an
   * `arguments` object. The host adds `range`, the contribution's current
   * range, to the arguments.
   */
  export type ContributionCommand =
    | string
    | { readonly name: string; readonly arguments?: { readonly [key: string]: JsonValue } };
  /**
   * Anchored semantic range. Optional decoration, gutter and command may be null.
   * Edits stabilize its anchors.
   */
  export interface EditorContribution {
    readonly range: ByteRange;
    /**
     * Paints the range. Where decorations overlap, `error`, `warning`, and
     * `info` paint above other keys, the most severe on top; backgrounds paint
     * beneath the selection.
     */
    readonly decoration?: Decoration | null;
    /**
     * Marks the gutter. Each of the two lanes shows one mark per line, chosen
     * like decorations by severity and then by the latest; a `bar` claims
     * every line it spans and a `deletion` its start line.
     */
    readonly gutter?: GutterMark | null;
    /** Runs when a gutter mark or the range's text is clicked. */
    readonly command?: ContributionCommand | null;
  }
  /** A decoration or gutter mark one view paints; see `Editor.setDecorations`.
   * Needs a `decoration`, a `gutter`, or both. */
  export interface ViewDecoration {
    readonly range: ByteRange;
    /** Paints the range, ordered among overlaps as contributions are. */
    readonly decoration?: Decoration | null;
    /** Marks the gutter as a contribution's does, in this view only. */
    readonly gutter?: GutterMark | null;
  }
  /**
   * Lines of a source buffer that an editor shows above one of its own lines;
   * see `Editor.setBlocks`.
   */
  export interface EditorBlockRows {
    /** The 0-based line of the editor the rows sit above. A line past the
     * last shows them above the last line. */
    readonly line: number;
    /** The buffer whose text and syntax highlighting the rows show. */
    readonly source: Buffer;
    /** The 0-based lines of `source` to show, `end` exclusive. Lines past the
     * source's last are left out, and an empty range shows nothing. */
    readonly lines: { readonly start: number; readonly end: number };
    /** Bands the rows in the color of a theme key, as a `line` decoration
     * does; without one they have no band. */
    readonly color?: ColorKey | null;
  }
  export interface FindOptions {
    /**
     * Compare characters exactly. The default folds case one character at a
     * time (Unicode simple case folding), so `ς`, `σ` and `Σ` agree but `ß`
     * does not match `ss` and `İ` and `ı` match only themselves.
     */
    readonly caseSensitive?: boolean;
    /**
     * Accept a match only at word boundaries, by the word classes that double
     * click and word movement use: a match edge that is not a word character
     * needs no boundary, and the text around a `range` counts.
     */
    readonly wholeWord?: boolean;
    /**
     * Read the query as a Rust `regex` pattern, matched natively in multi-line
     * mode: `^` and `$` match at line boundaries and `\n` matches across
     * lines. `caseSensitive` is the pattern's case flag, and `wholeWord`
     * checks each match's edges. Empty matches are returned; a search resumes
     * one character after one. Anchors and `\b` at a `range` edge look at the
     * text outside it.
     */
    readonly regex?: boolean;
    /**
     * Keep at most one match starting on each line, the first. After a match
     * that spans lines, the line it ends on is a candidate again, past the
     * match's end. For `replace`, one replacement per line.
     */
    readonly perLine?: boolean;
    /** Search only here; a match must lie wholly inside. Defaults to the whole buffer. */
    readonly range?: ByteRange;
    /** Most matches to return, 1000 by default and at most 100000 whatever is asked. */
    readonly limit?: number;
  }
  /** Options of `Buffer.replace`: those of `find`, with its own limit. */
  export interface ReplaceOptions extends Omit<FindOptions, "limit"> {
    /** Most replacements to make; every match up to 100000 by default. */
    readonly limit?: number;
    /**
     * The revision the matches must come from; defaults to the current one,
     * which suits a replacement made from the query alone.
     */
    readonly ifRevision?: number;
  }
  export type ReplaceResult = Readonly<{
    /** The revision after the replacement; unchanged when nothing was replaced. */
    revision: number;
    /** The matches replaced. */
    count: number;
    /** Whether a match followed the last replaced, cut off by the limit. */
    more: boolean;
  }>;
  /** A unit of text that double click selects; see `Buffer.wordAt`. */
  export type WordSegment = Readonly<{
    range: ByteRange;
    kind: "word" | "separator" | "space";
  }>;
  export type FindResult = Readonly<{
    /** The revision of the text searched. */
    revision: number;
    matches: readonly ByteRange[];
    /** Whether a match followed the last returned, cut off by the limit. */
    more: boolean;
  }>;
  /** One replaceable set per buffer and extension lifecycle. */
  export interface EditorContributions extends Disposable {
    /**
     * Atomically replace this lifecycle's entire set at the required revision.
     * Empty items clear it. Overlap is allowed; nonempty items on an empty buffer
     * are invalid; empty ranges on a nonempty buffer follow anchored-range rules.
     * Accepted replacements precede subsequent disposal in order; new calls after
     * disposal starts reject with OwnershipDisposed.
     * Resolution acknowledges native replacement, not paint.
     * @throws HostError with BufferClosed, RevisionConflict or InvalidRange;
     * OwnershipDisposed after this object's disposal starts; Cancelled on teardown.
     */
    replace(items: readonly EditorContribution[], options: RevisionOptions): Promise<void>;
  }
  /** A position in a buffer's text that follows edits. */
  export interface Mark extends Disposable {
    /**
     * The mark's byte offset now, or null once a deletion removed the text it
     * sat in (undo does not bring it back) or it was disposed.
     * @throws HostError with BufferClosed or Cancelled on teardown.
     */
    position(): Promise<number | null>;
  }
  /**
   * Opaque live text-buffer proxy. Each operation uses this buffer, never focus.
   * May be retained for asynchronous work within the extension lifecycle.
   * Closing one presentation does not invalidate a still-live shared buffer.
   * Do not persist it; it is not a persistent document identity.
   */
  export interface Buffer {
    readonly [bufferIdentity]: true;
    /**
     * Capture immutable text and revision. Omitted/null range means whole buffer.
     * @throws HostError with BufferClosed or InvalidRange, or Cancelled on teardown.
     */
    snapshot(range?: ByteRange | null): Promise<TextSnapshot>;
    /**
     * Atomically apply ordered, nonoverlapping replacements at ifRevision.
     * Repeated empty insertions at one position retain input order. A real batch
     * advances revision once; an empty/no-op batch keeps it and emits no change.
     * Resolution acknowledges the commit and returns its resulting revision.
     * @throws HostError with BufferClosed, BufferReadOnly, RevisionConflict,
     * InvalidRange or InvalidEditBatch; Cancelled before mutation or on teardown.
     */
    applyEdits(edits: readonly TextEdit[], options: RevisionOptions): Promise<Readonly<{ revision: number }>>;
    /**
     * Subscribe to committed changes; resolves once native ownership exists.
     * Use change.buffer inside asynchronous callbacks. A listener rejection
     * diagnoses only that callback; later FIFO deliveries continue.
     * Disposal stops new admission and discards queued unowned deliveries.
     * Registration racing teardown rejects instead of publishing a late listener.
     * @throws TypeError for a nonfunction; HostError with BufferClosed or Cancelled.
     */
    onDidChange(listener: (change: BufferChange) => void | Promise<void>): Promise<Disposable>;
    /**
     * Number of lines, from the line index; always at least one. Lines are
     * 0-based and split at `\n`, so a trailing newline adds an empty last line.
     * @throws HostError with BufferClosed, or Cancelled on teardown.
     */
    lineCount(): Promise<number>;
    /**
     * Range of a 0-based line without its `\n` or `\r\n` terminator.
     * @throws HostError with BufferClosed, InvalidRange for a line at or past
     * lineCount(), or Cancelled on teardown.
     */
    lineRange(line: number): Promise<ByteRange>;
    /**
     * Lines `first` through `last` (0-based, inclusive) in one read.
     * @throws HostError with BufferClosed, InvalidRange unless `first <= last` and `last` is below lineCount(), or
     * Cancelled on teardown.
     */
    lines(first: number, last: number): Promise<readonly BufferLine[]>;
    /**
     * The 0-based line holding a byte offset; the buffer length is the last line.
     * @throws HostError with BufferClosed, InvalidRange for an offset past the
     * end or splitting a UTF-8 scalar, or Cancelled on teardown.
     */
    lineAt(offset: number): Promise<number>;
    /**
     * The word, run of separators, or run of whitespace at a byte offset: the
     * unit double click selects, within the offset's line. A word ending at the
     * offset is preferred over what follows it. Null on a line without text.
     * @throws HostError with BufferClosed, InvalidRange for an offset past the
     * end or splitting a UTF-8 scalar, or Cancelled on teardown.
     */
    wordAt(offset: number): Promise<WordSegment | null>;
    /**
     * Find the occurrences of a query (literal, or a pattern with
     * `options.regex`), in order and without overlap,
     * in the whole buffer or `options.range`. Matching is native, so the text
     * never crosses the boundary. The result carries the revision it searched,
     * for the revision-checked calls that use it; `more` is true when a match
     * followed the last one returned. An empty query matches nothing.
     * @throws TypeError for a non-string query or an invalid limit; HostError
     * with BufferClosed, InvalidRange for a range past the end, reversed, or
     * splitting a UTF-8 scalar, InvalidQuery with the parser's message for an
     * invalid pattern (`details.message`), or Cancelled on teardown.
     */
    find(query: string, options?: FindOptions): Promise<FindResult>;
    /**
     * Replace the occurrences `find` would return for `query` and `options`
     * (`limit` and `range` included) with `replacement`, in one commit: one
     * undo step and one change notification. Matching and expansion are
     * native, so no text crosses the boundary. A literal query inserts the
     * replacement as typed. With `options.regex` the replacement expands `$1`,
     * `${name}` and `$$` from the match's groups (a missing group is empty;
     * write `${1}x`, since `$1x` names the group `1x`), and `\n`, `\t` and
     * `\\` to a line break, a tab and a backslash. Nothing is committed
     * when nothing matches or each replacement equals its match.
     * @throws TypeError for a non-string query or replacement or an invalid
     * limit or revision; HostError with BufferClosed, BufferReadOnly,
     * InvalidRange as for `find`, InvalidQuery with the parser's message for
     * an invalid pattern (`details.message`), RevisionConflict when
     * `ifRevision` is not the current revision, or Cancelled on teardown.
     */
    replace(query: string, replacement: string, options?: ReplaceOptions): Promise<ReplaceResult>;
    /**
     * Mark a byte offset. The mark stays with the character after it as edits
     * move that character, so text inserted before or at the mark moves it, and
     * undo and redo restore it with the text. It belongs to the caller's
     * lifecycle, ends with it, and is not persisted; names, listing, and
     * storage across restarts are the caller's.
     * @throws HostError with BufferClosed, or InvalidRange for an offset past
     * the end, splitting a UTF-8 scalar, or in an empty buffer; or Cancelled on
     * teardown.
     */
    createMark(offset: number): Promise<Mark>;
    /** Lifecycle-owned semantic data; disposing it does not dispose this buffer. */
    readonly contributions: EditorContributions;
  }

  /** Options of `buffers.create`. */
  export interface CreateBufferOptions {
    /** The initial text. Defaults to empty. A stored `persist` text replaces it. */
    readonly text?: string;
    /** A language ID, as `documents.setLanguage` takes. Defaults to plaintext. */
    readonly language?: string | null;
    /** Rejects every edit with BufferReadOnly. Defaults to false. */
    readonly readOnly?: boolean;
    /** A key, private to this package, under which the text persists across
     * restarts. Not for a read-only buffer. */
    readonly persist?: string;
  }
  /**
   * A buffer this package owns. It belongs to no document: it is never dirty,
   * has no closing prompt, and Save does not touch it. Ending it, by disposal
   * or by the package's unload, makes its operations reject with BufferClosed.
   */
  export interface ExtensionBuffer extends Buffer, AsyncDisposable {
    /**
     * Removes the text stored under the `persist` key and cancels a pending
     * write; later edits persist again. Does nothing without a key. Disposal
     * keeps the stored text.
     * @throws HostError with BufferClosed, or Cancelled on teardown.
     */
    clearPersisted(): Promise<void>;
  }
  export const buffers: Readonly<{
    /**
     * Creates a buffer outside document identity, owned by this lifecycle.
     * With `persist`, the text is kept in this package's storage, written
     * shortly after edits and at shutdown, and a buffer created with the same
     * key later starts from it. An empty buffer stores nothing.
     * @throws TypeError for invalid options or `persist` with `readOnly`;
     * HostError with PersistKeyInUse when a live buffer of this package has the
     * key, StorageUnavailable, or Cancelled on teardown.
     */
    create(options?: CreateBufferOptions): Promise<ExtensionBuffer>;
  }>;
  /**
   * The 0-based first row wholly inside an editor's viewport and how many rows
   * are. A row is one screen line of a possibly wrapped line, so without soft
   * wrap rows and buffer lines coincide.
   */
  export type Viewport = Readonly<{ first: number; rows: number }>;
  /** Where a document row starts: its 0-based buffer line and UTF-8 byte column. */
  export type RowStart = Readonly<{ line: number; column: number }>;
  /**
   * A fold over buffer lines: the 0-based `start` line, which stays visible,
   * through the `end` line, which is after it. Collapsed, it hides the lines
   * after `start` through `end`.
   */
  export type FoldRange = Readonly<{ start: number; end: number }>;
  /** A selection's ends as UTF-8 byte offsets; equal for a bare caret. */
  export type EditorSelection = Readonly<{ anchor: number; caret: number }>;
  /** One line of a buffer: its 0-based number, the UTF-8 byte offset where it starts, and its text without the
   * `\n` or `\r\n` terminator. */
  export type BufferLine = Readonly<{ line: number; startByteOffset: number; text: string }>;
  /** A selection read with `lines`: the caret's 0-based line and the lines from that many above it to that many
   * below it, clamped to the buffer, in order. */
  export type EditorSelectionWithLines = EditorSelection & Readonly<{ line: number; lines: readonly BufferLine[] }>;
  /** How an editor draws its caret. */
  export type CursorStyle = "bar" | "block" | "underline";
  export interface SetSelectionOptions {
    /**
     * How the view scrolls to show the caret. "nearest" (the default) scrolls
     * the minimum, as ordinary caret movement does; "center" centers the
     * caret's row when it is outside the viewport and otherwise keeps scroll;
     * "always-center" centers it even when it is on screen (Vim's `zz`);
     * "top" and "bottom" always put the caret's line at that edge of the
     * viewport, as far as the text allows (Vim's `zt` and `zb`).
     */
    readonly reveal?: "nearest" | "center" | "always-center" | "top" | "bottom";
  }
  /** A single-line text field of an editor bar. */
  export interface EditorBarField {
    /** Names the field within its bar; unique and non-empty. */
    readonly id: string;
    /** Shown while the field is empty. */
    readonly placeholder?: string;
    /**
     * Earlier entries, newest first. While the field has focus, Up replaces
     * its text with the next older entry that starts with the text typed
     * before recalling began, and Down steps back to newer ones, then to the
     * typed text. The entries are those of the call that opened the bar;
     * the extension owns, caps and persists them.
     */
    readonly history?: readonly string[];
  }
  /** A command a click on a bar's toggle or button runs, with the bar's
   * editor and buffer captured as the editor's own commands have them. */
  export interface EditorBarCommand {
    readonly name: string;
    readonly arguments?: JsonValue;
  }
  /** An icon button of an editor bar that runs a command. */
  export interface EditorBarButton {
    /** Names the button within its bar; unique among the bar's toggles and
     * buttons, and non-empty. */
    readonly id: string;
    readonly icon: IconRef;
    /** Shown while the pointer rests on the button. */
    readonly tooltip?: string;
    readonly command: EditorBarCommand;
  }
  /** An icon button of an editor bar that shows whether an option is on. A
   * click only runs `command`; the extension shows the new state by opening
   * the bar again with `active` changed. */
  export interface EditorBarToggle extends EditorBarButton {
    /** Defaults to false. */
    readonly active?: boolean;
  }
  /** What an editor bar shows: its fields, then its toggles, a line of text,
   * and its buttons. */
  export interface EditorBar {
    readonly fields?: readonly EditorBarField[];
    readonly toggles?: readonly EditorBarToggle[];
    readonly buttons?: readonly EditorBarButton[];
    readonly message?: string | null;
  }
  /** What an editor popover shows and where it floats. */
  export interface EditorPopover {
    /** The lines of the editor's buffer the popover is anchored to, `start`
     * through `end` exclusive. The anchor follows edits; deleting all of it
     * closes the popover. The popover floats below the last line, or above
     * the first when it does not fit below and does fit above, or on the
     * side with more room. It follows the anchor as the editor scrolls and
     * hides while the anchor is out of view or folded away. */
    readonly anchor: { readonly start: number; readonly end: number };
    /** How wide the popover is: logical pixels, which zoom scales, or
     * `"editor"` for the editor's text-area width (the default). It never
     * exceeds the text area. The height is the content's, up to the room
     * beside the anchor: an editor or text member is as tall as its lines up
     * to `maxLines`, a tree as its rows up to `maxRows`, a row as its tallest
     * member, a column the sum of its members, plus the toolbar. A member's
     * `size.pixels` in the popover are logical pixels as well. */
    readonly width?: "editor" | { readonly pixels: number };
    /** Whether the popover's focused member (the first, or the one marked
     * `focused`) takes focus when the popover opens, instead of the editor
     * keeping it. Default false. While the popover is open the editor holds
     * the key context `popover:<id>`. */
    readonly focus?: boolean;
    /** Icon buttons above the layout; each runs its command with the
     * anchoring editor's tab as its target and the editor as its editor,
     * wherever focus is. */
    readonly toolbar?: readonly CompositeButton[];
    /** Its content, as a composite view's: the members of a row or column,
     * with `key`s that keep their views when the popover is updated. Each
     * view is an ordinary view of the view tree, and an editor on a document
     * counts the popover's tab as one of the document's views. */
    readonly layout: CompositeGroup;
  }
  /** Why Knot closed a popover: `"escape"` was pressed, `"click"` landed outside
   * every popover, `"edit"` changed the anchoring editor's buffer (any edit
   * but one typed into a view inside a popover closes all of the editor's
   * popovers), or `"deleted"` removed its anchor. */
  export type EditorPopoverCloseReason = "escape" | "click" | "edit" | "deleted";
  /** Knot closed the caller's popover `popover`. */
  export type EditorPopoverClose = Readonly<{ popover: string; reason: EditorPopoverCloseReason }>;
  /** The user edited a field of the bar `bar`; `fields` holds every field's
   * text by ID. */
  export type EditorBarEdit = Readonly<{ bar: string; fields: Readonly<Record<string, string>> }>;
  /**
   * The document editor a command captured. It is usable within that
   * command's invocation tree, as CommandTarget is; every call rejects with
   * InvalidTarget once the editor's tab closes or moves, or the command ends.
   */
  export interface Editor {
    /**
     * The selection; with `lines`, also the caret's line and the lines around it, so a motion needs no further
     * read for them.
     * @throws HostError with InvalidTarget or Cancelled.
     */
    selection(): Promise<EditorSelection>;
    selection(options: { lines: number }): Promise<EditorSelectionWithLines>;
    /**
     * The text pasting `text` over `range` (UTF-8 offsets, default the selection) would insert: what the
     * paste provider of the language at the range's start answers, or `text` itself without
     * one or when it throws. The editor is not changed; the caller applies the text.
     * @throws HostError with InvalidTarget, InvalidRange, or Cancelled.
     */
    preparePaste(text: string, range?: [start: number, end: number]): Promise<string>;
    /**
     * Move the selection. Not an edit: it ends the current typing undo group,
     * as other caret moves do, and changes no revision.
     * @throws HostError with InvalidTarget, InvalidRange for an offset past the
     * end or splitting a UTF-8 scalar, or Cancelled; TypeError for an invalid reveal.
     */
    setSelection(selection: EditorSelection, options?: SetSelectionOptions): Promise<void>;
    /**
     * The rows wholly visible in the viewport, from the view's last layout,
     * so it may lag a scroll or resize by one frame. Always at least one row;
     * an editor that has not been laid out yet reports the first row.
     * @throws HostError with InvalidTarget or Cancelled.
     */
    viewport(): Promise<Viewport>;
    /**
     * The 0-based buffer line the document row `row` belongs to and the UTF-8
     * byte `column` where the row starts in it; a row past the last is the
     * last. Rows count as `viewport` and `scrollBy` do.
     * @throws HostError with InvalidTarget or Cancelled; TypeError for a
     * negative or non-integer row.
     */
    rowAt(row: number): Promise<RowStart>;
    /**
     * The document row that UTF-8 byte `column` of buffer line `line` shows on.
     * A column at a break shows on the row that starts there, as the view draws
     * the caret. Lines are 0-based.
     * @throws HostError with InvalidRange for a line past the last or a column
     * past the line's end or inside a character, or Cancelled.
     */
    rowOf(line: number, column: number): Promise<number>;
    /**
     * The display cells from the start of buffer line `line` to the UTF-8 byte
     * `column` in it, with tabs expanded from the line's start, measured as soft
     * wrap measures rows: a wide character takes two cells. Uses this editor's tab
     * size. Lines are 0-based.
     * @throws HostError with InvalidRange for a line past the last or a column
     * past the line's end or inside a character, or Cancelled.
     */
    cellsBefore(line: number, column: number): Promise<number>;
    /**
     * The UTF-8 byte column of the character covering display cell `cells` in
     * buffer line `line`, under the same measure as `cellsBefore`. A cell inside a
     * tab lands on the tab, and a cell past the line's end lands on its last
     * character, or on 0 for an empty line.
     * @throws HostError with InvalidRange for a line past the last, or Cancelled.
     */
    columnAtCells(line: number, cells: number): Promise<number>;
    /**
     * Collapse the lines of `range` into its first line, which shows a
     * placeholder. Rows, `viewport`, `rowAt`, `rowOf`, and `scrollBy` then
     * count only the lines no fold hides. A caret or selection end in the
     * hidden lines moves to the end of the first line and the selection is
     * cleared. Anything that puts the caret in hidden lines, such as
     * `setSelection`, expands the folds hiding it. A fold is per view, not an
     * edit, and follows the text as edits move it. Folding a range already
     * collapsed changes nothing; folds may nest.
     * @throws HostError with InvalidRange for a range that ends past the last
     * line, or Cancelled/InvalidTarget as other editor calls; TypeError for a
     * range that is not a safe-integer `start` below `end`.
     */
    fold(range: FoldRange): Promise<void>;
    /**
     * Expand the collapsed fold over exactly the lines of `range`, if there is
     * one. Folds nested inside it stay collapsed.
     * @throws HostError with InvalidTarget or Cancelled; TypeError as `fold`.
     */
    unfold(range: FoldRange): Promise<void>;
    /**
     * The collapsed folds, ordered by first line and then outermost first.
     * @throws HostError with InvalidTarget or Cancelled.
     */
    folds(): Promise<readonly FoldRange[]>;
    /**
     * The regions of the text that can fold, ordered by first line and then
     * outermost first, from the language's `folds` query or, without one,
     * from indentation. The Fold, Unfold, and Fold All commands choose among
     * them; `fold` accepts any range.
     * @throws HostError with InvalidTarget or Cancelled.
     */
    foldRegions(): Promise<readonly FoldRange[]>;
    /**
     * Scroll by rows, negative upward, without moving or revealing the
     * caret; the scroll stays within the text. Not an edit and not a caret
     * move, so it does not end an undo group.
     * @throws HostError with InvalidTarget or Cancelled; TypeError for a
     * non-integer.
     */
    scrollBy(rows: number): Promise<void>;
    /**
     * Set or clear a named key context on this editor, which bindings with a
     * matching `context` option require. The context belongs to the caller's
     * lifecycle: other lifecycles' contexts of the same name are independent,
     * and teardown clears it. It is presentation state, never recorded, and
     * lasts until cleared or the view closes.
     * @throws HostError with InvalidTarget or Cancelled; TypeError for an
     * empty name or a non-boolean `active`.
     */
    setKeyContext(name: string, active: boolean): Promise<void>;
    /**
     * Atomically replace the caller's decorations on this editor at the
     * required revision; empty items clear them. Only this view paints them,
     * so other views of the buffer stay clean, and they paint above the
     * buffer's contributions of equal precedence. Ranges follow the rules of
     * `EditorContributions.replace` and edits move them the same way. The set
     * belongs to the caller's lifecycle, as key contexts do: teardown clears
     * it. It is presentation state, never recorded, and lasts until replaced
     * or the view closes.
     * @throws HostError with RevisionConflict, InvalidRange, InvalidTarget, or
     * Cancelled; TypeError for a malformed item.
     */
    setDecorations(items: readonly ViewDecoration[], options: RevisionOptions): Promise<void>;
    /**
     * Atomically replace the caller's block rows on this editor; an empty list
     * clears them. Each block shows lines of a source buffer, in that buffer's
     * syntax highlighting, as read-only rows above a line of this editor. The
     * rows belong to no line of the text: the caret skips them, a click on one
     * lands at the start of the line below, and they take no line number. Rows
     * of several blocks on one line stack in the order given. They follow
     * their line through edits, repaint when the source's text or syntax
     * changes, and wrap with the editor. If the source ends, its rows keep the
     * text they last showed. The set belongs to the caller's lifecycle, as
     * decorations do: teardown clears it. It is presentation state, never
     * recorded, and lasts until replaced or the view closes.
     * @throws HostError with BufferClosed, InvalidTarget, or Cancelled;
     * TypeError for a malformed block or a source that is not a buffer;
     * RangeError for lines that end before they start.
     */
    setBlocks(blocks: readonly EditorBlockRows[]): Promise<void>;
    /**
     * Draw this editor's caret as a thin "bar" (the default), a "block" over
     * the character under it, or an "underline" beneath it; `null` clears the
     * caller's choice. Like key contexts the choice belongs to the caller's
     * lifecycle: other lifecycles' choices are independent, the latest set wins,
     * clearing reveals the previous one, and teardown clears it. It is
     * presentation state, never recorded, and lasts until cleared or the view
     * closes.
     * @throws HostError with InvalidTarget or Cancelled; TypeError for an
     * unknown style.
     */
    setCursorStyle(style: CursorStyle | null): Promise<void>;
    /**
     * Open a bar above this editor's text, or update the caller's bar `id` in
     * place. Bars stack in the order they were opened. An update keeps the
     * text of fields whose IDs it keeps; new fields start empty. The bar
     * belongs to the caller's lifecycle, as key contexts do: other lifecycles'
     * bars of the same ID are independent, and teardown closes it. It is
     * presentation state, never recorded, and lasts until closed or the view
     * closes. The editor stays editable while bars are open.
     * @throws HostError with InvalidTarget or Cancelled; TypeError for an
     * empty ID or a malformed bar; RangeError for empty or repeated field IDs, or empty or repeated toggle and button IDs.
     */
    openBar(id: string, bar: EditorBar): Promise<void>;
    /** Close the caller's bar `id`; closing a bar that is not open does
     * nothing. A field of it with focus returns focus to the text.
     * @throws HostError with InvalidTarget or Cancelled. */
    closeBar(id: string): Promise<void>;
    /**
     * Open a popover floating over this editor's text, anchored to a range of
     * its lines, or update the caller's popover `id` in place. Popovers
     * paint in the order they were opened, newest on top. The popover belongs
     * to the caller's lifecycle, as bars do: other lifecycles' popovers of
     * the same ID are independent, and teardown closes it. It is presentation
     * state, never recorded, and lasts until closed, its anchor is deleted,
     * or the view closes. Escape closes the focused popover, else the newest;
     * a click outside every popover closes all; any edit of the buffer except
     * one typed into a view inside a popover closes all of the editor's. Use
     * `onDidClosePopover` on a handle to learn of these.
     * @throws HostError with InvalidTarget, BufferClosed, or Cancelled;
     * TypeError for an empty ID or a malformed popover; RangeError for an
     * anchor without lines.
     */
    openPopover(id: string, popover: EditorPopover): Promise<void>;
    /** Close the caller's popover `id`; closing one that is not open does
     * nothing.
     * @throws HostError with InvalidTarget or Cancelled. */
    closePopover(id: string): Promise<void>;
    /**
     * Give a field of the caller's bar `id` the editor's focus in place of its
     * text, first replacing the field's text with `text`, all selected, when
     * given; line breaks are dropped. Clicking the text or a field moves focus
     * too. Replacing the text this way is not an edit `onDidEditBar` reports.
     * @throws HostError with BarNotFound when the caller has no such bar or
     * field, InvalidTarget, or Cancelled.
     */
    focusBarField(id: string, field: string, options?: { readonly text?: string }): Promise<void>;
    /** The text of each field of the caller's bar `id`, by field ID.
     * @throws HostError with BarNotFound, InvalidTarget, or Cancelled. */
    barFields(id: string): Promise<Readonly<Record<string, string>>>;
    /**
     * Make everything committed to this editor's buffer until the returned
     * group is disposed one undo step: this caller's edit batches, native
     * typing, paste and deletion, and other views' edits. Caret moves and
     * other native history breaks do not end it. Undoing the step restores
     * this view's selection from when the group began, and redoing it the
     * selection from when the group ended; a group with no commit leaves no
     * step. Undo and redo end an open group first.
     * The group belongs to the caller's lifecycle and also ends when its
     * lifecycle ends, when the editor handle is released, or when the handle's
     * tab closes.
     * @throws HostError with InvalidTarget, Cancelled, or OperationInProgress
     * when the buffer already has an open group.
     */
    beginUndoGroup(): Promise<Disposable>;
    /**
     * Start recording the input this editor receives from the keyboard: each
     * command a key or text binding runs (native commands that act on the window
     * rather than the editor, such as opening the palette, are left out), text typed or committed by an input
     * method, and each accepted completion, in order. Palette, menu,
     * scripted, and nested command invocations, mouse input, and composition
     * preedit are not recorded. Recordings are independent, so several can
     * be active at once. A recording ends when it is stopped or disposed,
     * when the editor handle is released, when the handle's tab closes, and
     * when the caller's lifecycle ends; stopping one that ended resolves with
     * no entries.
     * @throws HostError with InvalidTarget or Cancelled.
     */
    record(): Promise<Recording>;
    /** Selection and scroll, restorable exactly while the text is unchanged.
     * @throws HostError with InvalidTarget or Cancelled. */
    viewState(): Promise<EditorViewState>;
    /** Apply a view state; offsets clamp to the current text.
     * @throws HostError with InvalidTarget or Cancelled. */
    restoreViewState(state: EditorViewState): Promise<void>;
  }
  /** One recorded step of an editor's input, replayable with
   * `commands.invoke(command, arguments)` on the same editor. Typed text is
   * `editor.type` with `{ text }`, plus `replace`, the number of characters
   * before the caret an accepted completion replaced. */
  export interface RecordedCommand {
    readonly command: string;
    readonly arguments?: unknown;
  }
  /** An input recording of one editor; see `EditorHandle.record`. */
  export interface Recording extends Disposable {
    /** End the recording and resolve with its entries in order. Calling it
     * again, or after the recording ended, resolves with no entries. */
    stop(): Promise<readonly RecordedCommand[]>;
  }
  /**
   * Command cancellation signal with the supported interface below.
   * No browser AbortSignal inheritance, listener options or onabort property.
   * Cancellation does not roll back committed mutations.
   */
  export interface CancellationSignal {
    /** True after cancellation has been signalled. */
    readonly aborted: boolean;
    /** Cancellation reason, undefined before cancellation. */
    readonly reason: unknown;
    /** Subscribe before cancellation; no replay for an already-aborted signal. */
    addEventListener(type: "abort", listener: (event: CancellationEvent) => void): void;
    /** Remove the same listener function previously registered. */
    removeEventListener(type: "abort", listener: (event: CancellationEvent) => void): void;
    /** Throw reason when aborted; otherwise return normally. */
    throwIfAborted(): void;
  }
  /** Event delivered to cancellation listeners. */
  export interface CancellationEvent {
    readonly type: "abort";
    readonly target: CancellationSignal;
  }
  /**
   * Context captured at command admission, preserved across awaits and children.
   * Root, inline and cross-lifecycle handlers receive equivalent target authority.
   */
  export interface CommandContext {
    /** Originating window/presentation; unrelated targets are not authorized. */
    readonly target: CommandTarget;
    /** Captured text buffer, or null for a surface without a text buffer. */
    readonly buffer: Buffer | null;
    /** Captured document editor, or null for any other surface. */
    readonly editor: Editor | null;
    /** Caller-supplied JSON value, defaulting to null when omitted. */
    readonly arguments: JsonValue;
    /** Cancellation of this admitted command. */
    readonly signal: CancellationSignal;
  }
  /**
   * Awaited handler; return values do not become command results.
   * Throws become handlerFailure; invalidArguments produces invalidArgument
   * and unavailable produces unavailable.
   */
  export type CommandHandler = (context: CommandContext) => void | Promise<void>;
  /** How a command presents itself in the command palette. */
  export interface CommandOptions {
    /** Palette title; defaults to the command name. */
    readonly title?: string;
    /** Muted text shown beside the title. */
    readonly description?: string | null;
    /** False hides the command from the palette; it stays invocable. */
    readonly palette?: boolean;
  }
  /**
   * Execution status rather than a semantic result. Unavailable includes declined
   * routing and unsupported composition; invalidTarget identifies a stale capture.
   * InvalidArgument and handlerFailure include diagnostics in message.
   */
  export type CommandOutcome = Readonly<
    { kind: "completed" | "unavailable" | "invalidTarget" | "cancelled" }
    | { kind: "invalidArgument" | "handlerFailure"; message: string }
  >;
  /**
   * The icon a surface paints: an icon name such as `file` or `symbol`, or
   * `{ file }` or `{ folder }` with a file or folder name, which resolves
   * through the icon associations and the active pack when painted, so it
   * follows changes to them. A tree paints an expanded row's icon in its
   * `.open` form (`X.open` before `X`) when one is defined. An unknown name
   * paints a fallback icon.
   */
  export type IconRef = string | { readonly file: string } | { readonly folder: string };
  /** How a tree row was activated. */
  export type TreeGesture = "click" | "doubleClick" | "middleClick" | "rightClick" | "enter";
  /** The argument value of a tree row's command. */
  export interface TreeRowActivation {
    readonly gesture: TreeGesture;
    readonly arguments: JsonValue;
  }
  /**
   * One semantic command-backed action on a row, group header, or frame.
   * Arguments are JSON and are passed to the command as its argument value;
   * an absent value is null. Clicking an action never activates the row.
   * An action with an `icon` renders as an icon button that shows `label` as
   * its tooltip; otherwise it renders as a text button.
   */
  export interface TreeAction {
    readonly label: string;
    /** An icon such as `action.refresh`. */
    readonly icon?: IconRef;
    readonly command: string;
    readonly arguments?: JsonValue;
    /** Offer the action only in the context menu, not as a button on the row
     * or toolbar. */
    readonly menuOnly?: boolean;
    /** Show the button on a row only while the row is pointed at or selected. */
    readonly showOnHover?: boolean;
  }
  /**
   * Native tree semantics. Optional description, icon and command are nullable.
   * A group is a non-activatable, non-disclosure header that organizes child
   * rows; its children are fetched immediately and remain visible.
   */
  export interface TreeItem {
    /** Author-supplied identity, stable and unique within the provider tree. */
    readonly id: string;
    readonly label: string;
    readonly description?: string | null;
    /** Muted text under the label. It wraps, and the row grows to fit it. */
    readonly detail?: string | null;
    /** How many lines `detail` may wrap over before it is cut off, 1 to 255;
     * without it the whole text shows. */
    readonly detailMaxLines?: number | null;
    /** An icon such as `symbol`, `{ file: "main.rs" }`, or `{ folder: "src" }`. */
    readonly icon?: IconRef | null;
    /** Required expansion state; never inferred from child results. */
    readonly collapsibleState: "none" | "collapsed" | "expanded";
    /**
     * Activation command for ordinary item rows; ignored for groups. A row that
     * can expand toggles on click and Enter and runs no command. The command
     * receives `{ gesture, arguments }` as its argument value, with the
     * gesture that activated the row and this item's `arguments`.
     * A double click runs `click` and then `doubleClick`. `rightClick` is
     * delivered only for a row without actions, since actions open the
     * context menu instead.
     */
    readonly command?: string | null;
    /** Passed to the row's command as `arguments`; JSON, absent is null. */
    readonly arguments?: JsonValue;
    /** Row kind; defaults to "item". */
    readonly kind?: "item" | "group";
    /** Paint the row dimmed, as for an ignored file. */
    readonly muted?: boolean;
    /** Paint the label in a heavier weight, as for an unread entry. */
    readonly bold?: boolean;
    /** Right-aligned short status text, e.g. a Git change letter. */
    readonly statusText?: string | null;
    /** A badge dot in this key's color; put any short label in statusText. */
    readonly badge?: ColorKey | null;
    /** Row or group actions; frame actions belong to the contribution. */
    readonly actions?: readonly TreeAction[];
    /** Whether the row can be dragged onto a `dropTarget` row of its tree.
     * Needs the contribution's `drop` command. */
    readonly draggable?: boolean;
    /** Whether a dragged row can be dropped onto this row. */
    readonly dropTarget?: boolean;
    /** The resource URI the row stands for. A `draggable` row with one can
     * be dropped onto the editor area, which opens it as a tab. */
    readonly resource?: string;
  }
  /** The argument value of a tree's `drop` command. */
  export interface TreeDrop {
    /** The dragged row. */
    readonly source: { readonly id: string; readonly arguments: JsonValue };
    /** The row it was dropped onto. */
    readonly target: { readonly id: string; readonly arguments: JsonValue };
    /** Whether Option was held at drop time, asking for a copy. */
    readonly copy: boolean;
  }
  /** Explicit live context supplied to a contributed view's provider. */
  export interface ViewContext {
    /** Window owning this native instance. */
    readonly window: WindowTarget;
    /** Opaque native instance identity while this presentation stays live. */
    readonly instance: ViewId;
  }
  /** Tree callbacks have no implicit window, active buffer or public signal. */
  export interface TreeDataProvider {
    /**
     * Return children of parentId; null requests roots. Must return an iterable
     * of valid items, not a string or accidental array-like object.
     * Context identifies the requesting window and native instance explicitly.
     * Throws, rejections and invalid items become isolated request diagnostics.
     * Native generation validation discards stale results. Receiver cancellation
     * releases pending work without promising JavaScript stack unwind.
     */
    getChildren(parentId: string | null, context: ViewContext): Iterable<TreeItem> | Promise<Iterable<TreeItem>>;
  }
  /**
   * Views are tool windows, each docked in one of six slots around the
   * document center. A slot shows at most one tool window.
   */
  export type ViewLocation =
    | "leftTop"
    | "leftBottom"
    | "rightTop"
    | "rightBottom"
    | "bottomLeft"
    | "bottomRight";
  /** Initial placement; remembered user layout overrides it on later windows. */
  export interface ViewPlacement {
    /** Slot. Defaults to `leftTop`. */
    readonly location?: ViewLocation;
    /** Deterministic order among the tool windows of a slot; defaults after native tools. */
    readonly order?: number;
    /** Initial visibility. Defaults to true. */
    readonly visible?: boolean;
  }
  /**
   * Native content for one contributed view. A tree is populated through its
   * provider; a text frame starts empty and is replaced with setText.
   */
  export type ViewContent =
    | Readonly<{ kind: "tree"; provider: TreeDataProvider }>
    | Readonly<{ kind: "text" }>;
  /** One lifecycle-owned native view definition. */
  export interface ViewContribution {
    /** Stable, unique, namespaced identity; `knot.` is reserved. */
    readonly id: string;
    readonly title: string;
    /** Omitted for a container tool window, which holds the tabs opened into
     * it, such as terminals; see `terminals.open`. */
    readonly content?: ViewContent;
    /** A container's command that opens a tab in it: the `+` button runs it,
     * and so does showing the container while it has no tabs. */
    readonly newTab?: string;
    /** A tree's command that runs when a `draggable` row is dropped onto a
     * `dropTarget` row; it receives a {@link TreeDrop} and rejects a drop it
     * cannot do. */
    readonly drop?: string;
    /** Actions shown in the frame toolbar; only meaningful for tree frames. */
    readonly actions?: readonly TreeAction[];
    readonly placement?: ViewPlacement;
    /** Language ID a text frame's content is highlighted as; text frames only. */
    readonly language?: string;
    /** Whether a text frame shows the editor gutter of markers and line
     * numbers, which then follow `editor.lineNumbers` under the view's ID.
     * Text frames only; defaults to false. */
    readonly gutter?: boolean;
    /** Whether composite views host this view instead of a tool window:
     * `show`, `hide`, `focus`, and `toggle` do nothing. A composite's `tree`
     * or `text` member takes the registration, and each composite holding it
     * shows one instance, so they share its content. Tree and text frames
     * only; defaults to false. */
    readonly embedded?: boolean;
  }
  export interface TreeEditOptions {
    /** The initial text; defaults to empty. */
    readonly value?: string;
    /** UTF-16 range of `value` to select; all of it when omitted. */
    readonly selection?: readonly [start: number, end: number];
    /** Called with each typed value, and the initial one; a returned message
     * shows under the field and blocks the commit, nothing or null accepts.
     * Calls are serial; a throw stops further calls, accepts the values
     * typed after it, and is rethrown once the edit settles. */
    readonly validate?: (value: string) => string | null | undefined | Promise<string | null | undefined>;
  }
  export interface TreeNewItemOptions extends TreeEditOptions {
    /** The icon of the new row. */
    readonly icon?: IconRef;
  }
  /** Registration owns one contributed view across every live window. */
  export interface ViewRegistration extends Disposable {
    readonly id: string;
    /**
     * Show this view in every live window, or only in one explicit
     * window. Programmatic presentation never updates the fresh-window default.
     * @throws HostError with InvalidTarget for a stale window, or Cancelled on
     * teardown.
     */
    show(window?: WindowTarget): Promise<void>;
    /** Hide this view in every live window, or only in one window. */
    hide(window?: WindowTarget): Promise<void>;
    /** Show if hidden, then focus this view in every window or one window. */
    focus(window?: WindowTarget): Promise<void>;
    /** Show and focus this view where it is hidden or not focused, and hide it
     * where it is focused, returning focus to the center. */
    toggle(window?: WindowTarget): Promise<void>;
    /**
     * Paint a blue ("info") or red ("error") dot on the tool's stripe button
     * in every window, or clear it with null.
     * @throws HostError with OwnershipDisposed after disposal starts or
     * Cancelled on teardown.
     */
    setBadge(badge: "info" | "error" | null): Promise<void>;
    /**
     * Invalidate root children when parentId is omitted/null, otherwise that
     * parent's children. Omitting window invalidates every live instance; an
     * explicit window must own a live instance.
     * Resolution acknowledges native invalidation, not fetch or paint. Calls
     * accepted before disposal retain request order and settle before cleanup
     * acknowledgement.
     * @throws HostError with OwnershipDisposed after disposal starts,
     * ViewRegistrationNotFound for stale native ownership, ViewNotFound for a
     * window without a live instance, or Cancelled on teardown.
     */
    invalidate(parentId?: string | null, window?: WindowTarget): Promise<void>;
    /**
     * Turn the label of row `id` into a text field in the window the command
     * runs in. Enter or losing focus commits, and Escape dismisses. While
     * `validate` has not answered the typed value, a commit waits; a message
     * it returns shows under the field and blocks the commit. Keys typed
     * while a key-invoked command waits for the edit go to the field.
     * @throws HostError with ContextUnavailable outside a command,
     * TreeItemNotFound for a row that is not loaded, ViewNotFound, or
     * Cancelled.
     */
    editLabel(id: string, options?: TreeEditOptions): Promise<InputResult>;
    /**
     * Expand the row `parentId` and show a temporary row with an empty field
     * first among its children, in the window the command runs in. Committing
     * an empty field dismisses; otherwise it behaves like `editLabel`. The
     * caller creates the item and invalidates the parent.
     */
    editNewItem(parentId: string, options?: TreeNewItemOptions): Promise<InputResult>;
    /**
     * Expand each ancestor named by `idPath`, waiting for its children, then
     * select and scroll to the last ID, in every live window or one explicit
     * window. The path is the IDs from a root row down to the target; the
     * reveal stops where a row is missing. Resolution acknowledges the request,
     * not the expansion. Tree frames only.
     * @throws HostError with OwnershipDisposed after disposal starts,
     * ViewRegistrationNotFound for stale native ownership, ViewNotFound for a
     * window without a live instance, ContentKindMismatch for a text frame, or
     * Cancelled on teardown.
     */
    reveal(idPath: readonly string[], window?: WindowTarget): Promise<void>;
    /**
     * Replace the complete text of a text frame in every live window, or in
     * one explicit window. Tree frames reject with ContentKindMismatch.
     * The last text is retained for windows created later; this is not an
     * editable document and has no persistence or backing resource.
     * @throws HostError with ContentKindMismatch, ViewNotFound (explicit window
     * without a live instance), OwnershipDisposed, or Cancelled on teardown.
     */
    setText(text: string, window?: WindowTarget): Promise<void>;
  }
  /** One selectable row offered to a picker; fuzzy-matched locally by label,
   * then description. */
  export interface PickerItem {
    readonly id: string;
    readonly label: string;
    /** Muted text beside the label. */
    readonly description?: string | null;
    /** A muted second line. */
    readonly detail?: string | null;
    /** Right-aligned text, such as a keybinding. */
    readonly hint?: string | null;
    /** Painted before the label. */
    readonly icon?: IconRef | null;
    /** The picker opens with this row highlighted; the first such row wins. */
    readonly selected?: boolean;
    /** The IDs of the `tabs` this row shows in; it shows in every tab when
     * omitted or empty. Rows an `onQuery` returns are shown as given, so
     * the tag is for `items`. */
    readonly tabs?: readonly string[] | null;
  }
  /** One scope tab of a picker. */
  export interface PickerTab {
    readonly id: string;
    readonly label: string;
    /** Shown in the empty query field while this tab is active; the picker's
     * `title` shows when absent. Tab switches tabs, so it never accepts this. */
    readonly placeholder?: string;
  }
  /** Selection or explicit dismissal; dismissal is not an operational failure. */
  export type PickerResult = Readonly<
    { kind: "selected"; id: string } | { kind: "dismissed" }>;
  /** Accepted text or explicit dismissal. `highlighted` is the index of the
   * completion highlighted when the text was accepted, or null when none was:
   * no list, or one answering earlier text than the field now holds. */
  export type InputResult = Readonly<
    { kind: "accepted"; value: string; highlighted: number | null } | { kind: "dismissed" }>;
  /** Confirmed or dismissed native confirmation. */
  export type ConfirmationResult = Readonly<
    { kind: "confirmed" } | { kind: "dismissed" }>;
  /** Selected directory URI or explicit dismissal; dismissal is not a failure. */
  export type FolderPickerResult = Readonly<
    { kind: "selected"; uri: string } | { kind: "dismissed" }>;
  /** An explicit target or the admitted command's window is required; a
   * context-free call without either rejects ContextUnavailable. */
  export interface TransientTargetOptions { readonly target?: WindowTarget; }
  /** A row a picker's `onQuery` answers with. */
  export interface PickerRow extends PickerItem {
    /** UTF-16 ranges of `label` to highlight as matched, as `completion.match`
     * returns them. */
    readonly labelMatches?: readonly (readonly [start: number, end: number])[] | null;
    /** UTF-16 ranges of `description` to highlight as matched. */
    readonly descriptionMatches?: readonly (readonly [start: number, end: number])[] | null;
  }
  export interface PickerOptions extends TransientTargetOptions {
    readonly title: string;
    /** The rows to match locally; exclusive with `onQuery`. */
    readonly items?: readonly PickerItem[];
    /**
     * Supplies the rows in place of `items`. Called with the query, first
     * with the empty one and then whenever it changes; the picker does no
     * matching of its own and shows the rows in the order returned, with the
     * ranges they carry highlighted. Each query is passed as it changes, even
     * while an earlier call runs; only the answer to the latest call applies,
     * and answers still pending when the picker settles are dropped. A throw
     * from the latest call stops further calls and is rethrown once the
     * picker settles; an earlier call's throw is dropped with its answer. A
     * call still running after the picker settles loses the command's
     * target. `tab` is the active tab's ID,
     * or null without `tabs`; a tab change calls it again, and an answer
     * applies only while the field and the tab still hold what it answers.
     * `onHighlight` works as it does with `items`.
     */
    readonly onQuery?: (
      query: string,
      tab: string | null,
    ) => readonly PickerRow[] | Promise<readonly PickerRow[]>;
    /**
     * Scope tabs shown under the field, the first active; by convention an
     * "All" tab leads. The IDs must be unique. Tab and Shift+Tab switch
     * tabs, as the commands `picker.next-tab` and `picker.previous-tab`
     * bound in the `picker` view; clicking a tab does too. With `items`, a
     * row shows in the tabs its `tabs` names before matching.
     */
    readonly tabs?: readonly PickerTab[];
    /** The ID of the tab that starts active; the first by default.
     * @throws TypeError when it names none of `tabs`. */
    readonly initialTab?: string;
    /**
     * Called with the highlighted item's ID, or null when nothing matches,
     * first for the initial highlight and then whenever it changes. Calls are
     * serial; while one runs, later changes collapse into the latest, and the
     * picker's result waits for the call in flight. Undoing
     * a preview after dismissal is the caller's job. A throw stops further
     * calls and is rethrown once the picker settles.
     */
    readonly onHighlight?: (id: string | null) => void | Promise<void>;
  }
  /** One completion listed under an input. */
  export interface InputCompletion {
    readonly label: string;
    /** The whole new field text when the completion is accepted. */
    readonly value: string;
    /** Muted text after the label. */
    readonly description?: string;
    /** Right-aligned muted text, such as a keybinding. */
    readonly hint?: string;
    /** Painted before the label. */
    readonly icon?: IconRef;
    /** UTF-16 ranges of `label` to highlight as matched, as `completion.match`
     * returns them. */
    readonly matches?: readonly (readonly [start: number, end: number])[];
    /** UTF-16 ranges of `description` to highlight as matched. */
    readonly descriptionMatches?: readonly (readonly [start: number, end: number])[];
  }
  /** What an input's `onChange` may answer with besides a plain message. */
  export interface InputFeedback {
    /** The muted message line; omitted or null clears it. */
    readonly message?: string | null;
    /** Completions in display order; omitted or empty hides the list. */
    readonly items?: readonly InputCompletion[];
  }
  export interface InputOptions extends TransientTargetOptions {
    readonly title: string;
    /** Initial text; defaults to empty. */
    readonly value?: string;
    /**
     * Fixed text shown before the field, outside its value, as a command
     * line shows `:`. With a prompt, Backspace on an empty field dismisses
     * the input.
     */
    readonly prompt?: string;
    /**
     * Muted text shown in the empty field. Tab on an empty field copies it
     * into the field as an ordinary edit, so `onChange` runs. While a
     * completion row is highlighted (an empty field with `history` lists all
     * of it), Tab accepts that row instead.
     */
    readonly placeholder?: string;
    /**
     * Earlier entries, newest first. The entries matching the typed text, as
     * `completion.match` matches, head the completion list in that order,
     * before what `onChange` returns; an empty field lists the whole history.
     * They are chosen with the completion keys, and Enter still submits the
     * field's text. `highlighted` in the result counts only the items
     * `onChange` returned, and is null while a history row is highlighted.
     * The extension owns, caps and persists the entries.
     */
    readonly history?: readonly string[];
    /**
     * Called with the field's text, first with the initial value and then
     * whenever it changes. Calls are serial; while one runs, later changes
     * collapse into the latest, and the input's result waits for the call in
     * flight. A returned string shows as a muted message
     * line under the field; null or nothing clears it. An {@link InputFeedback}
     * object carries a message and a completion list. The list shows under
     * the field with its first row highlighted; Up, Down, Ctrl-N and Ctrl-P
     * move the highlight, and Tab replaces the field with its `value`, which
     * calls `onChange` again. Enter always submits the field as shown.
     * Feedback applies only while the field still holds the value it answers;
     * otherwise it is dropped. A throw, or a return value of another shape,
     * stops further calls and is rethrown once the input settles.
     */
    readonly onChange?: (
      value: string,
    ) => string | InputFeedback | null | void | Promise<string | InputFeedback | null | void>;
  }
  export interface ConfirmationOptions extends TransientTargetOptions {
    readonly title: string;
    readonly message?: string;
    readonly confirmLabel?: string;
    readonly cancelLabel?: string;
  }
  export type NoticeSeverity = "info" | "warning" | "error";
  /** A button on a notice that runs a command in the popup's window. */
  export interface NoticeAction {
    readonly title: string;
    readonly command: { readonly name: string; readonly arguments?: JsonValue };
  }
  export interface NoticeOptions extends TransientTargetOptions {
    readonly message: string;
    readonly title?: string;
    /** Defaults to "info". */
    readonly severity?: NoticeSeverity;
    /** Whether the popup hides on its own after `notifications.timeout`.
     * Defaults to true for "info" and false for the other severities. */
    readonly autoDismiss?: boolean;
    readonly actions?: readonly NoticeAction[];
    /** The window whose popup shows the notice; every window when omitted. */
    readonly target?: WindowTarget;
  }
  /** One notice in the application's history. */
  export interface Notice {
    readonly id: number;
    readonly message: string;
    readonly title: string | null;
    readonly severity: NoticeSeverity;
    readonly autoDismiss: boolean;
    readonly actions: readonly {
      readonly title: string;
      readonly command: string;
      readonly arguments: JsonValue;
    }[];
    /** The window the popup is limited to; null for every window. */
    readonly target: WindowTarget | null;
    /** The package that raised the notice, or "Knot" for native ones. */
    readonly source: string;
    /** Milliseconds since the Unix epoch. */
    readonly time: number;
    /** Whether the user closed its popup, ran an action, or it was marked read. */
    readonly read: boolean;
  }
  /** What the status bar or toolbar shows for one item. */
  export interface StatusItemOptions {
    /** An item with neither text nor icon is not shown, so a window's value
     * can hide an item there. */
    readonly text: string;
    /** An icon painted before the text, in `color` when one is set. */
    readonly icon?: IconRef;
    /** Shown while the pointer rests on the item. */
    readonly tooltip?: string;
    /** Run when the item is clicked, in the window that shows it. */
    readonly command?: { readonly name: string; readonly arguments?: JsonValue };
    /** A theme color key for the text, such as "warning". */
    readonly color?: string;
    /** Which end of the bar the item sits at. Defaults to "left". */
    readonly side?: "left" | "right";
    /** The bar that shows the item: the status bar below the workbench or the
     * toolbar above it. Defaults to "statusBar". */
    readonly placement?: "statusBar" | "toolbar";
    /** Higher values sit further left within a side; equal values keep
     * registration order. Defaults to 0. */
    readonly priority?: number;
  }
  /**
   * One status bar item. `update` replaces the whole value, so pass every
   * field each time. With a window it sets that window's own value, which
   * shadows the application-wide one until a null clears it.
   */
  export interface StatusItem extends Disposable {
    readonly id: string;
    update(options: StatusItemOptions): Promise<void>;
    update(options: StatusItemOptions | null, scope: { readonly window: WindowTarget }): Promise<void>;
  }
  /** Where `menus.add` places an item. `tab` is the right-click menu of a tab
   * header and `content` the one inside an editor or terminal; the `menubar`
   * locations are the application menu bar's menus. */
  export type MenuLocation =
    | "tab" | "content"
    | "menubar.file" | "menubar.edit" | "menubar.view" | "menubar.go";
  /** One command in a menu. */
  export interface MenuItemOptions {
    /** The command to run, with its menu's target: the clicked tab or
     * surface, not the focused one. An unavailable command does nothing. */
    readonly command: string;
    /** The label. Defaults to the command's title. */
    readonly title?: string;
    readonly arguments?: JsonValue;
    /** Items of a group sit together, groups in alphabetical order, with a
     * separator between them (`"1_close"`, `"2_split"`). Ungrouped items come
     * last. */
    readonly group?: string;
    /** Within a group, lower values come first, then titles. Defaults to 0. */
    readonly order?: number;
    /** Show the item only on surfaces of this view kind: `"editor"`,
     * `"terminal"`, or a contributed view ID. Not for menu bar locations. */
    readonly view?: string;
    /** Show the item only where the surface has this editor key context;
     * needs `view`. Not for menu bar locations. */
    readonly context?: string;
  }
  /** What one source paints on a resource. */
  export interface ResourceDecoration {
    /** A theme color key that tints the label in trees and the title of a
     * document tab. */
    readonly color?: string;
    /** Painted at the right of a tree row, in `color`, as an icon named in
     * the icon registry or as one or two characters such as "M". */
    readonly badge?: { readonly icon: string } | { readonly text: string };
    /** Shown while the pointer rests on the row or tab. */
    readonly tooltip?: string;
    /** Also tint the ancestors of the resource with `color`, which only
     * colors them. Defaults to false. */
    readonly propagate?: boolean;
  }
  /**
   * One source of resource decorations, which Knot paints wherever it shows a
   * resource. The source keeps its own map from URI to decoration, and
   * disposing it or tearing down the lifecycle removes all of them.
   *
   * When several sources decorate a URI, the color and the badge are each
   * taken from the `error`, `warning`, or `info` decoration first, then from
   * the most recently created source; tooltips are joined, one per line. A
   * propagated color ranks below a color set directly on the ancestor.
   */
  export interface DecorationSource extends Disposable {
    /** Change the named URIs, leaving the others as they are; null removes a
     * URI's decoration. URIs are normalized as elsewhere, and decorations
     * apply in every window.
     * @throws TypeError for a malformed decoration, an unknown field, an
     * empty icon name, or badge text that is not one or two characters;
     * HostError with InvalidUri or Cancelled. */
    set(decorations: Readonly<Record<string, ResourceDecoration | null>>): Promise<void>;
    /** Remove every decoration of this source. */
    clear(): Promise<void>;
  }
  export const decorations: Readonly<{
    /** Create a source owned by this lifecycle. */
    createSource(): Promise<DecorationSource>;
  }>;
  export const menus: Readonly<{
    /** Adds a command to a menu until the returned handle or the owning
     * lifecycle is disposed.
     * @throws TypeError for an unknown location, a bad item, or a view or
     * context on a menu bar location. */
    add(location: MenuLocation, item: MenuItemOptions): Promise<Disposable>;
  }>;

  /** What a served completion or paste method receives beside its argument. */
  export interface ProviderCallContext {
    /** The window of the command the caller runs in; null outside a command
     * with a window. */
    readonly window: WindowTarget | null;
  }

  /** What the editor asks a completion provider for. */
  export interface CompletionRequest {
    /** The document being completed; `documents.get(document)` gives its
     * buffer, which a provider can keep while the document is open. */
    readonly document: DocumentId;
    readonly revision: number;
    /** Absolute UTF-8 cursor coordinate at revision. */
    readonly cursorByteOffset: number;
    /** The word before the cursor that a candidate replaces. */
    readonly prefix: string;
  }
  /** Candidate display label and insertion text; both are required strings. */
  export interface CompletionItem { readonly label: string; readonly insertText: string; }
  /**
   * An editor completion provider, served with `completionContract.serve`.
   * The editor asks every registration, oldest first, and merges their items;
   * a later registration adds to the others rather than replacing them.
   */
  export interface CompletionProvider {
    /**
     * Return the candidates for `request`, in the editor's window. A throw or
     * an invalid list fails only this provider's part of the completion, and
     * results for a revision the buffer has left are discarded.
     */
    provideCompletions(request: CompletionRequest, context: ProviderCallContext): readonly CompletionItem[] | Promise<readonly CompletionItem[]>;
  }
  /** What native paste asks a paste provider for. */
  export interface PasteRequest {
    readonly document: DocumentId;
    readonly revision: number;
    /** The text being pasted. */
    readonly text: string;
    /** The selection the paste replaces, as absolute UTF-8 coordinates at revision. */
    readonly range: readonly [number, number];
    /** The text in `range`; empty for a caret. */
    readonly selectedText: string;
  }
  /**
   * A language's paste provider, served with `pasteContract(languageId).serve`,
   * where the language is the one at the start of the selection (inside a
   * Markdown code fence, the fence's language). The latest registration for
   * the language answers; a language with none pastes synchronously.
   */
  export interface PasteProvider {
    /**
     * Return the text to insert instead of `request.text`, or null to insert
     * it unchanged. A throw or an invalid result inserts `request.text`. The
     * editor makes the edit, as one undo step; an answer for a revision or
     * selection the editor has left is discarded.
     */
    providePaste(request: PasteRequest, context: ProviderCallContext): string | null | Promise<string | null>;
  }
  /**
   * A contract Knot defines and asks itself: any package serves it, with no
   * `requires` entry, and none calls it through a client. The host validates
   * each result against Knot's schema, so a malformed answer fails only that
   * provider's part.
   */
  export type KnotContract<M extends import("knot/contract").ContractMethods> =
    Pick<import("knot/contract").Contract<M>, "name" | "methods" | "serve">;
  /** The completion contract, owned by `knot`. */
  export const completionContract: KnotContract<{
    readonly provideCompletions: import("knot/contract").FnSchema<CompletionRequest, readonly CompletionItem[]>;
  }>;
  /**
   * The paste contract of one language, owned by `knot`.
   * @throws TypeError for an empty language ID.
   */
  export function pasteContract(languageId: string): KnotContract<{
    readonly providePaste: import("knot/contract").FnSchema<PasteRequest, string | null>;
  }>;
  /** Explicit buffers. */
  export const editor: Readonly<{
    /**
     * Resolve the originating command's text buffer; a valid nontext capture
     * returns null. Omission uses the current frame; an explicit target must be
     * its host-issued capture or authorized inherited capture. Never reads focus.
     * @throws HostError with ContextUnavailable outside a command, InvalidTarget
     * for forged/stale/closed/replaced captures, or Cancelled after cancellation.
     */
    activeBuffer(target?: CommandTarget): Promise<Buffer | null>;
  }>;
  /** A command as `commands.list` reports it. */
  export interface CommandInfo {
    readonly name: string;
    readonly title: string;
    readonly description: string | null;
    /** The key binding shown for the view given to `commands.list`, formatted
     * for display; null when it has none. */
    readonly key: string | null;
  }
  /** Register handlers and compose child invocations with captured context. */
  export const commands: Readonly<{
    /**
     * Register a global command name. The palette shows its title, which
     * defaults to the name, and description, and matches the name too.
     * Valid during activation. Resolves when native ownership exists;
     * teardown races reject instead of publishing a late callable handler.
     * @throws TypeError for invalid arguments; HostError with CommandNameInUse
     * when the name is already owned, or Cancelled on teardown.
     */
    register(name: string, handler: CommandHandler, options?: CommandOptions): Promise<Disposable>;
    /**
     * Handle an existing command for one registered contribution ID. Native
     * routing binds live instances and preserves the captured presentation.
     * Resolution acknowledges registration; disposal removes its bindings.
     * @throws HostError with ViewNotFound, CommandNotFound, CommandNameInUse
     * or Cancelled; TypeError for invalid argument types.
     */
    registerForView(viewKind: string, name: string, handler: CommandHandler): Promise<Disposable>;
    /**
     * Invoke a child of the current command; arguments default to null.
     * Inherits the originating window/buffer and resolves with execution status.
     * Children may run at once (for example under `Promise.all`), and
     * cancelling the command cancels each of them; a handler already running
     * in the command's own ancestry is unavailable.
     * @throws HostError with ContextUnavailable outside a command, or Cancelled
     * for transport cancellation; TypeError for non-JSON-compatible arguments.
     */
    invoke(name: string, arguments_?: JsonValue): Promise<CommandOutcome>;
    /** The commands the palette lists, by title: the ones that did not opt out
     * of the palette. Any of them can be run with `invoke`. `key` is the
     * binding shown for `view`'s kind of view, or the global one without it.
     * @throws HostError with InvalidTarget when `view` is not live. */
    list(view?: ViewTarget): Promise<readonly CommandInfo[]>;
    /** Throw an author diagnostic converted by the handler to invalidArgument. */
    invalidArguments(message: string): never;
    /** Throw to end the handler with the unavailable outcome, as a command
     * that does not apply to its target reports. */
    unavailable(): never;
  }>;
  export type PresentationCloseOutcome = Readonly<{ kind: "closed" | "cancelled" }>;
  /** A composite view kind, registered with `workbench.registerComposite`. */
  export interface CompositeContribution {
    /** Stable, unique, namespaced identity; `knot.` is reserved. */
    readonly id: string;
  }
  /** What a composite view's tab shows. */
  export interface CompositeTab {
    /** The tab title; long titles truncate, with the full title in a tooltip. */
    readonly title: string;
    readonly icon?: IconRef;
    /** A resource URI whose decorations, such as a Git status color, the tab
     * shows as a document tab does. */
    readonly resource?: string;
  }
  /** An icon button of a composite's toolbar. Clicking it runs the command
   * with the composite's tab as the target and the view focused there as its
   * focus, wherever focus is. */
  export interface CompositeButton {
    /** The tooltip. */
    readonly label: string;
    readonly icon: IconRef;
    readonly command: string;
    readonly arguments?: JsonValue;
    /** Shows the button pressed. Defaults to false. */
    readonly toggled?: boolean;
    /** Dims the button and ignores clicks. Defaults to false. */
    readonly disabled?: boolean;
  }
  /** How much of its group's main axis a layout member takes. */
  export type CompositeSize =
    /** A share of the space the `pixels` members leave, in proportion to the
     * other weights. Defaults to `{ weight: 1 }`. */
    | { readonly weight: number }
    | { readonly pixels: number };
  interface CompositeLayoutMember {
    /** In a popover, the most lines an `editor` or `text` member shows
     * before it scrolls (default 12). Not for composite views. */
    readonly maxLines?: number;
    /** In a popover, the most rows a `tree` member shows before it scrolls
     * (default 12). Not for composite views. */
    readonly maxRows?: number;
    /** Names the member across `update`. A member of the new layout with the
     * key of one in the old and the same content keeps its view, with its
     * handle, state, and focus. Unique in the layout and not empty; a member
     * without a key is always new. */
    readonly key?: string;
    readonly size?: CompositeSize;
    /** The member that has focus when the tab opens; a group gives it to its
     * first view, and tabs show the member. At most one member of a group
     * sets it. When none does, the first view in layout order has focus.
     * Later, the view focused last has it when the tab is selected again. */
    readonly focused?: boolean;
    /** The label of the member's tab. Required for a member of `tabs`, and
     * for no other member. */
    readonly title?: string;
  }
  /** An editor on a buffer of `buffers.create` or on a document. A document's
   * editor edits the document and counts the composite as one of its views,
   * once however many editors the composite holds on it: the tab asks about
   * unsaved changes when closing it would close the document's last view. */
  export type CompositeEditor = CompositeLayoutMember & {
    readonly type: "editor";
  } & (
    | { readonly buffer: ExtensionBuffer; readonly document?: never }
    | { readonly document: DocumentId; readonly buffer?: never }
  );
  /** A text frame of a view registered with `embedded`. */
  export interface CompositeText extends CompositeLayoutMember {
    readonly type: "text";
    readonly view: ViewRegistration;
  }
  /** A tree of a view registered with `embedded`. */
  export interface CompositeTree extends CompositeLayoutMember {
    readonly type: "tree";
    readonly view: ViewRegistration;
  }
  /** A group of members side by side (`row`) or stacked (`column`). A group
   * inside another is a composite view of its own in the view tree. */
  export interface CompositeGroup extends CompositeLayoutMember {
    readonly type: "row" | "column";
    readonly children: readonly CompositeLayout[];
  }
  /** Members of which one shows at a time, chosen from a strip of their
   * titles. A composite view of its own in the view tree, whose children are
   * all the members; the shown one holds focus. */
  export interface CompositeTabs extends CompositeLayoutMember {
    readonly type: "tabs";
    readonly children: readonly CompositeLayout[];
  }
  /**
   * A strip that joins line ranges of two editor members of the layout, as
   * a band between the lines of each, following both editors' scrolling. It
   * is not a view: it has no key, focus, or place in the view tree, and is
   * not a member of `tabs`. Place it between the editors in a `row`. Its
   * `size` defaults to `{ pixels: 32 }`. It joins editors by `key`, so both
   * keys must name editor members of the layout, and different ones; a
   * connector that does not is a TypeError. Send the links again with an
   * `update` of the layout when the texts change.
   */
  export interface CompositeConnector {
    readonly type: "connector";
    readonly size?: CompositeSize;
    /** The `key` of the editor whose lines the left edge follows. */
    readonly first: string;
    /** The `key` of the editor whose lines the right edge follows. */
    readonly second: string;
    readonly links: readonly CompositeConnectorLink[];
  }
  export interface CompositeConnectorLink {
    /** The lines of the first editor the band's left edge spans. */
    readonly first: LineRange;
    /** The lines of the second editor the band's right edge spans. */
    readonly second: LineRange;
    /** The theme color key the band is filled with, such as
     * `diff.added.background`. */
    readonly color: string;
  }
  export type CompositeLayout = CompositeEditor | CompositeText | CompositeTree | CompositeGroup | CompositeTabs | CompositeConnector;
  /** Which lines of two editors lie side by side, so scrolling one scrolls
   * the other to the same place. */
  export interface CompositeScrollLink {
    /** The `key` of the first editor member. */
    readonly first: string;
    /** The `key` of the second editor member, another one. */
    readonly second: string;
    /**
     * The regions where the two texts differ, ascending and disjoint, as
     * `diff` returns them with `old` as the first editor's lines and `new`
     * as the second's. Lines between regions match one for one; the lines of
     * a region map proportionally onto the other side's, and a region with
     * no lines on one side sits at the boundary the other's lines belong to.
     */
    readonly map: readonly { readonly first: LineRange; readonly second: LineRange }[];
  }
  export interface OpenCompositeOptions extends DocumentDestinationOptions, CompositeTab {
    /** Links the scrolling of two editor members; the first leads at the
     * start. A link naming a key that no editor member has, or the same twice,
     * is an InvalidTarget. */
    readonly scrollLink?: CompositeScrollLink;
    /** The layout, which the user cannot rearrange: a row or column holding at
     * least one editor, text frame, or tree at any depth. */
    readonly layout: CompositeGroup;
    /** The buttons of a bar above the layout, in order; none shows no bar. */
    readonly toolbar?: readonly CompositeButton[];
  }
  /** An open composite view. Disposing it closes the tab, as `close` does. */
  export interface CompositeInstance extends AsyncDisposable {
    readonly target: ViewTarget;
    /**
     * Change what the tab shows; a field left out keeps its value and a
     * `null` icon or resource clears it. A `layout` replaces the old one:
     * a keyed member that shows the same thing, the same buffer or document
     * for an editor and the same embedded view for a text frame or tree,
     * keeps its view, and a group keeps its identity with its members
     * replaced the same way. Other members are new views, and views left out
     * close. The view that had focus keeps it; when it closes, the new
     * layout's initial focus takes over as on open, and a `focused` member
     * that is not that does not move focus. A `toolbar` replaces the buttons,
     * so state changes by sending the list again; `[]` removes the bar. A
     * `scrollLink` replaces the link and brings the second editor in line with
     * the first; `null` removes it. A layout
     * without a key the link names needs a new link or `null` with it.
     * @throws TypeError for invalid fields or layout; HostError with
     * InvalidTarget for a tab that was closed or a scroll link, given or kept, that
     * does not join two editors of the layout, InvalidUri, BufferClosed, DocumentNotFound,
     * DocumentUnsaved when the layout drops the final view of
     * a document with unsaved changes, OwnershipDisposed after disposal, or
     * Cancelled. A failed update changes nothing.
     */
    update(changes: Partial<Omit<CompositeTab, "icon" | "resource">> & {
      readonly icon?: IconRef | null;
      readonly resource?: string | null;
      readonly layout?: CompositeGroup;
      readonly toolbar?: readonly CompositeButton[];
      readonly scrollLink?: CompositeScrollLink | null;
    }): Promise<void>;
    /** Close the tab as `workbench.close` does; a tab already closed resolves
     * as closed. */
    close(options?: CloseOptions): Promise<PresentationCloseOutcome>;
  }
  /** Owns a composite view kind. Disposing it closes its instances. */
  export interface CompositeRegistration extends AsyncDisposable {
    readonly id: string;
    /**
     * Open an instance as a center tab of the destination's window and focus
     * it. Instances are not restored with their window. The caller's
     * lifecycle owns the tab: it closes when the extension unloads, the
     * instance or this registration is disposed.
     * @throws TypeError for invalid options, a layout without a view, a buffer
     * that is not this package's, or a view that is not embedded; HostError with
     * BufferClosed, InvalidUri, InvalidTarget for a stale destination or a
     * view whose registration ended, OwnershipDisposed after disposal, or
     * Cancelled.
     */
    open(options: OpenCompositeOptions): Promise<CompositeInstance>;
  }
  export interface CloseOptions {
    /** Close a dirty final view without the Save prompt, discarding its
     * changes. Defaults to false. */
    readonly discard?: boolean;
  }
  /** One document reference of a session window. A live document matching
   * `document` or `uri` is always presented and any capture is ignored; else
   * the capture is restored, else the file is opened. */
  export interface SessionEntry {
    readonly uri?: string | null;
    readonly document?: DocumentId | null;
    /** A capture retained by this owner. */
    readonly capture?: CaptureId | null;
  }
  /** A document presented in a released window. */
  export interface SuspendedDocument {
    readonly document: DocumentId;
    /** Required when the document is dirty or a non-empty untitled document
     * and its final view is removed; it must be retained at the document's
     * current revision. */
    readonly capture: CaptureId | null;
  }
  /** Restorable editor state as UTF-8 byte offsets; top is the start of the
   * first visible line. Offsets clamp to shorter text on restore. */
  export type EditorViewState = Readonly<{
    anchor: number;
    caret: number;
    top: number;
    /** Horizontal scroll in logical pixels. */
    scrollX: number;
    /** Collapsed folds as byte ranges, in a window layout's tab records. The
     * editor handle's `viewState` omits them and `restoreViewState` leaves
     * folds as they are; use `folds` and `fold` there. */
    folds?: readonly Readonly<{ start: number; end: number }>[];
  }>;
  /** Each tab kind records only what recreates it in the same position. A
   * terminal tab records its shell's working directory as a file URI and
   * restores as a fresh shell started there, or in the window's default
   * directory when it no longer exists; no process or content survives.
   * A document tab records its document's explicit language, if any;
   * restoration applies it unless the live document already has an explicit
   * language. */
  export type LayoutTab =
    | Readonly<{ kind: "document"; document: DocumentId | null; uri: string | null; view: EditorViewState; language?: string }>
    | Readonly<{ kind: "terminal"; directory?: string }>;
  export type LayoutNode =
    | Readonly<{ kind: "pane"; selected: number; tabs: readonly LayoutTab[] }>
    | Readonly<{
        kind: "split";
        direction: "horizontal" | "vertical";
        /** The fraction of the split's extent given to `first`. */
        ratio: number;
        first: LayoutNode;
        second: LayoutNode;
      }>;
  /** Logical bounds, presentation state, and the display UUID. */
  export type WindowGeometry = Readonly<{
    x: number;
    y: number;
    width: number;
    height: number;
    state: "windowed" | "maximized" | "fullscreen";
    display: string | null;
  }>;
  /** Versioned, JSON-compatible window layout from workbench.captureLayout.
   * Store it verbatim; it holds no live handles and is assignable to JsonValue. */
  export type WindowLayout = Readonly<{
    version: 3;
    geometry: WindowGeometry | null;
    /** The document center's split tree; documents and terminals restore
     * here and in container tool windows. */
    center: LayoutNode | null;
    /** The tool windows, by slot order. */
    tools: readonly LayoutTool[];
    /** Index of the focused pane among the center's panes and the container
     * tool windows' panes, in layout order. */
    focused: number | null;
  }>;
  /** One tool window: where it docks, whether it shows and has a stripe
   * button, its size and its share of its side when both slots show, and a
   * container's split tree of terminals. A hidden tool window keeps its
   * arrangement. */
  export type LayoutTool = Readonly<{
    tool: string;
    slot: ViewLocation;
    shown: boolean;
    sidebar: boolean;
    size: number;
    share: number;
    root: LayoutNode | null;
  }>;
  export interface SessionWindow {
    /** Ordered directory roots, validated and deduplicated as in
     * workspaces.replaceRoots. */
    readonly roots: readonly string[];
    readonly entries: readonly SessionEntry[];
    /** Index of the entry whose tab is selected when no layout applies. */
    readonly selected?: number | null;
    /** A captured layout. Each document tab resolves to the entry with the
     * same document, else the same URI; unmatched document tabs, unregistered
     * tools, and repeated documents in a pane are pruned, then empty panes and
     * one-child splits collapse. Entries the layout does not place open in the
     * normal group, and tools it does not name are hidden. An unsupported
     * version or shape is ignored. Geometry is fitted to connected displays. */
    readonly layout?: WindowLayout | JsonValue | null;
  }
  export interface ReplaceWindowsOptions {
    /** Every document presented in the released windows, exactly. */
    readonly suspend: readonly SuspendedDocument[];
    /** Destination windows in order; each one is created. */
    readonly windows: readonly SessionWindow[];
    /** Windows leaving the session. They close as any window does, stopping
     * their terminals. */
    readonly release?: readonly WindowTarget[];
    /** Index of the destination window that receives focus. */
    readonly focus?: number | null;
  }
  /** How one entry resolved. "live" presented an existing document; "missing"
   * means a clean file or untitled document no longer exists; "failed" entries
   * were skipped with a host error code while the rest committed. */
  export type SessionEntryOutcome = Readonly<
    | { kind: "live" | "opened"; document: DocumentId }
    | { kind: "restored"; document: DocumentId; resource: "untitled" | "unchanged" | "missing" | "changed" }
    | { kind: "missing" }
    | { kind: "failed"; code: HostErrorCode; message: string }>;
  export interface ReplaceWindowsResult {
    /** Created windows in destination order. layout is null without a layout
     * and "ignored" for an unsupported version or shape. */
    readonly windows: readonly Readonly<{ window: WindowTarget; entries: readonly SessionEntryOutcome[];
      layout: "applied" | "ignored" | null }>[];
    /** True when every restored dirty document was durably protected by
     * automatic recovery before resolution, so its captures may be released. */
    readonly protected: boolean;
  }
  /**
   * An editor that outlives the command that found it, bound to one exact
   * editor view. Every call resolves that view and rejects InvalidTarget once
   * the view leaves its tab's tree or its window closes; it never follows
   * focus or retargets a replacement. Disposing it, its lifecycle ending, or
   * its view going away releases its listeners, though a key context it set
   * stays until cleared or the lifecycle ends.
   */
  export interface EditorHandle extends Editor, Disposable {
    /** Runs once when the view goes away, so a mode can drop its
     * per-editor state. Not run for disposal or lifecycle teardown. Registered
     * after the view already went away, it runs asynchronously once. */
    onDidClose(listener: () => void | Promise<void>): Disposable;
    /**
     * Runs after the selection changes for any reason, including caret motion
     * caused by edits. The listener receives the selection as of when its turn
     * starts, so it needs no `selection()` read. Notifications coalesce: changes while one is queued join it, and a
     * listener still running when more arrive gets one further notification,
     * never a queue. Native painting and input never wait on a listener.
     * Resolves once the editor is being followed; changes before then are not
     * reported. @throws HostError with InvalidTarget or Cancelled.
     */
    onDidChangeSelection(listener: (selection: EditorSelection) => void | Promise<void>): Promise<Disposable>;
    /**
     * As above, and `lines` asks for the lines around the caret as `Editor.selection` does. A handle reads the
     * widest window any of its listeners asked for, so a listener may receive more lines than it asked for.
     */
    onDidChangeSelection(
      listener: (selection: EditorSelectionWithLines) => void | Promise<void>,
      options: { lines: number },
    ): Promise<Disposable>;
    /**
     * Runs after the user edits a field of one of the caller's bars on this
     * editor, by typing, deleting, or pasting, with the bar's field texts at
     * the time the listener turn starts. Notifications coalesce as
     * `onDidChangeSelection`'s do: edits to one bar while a notification is
     * queued join it, and a bar is reported once per turn.
     * @throws HostError with InvalidTarget or Cancelled.
     */
    onDidEditBar(listener: (edit: EditorBarEdit) => void | Promise<void>): Promise<Disposable>;
    /**
     * Runs after Knot closes one of the caller's popovers on this editor, with
     * the popover's ID and the reason. Closing it with `closePopover`, the
     * editor closing, and the package unloading report nothing. Closes while
     * a notification is queued join it, one call per popover.
     * @throws HostError with InvalidTarget or Cancelled.
     */
    onDidClosePopover(listener: (close: EditorPopoverClose) => void | Promise<void>): Promise<Disposable>;
  }
  /** Contributed view registration, captured selection, and clipboard access. */
  export const workbench: Readonly<{
    /** Atomically register and capture current metadata plus sequence watermark.
     * Later committed changes arrive as asynchronous FIFO listener roots after the
     * observing root finishes. Queries may already see newer state. Exceptions are
     * isolated; disposal/unload prevents queued author callbacks. Awaiting another
     * host request or self-disposal from a listener is supported. No replay/retries.
     * Valid during activation before windows exist; operations do not await listeners.
     */
    observe(listener: (event: ApplicationEvent) => void | Promise<void>): Promise<WorkbenchObservation>;
    /** Closes this exact native tab through protected closure. Dirty final views
     * offer Save / Don't Save / Cancel. Cancel returns { kind: "cancelled" };
     * a committed close returns { kind: "closed" } without awaiting observers.
     * Other presentations keep a shared document live. Replaced/transferred/closed
     * targets reject InvalidTarget; concurrent closure rejects OperationInProgress;
     * save failures reject PersistenceFailed and leave the presentation intact.
     * Caller cancellation/unload before closure commits prevents the close; an
     * accepted save may already have persisted content and cannot be rolled back.
     * With `discard`, a dirty final view closes without asking and its changes
     * are discarded; an edit after the call keeps it open and rejects
     * InvalidTarget, and unsaved-document recovery keeps the text until the
     * close commits.
     * @throws TypeError for invalid options. */
    close(target: ViewTarget, options?: CloseOptions): Promise<PresentationCloseOutcome>;
    /**
     * Register a composite view kind: a center tab whose views the package
     * lays out and the user cannot rearrange. The ID is unique across
     * packages.
     * @throws HostError with ViewIdInUse, InvalidViewId or Cancelled; TypeError
     * for an invalid contribution.
     */
    registerComposite(contribution: CompositeContribution): Promise<CompositeRegistration>;
    /** A long-lived editor for the editor view `target` names, at any depth of
     * a tab's view tree; targets come from observation. @throws HostError with
     * InvalidTarget for a target that is not a live editor view, a tab target
     * included, or Cancelled. */
    editor(target: ViewNodeTarget): Promise<EditorHandle>;
    /** Close a set of windows and open their replacements in one foreground
     * commit, without prompts: final-view text must already be retained.
     * Resources and captures are read before the commit; a commit then
     * revalidates the released windows, the suspended documents, and each
     * retained revision, so observers see only the committed result. Every
     * destination window is created with its layout and geometry before the
     * released windows close. Restoration never changes the fresh-window tool
     * default. Rejection changes nothing.
     * @throws HostError with OperationInProgress during closure or another
     * replacement, SessionConflict when an edit, tab, or document changed after
     * the caller's capture, InvalidTarget, Cancelled (caller cancellation or
     * unload before the commit), InvalidUri, or the
     * workspaces.openFolder resource codes for a destination root. */
    replaceWindows(options: ReplaceWindowsOptions): Promise<ReplaceWindowsResult>;
    /** Capture a window's layout for a later replaceWindows.
     * @throws HostError with InvalidTarget for a closed window, or Cancelled. */
    captureLayout(window: WindowTarget): Promise<WindowLayout>;
    /**
     * Register a lifecycle-owned native view, even before windows exist. The
     * contribution ID is the native view kind and command/keybinding scope.
     * Every window receives at most one live instance. Resolution
     * acknowledges native ownership; teardown racing registration rejects
     * instead of publishing a late callable object.
     * @throws HostError with ViewIdInUse, InvalidViewId or Cancelled; TypeError
     * for invalid fields or a missing getChildren method.
     */
    registerView(contribution: ViewContribution): Promise<ViewRegistration>;
    /** The document whose tab in `window` most recently had focus, or null
     * when none did. A focused terminal leaves the last editor's document
     * here. */
    lastFocusedDocument(window: WindowTarget): Promise<DocumentInfo | null>;
    /**
     * Present a searchable single-selection picker with a native text field and
     * local fuzzy matching. Resolution is a typed selection or dismissal.
     * @throws TypeError for invalid options; HostError with OperationInProgress
     * when another extension interaction owns the window, InvalidTarget for a
     * stale target, ContextUnavailable without a target or command capture, or
     * Cancelled on window closure, unload or caller cancellation.
     */
    showPicker(options: PickerOptions): Promise<PickerResult>;
    /**
     * Present a single-line text field with a title and optional initial text.
     * Accepting returns the current text; Escape returns dismissal.
     * @throws TypeError for invalid options; HostError with the same set as
     * showPicker.
     */
    showInput(options: InputOptions): Promise<InputResult>;
    /**
     * Present the native confirmation prompt. The cancel button and window
     * closure map to dismissal.
     * @throws TypeError for invalid options; HostError with the same set as
     * showPicker.
     */
    showConfirmation(options: ConfirmationOptions): Promise<ConfirmationResult>;
    /**
     * Raise a notice: a popup card in the target window, or every window, and
     * an entry in the notification history. Resolves with the notice's ID once
     * it is recorded, without waiting for the user. A `target` that is not a
     * live window rejects InvalidTarget. Notices outlive the package that
     * raised them.
     * @throws TypeError for invalid options; HostError with InvalidTarget or
     * Cancelled.
     */
    showNotice(options: NoticeOptions): Promise<number>;
    /** Mark notices read, which closes their popups. Unknown IDs are ignored. */
    markNotificationsRead(ids: readonly number[]): Promise<void>;
    /** Remove a notice from the history. An unknown ID is ignored. */
    dismissNotification(id: number): Promise<void>;
    /** Empty the notice history. */
    clearNotifications(): Promise<void>;
    /**
     * Add an item to the status bar of every window. The latest
     * registration of an ID defines the item and disposing it reveals the
     * previous one. The `statusBar.hiddenItems` setting hides an ID without
     * stopping its registrant, and teardown removes the item.
     * @throws TypeError for invalid options; HostError with Cancelled.
     */
    registerStatusItem(id: string, options: StatusItemOptions): Promise<StatusItem>;
    /**
     * Read the selection of an admitted text-frame view handler; no selection
     * is null. Omission uses its current capture; explicit target must be
     * authorized for that invocation. Native editor/terminal selection is not
     * exposed here, and a tree view uses `selectedTreeItem`.
     * @throws HostError with ContextUnavailable outside a command, InvalidTarget
     * for invalid captures, UnsupportedOperation for unsupported surfaces,
     * or Cancelled after cancellation.
     */
    selectedText(target?: CommandTarget): Promise<string | null>;
    /**
     * The ID of the selected row of the tree view an admitted view-scoped
     * command handler runs in; no selection is null. Omission uses its current
     * capture; explicit target must be authorized for that invocation.
     * @throws HostError with ContextUnavailable outside a command, InvalidTarget
     * for invalid captures, UnsupportedOperation for a view that is not a tree,
     * or Cancelled after cancellation.
     */
    selectedTreeItem(target?: CommandTarget): Promise<string | null>;
    /**
     * Write application clipboard text without a window/selection requirement.
     * Valid during activation and callbacks. Resolution acknowledges the native
     * clipboard write. TypeError for nontext; HostError on cancellation/teardown.
     */
    writeClipboardText(text: string): Promise<void>;
    /**
     * Read the application clipboard's text, or null when it holds none.
     * Like the write, it needs no window or selection and is valid during
     * activation and callbacks.
     * @throws HostError on cancellation or teardown.
     */
    readClipboardText(): Promise<string | null>;
  }>;
  /**
   * Optional focused view scope; omission/null means application-wide. A
   * `context` additionally requires the focused view to hold that key
   * context (see `Editor.setKeyContext`) and needs a `view`. While an editor
   * bar's field has focus, the view is "bar" and its only context is the
   * bar's ID.
   */
  export interface KeybindingOptions {
    readonly view?: string | null;
    readonly context?: string | null;
  }
  /** Command name and JSON arguments, defaulting to null when omitted. */
  export interface KeybindingCommand { readonly name: string; readonly arguments?: JsonValue; }
  /** One preset binding; a null command unbinds the key. A view scope names
   * a view kind; a binding for a kind that is not declared is skipped. */
  export interface KeymapPresetBinding {
    readonly key: string;
    readonly command: string | KeybindingCommand | null;
    readonly view?: string | null;
  }
  /** A named set of default bindings. Title defaults to the name. */
  export interface KeymapPresetDefinition {
    readonly name: string;
    readonly title?: string;
    readonly bindings: readonly KeymapPresetBinding[];
  }
  /** A preset as `getPreset` returns it, with every field present. */
  export interface KeymapPreset {
    readonly name: string;
    readonly title: string;
    readonly bindings: readonly {
      readonly key: string;
      readonly command: { readonly name: string; readonly arguments: JsonValue } | null;
      readonly view: string | null;
    }[];
  }
  export interface KeymapPresetInfo { readonly name: string; readonly title: string; }
  /** A range of 0-based line numbers, half-open, numbered as `Buffer.lines` does. */
  export type LineRange = Readonly<{ start: number; end: number }>;
  /**
   * One changed region: the `old` lines are replaced by the `new` lines. An
   * empty `old` range is an insertion before `old.start`; an empty `new`
   * range is a deletion.
   */
  export type DiffHunk = Readonly<{ old: LineRange; new: LineRange }>;
  export type DiffResult = Readonly<{
    /** The changed regions, ascending and disjoint; empty when the sides are equal. */
    hunks: readonly DiffHunk[];
    /** The revision of the old side when it is a buffer, otherwise absent. */
    oldRevision?: number;
    /** The revision of the new side when it is a buffer, otherwise absent. */
    newRevision?: number;
  }>;
  export interface DiffOptions {
    /**
     * Treat lines as equal when they match after removing all whitespace (as
     * `git diff -w`), which also ignores `\r\n` against `\n`; lines that
     * differ only in whitespace then fall inside no hunk. Lines compare
     * exactly, terminators included, by default.
     */
    readonly ignoreWhitespace?: boolean;
  }
  /**
   * Compare two texts by line and return the hunks that turn `old` into
   * `updated`. Each side is a string or a live `Buffer`; a buffer is read
   * natively, so its text never crosses the boundary. The comparison runs off
   * the foreground and has no size limit. Changes inside a line are not
   * reported.
   * @throws TypeError for a side that is neither a string nor a buffer or a
   * non-boolean `ignoreWhitespace`; HostError with BufferClosed or Cancelled
   * on teardown.
   */
  export function diff(
    old: string | Buffer,
    updated: string | Buffer,
    options?: DiffOptions,
  ): Promise<DiffResult>;
  /** The operating system Knot runs on, for platform-specific configuration. */
  export const platform: "macos" | "windows" | "linux";
  /** The user's home directory as a normalized `file:` URI, in the same form
   * document and resource URIs use, so plain string comparison applies; null
   * when no home directory is known. */
  export const homeUri: string | null;
  /**
   * Key bindings and keymap presets. The selected preset supplies default
   * bindings beneath every `bind`; the `keymap.preset` setting selects it,
   * resolved when startup completes and whenever the setting changes. An
   * unregistered name selects the native `"knot"` preset.
   */
  export const keymaps: Readonly<{
    /**
     * Bind a gpui key sequence, e.g. "cmd-g", in the caller's lifecycle-owned slot.
     * Null command masks earlier bindings and leaves raw input available.
     * Every binding outranks the preset. Among the rest, the binding whose
     * scope requires the most facts of the focused view wins (view and
     * context beat view, which beats global), then the latest write, so
     * personal config overrides a package's binding from post-init, which
     * runs after packages start. Contexts resolve against the view's state
     * when the key arrives; a key nothing binds in the view's contexts falls
     * through to the remaining bindings and then to typing. Teardown
     * reveals the previous write. Keys and text typed while a key-invoked
     * extension command runs wait for it and then apply in order; the wait
     * ends early when focus leaves the view or after one second.
     * Synchronous void call: mutations commit atomically at the end of the V8 turn.
     * Invalid input fails the whole batch through its turn/startup/command result,
     * with no partial map. No separate promise or asynchronous HostError is returned.
     */
    bind(key: string, command: string | KeybindingCommand | null, options?: KeybindingOptions | null): void;
    /**
     * Remove only the caller's slot for this key/scope, revealing lower owners.
     * Same synchronous atomic turn-batch validation/completion as bind.
     * Lifecycle teardown removes its slots without requiring explicit removal.
     */
    unbind(key: string, options?: KeybindingOptions | null): void;
    /**
     * Route committed text typed in a matching editor to `command` instead
     * of inserting it, with `{ text }` as the command's arguments, where
     * `text` is what the editor would have inserted after keyboard layout
     * and IME. Keys bound in the same scope still win, because text arrives
     * only for keys nothing binds. Null masks a lower text binding so the
     * view types natively. Text bindings resolve by the same scope rule as
     * `bind`; a composition's preedit is not shown where one applies.
     * Same synchronous atomic turn batch as bind; teardown removes it.
     */
    bindText(command: string | null, options?: KeybindingOptions | null): void;
    /** Remove only the caller's text binding for this scope. */
    unbindText(options?: KeybindingOptions | null): void;
    /** Register a lifecycle-owned preset. The latest registration of a name
     * wins and teardown reveals the previous one. A derived preset spreads
     * `getPreset`'s bindings. Registering the selected name takes effect at
     * the next selection change.
     * @throws TypeError for invalid fields or key sequences, or the reserved
     * name "knot"; HostError with Cancelled. */
    registerPreset(definition: KeymapPresetDefinition): Promise<Disposable>;
    /** The preset `name` selects, including the native "knot" preset for
     * the current platform; null when no preset has that name. */
    getPreset(name: string): Promise<KeymapPreset | null>;
    /** "knot" and every registered preset, sorted by name. */
    listPresets(): Promise<readonly KeymapPresetInfo[]>;
  }>;
}

/**
 * Schema-built contracts between packages. A package's contract module
 * imports only this module and its own relative modules. Each schema value is
 * plain, frozen data: the host compiles it and validates every value that
 * crosses a contract against it, so the contract module is the single source
 * of a contract's types and its runtime checks.
 */
declare module "knot/contract" {
  import type { Disposable, JsonValue, WindowTarget } from "knot";

  const inferred: unique symbol;
  /** A schema for values of type `T`. Build it with `s`. */
  export interface Schema<T> {
    readonly kind:
      | "string" | "int" | "number" | "boolean" | "null" | "json"
      | "literal" | "enum" | "array" | "tuple" | "record" | "object" | "union";
    /** Never present at runtime: carries `T` for inference. */
    readonly [inferred]?: T;
  }
  /** An object field that may be absent; only `s.object` takes one. */
  export interface OptionalSchema<T> {
    readonly kind: "optional";
    readonly schema: Schema<T>;
  }
  /** No value: a method without an argument or result. */
  export interface VoidSchema {
    readonly kind: "void";
  }
  /** One contract method: async, at most one argument, one result. */
  export interface FnSchema<A, R> {
    readonly kind: "fn";
    readonly argument: Schema<A> | VoidSchema;
    readonly result: Schema<R> | VoidSchema;
  }

  /** The type of the values `S` accepts. */
  export type Infer<S> = S extends Schema<infer T> ? T : S extends VoidSchema ? void : never;

  type ObjectFields = Readonly<Record<string, Schema<any> | OptionalSchema<any>>>;
  type Flatten<T> = { readonly [K in keyof T]: T[K] } & {};
  type InferObject<F extends ObjectFields> = Flatten<
    { readonly [K in keyof F as F[K] extends OptionalSchema<any> ? never : K]: Infer<F[K]> } &
    { readonly [K in keyof F as F[K] extends OptionalSchema<any> ? K : never]?:
      F[K] extends OptionalSchema<infer T> ? T : never }
  >;
  type Scalar = string | number | boolean | null;

  /**
   * The schema builder. Values are JSON-shaped: `int` accepts safe integers
   * only, objects reject fields they do not declare, `json` accepts any JSON
   * value, and `nullable(x)` is `union([x, null()])`.
   * @throws TypeError for an argument that is not a schema of the expected
   * kind, a literal that is not a JSON scalar, or an empty or repeated enum.
   */
  export const s: Readonly<{
    string(): Schema<string>;
    int(): Schema<number>;
    number(): Schema<number>;
    boolean(): Schema<boolean>;
    null(): Schema<null>;
    json(): Schema<JsonValue>;
    /** Only an `s.fn` argument or result. */
    void(): VoidSchema;
    literal<const T extends Scalar>(value: T): Schema<T>;
    enum<const T extends readonly [string, ...string[]]>(values: T): Schema<T[number]>;
    array<T>(items: Schema<T>): Schema<readonly T[]>;
    tuple<const T extends readonly Schema<any>[]>(items: T): Schema<{ readonly [K in keyof T]: Infer<T[K]> }>;
    record<T>(values: Schema<T>): Schema<Readonly<Record<string, T>>>;
    object<const F extends ObjectFields>(fields: F): Schema<InferObject<F>>;
    optional<T>(schema: Schema<T>): OptionalSchema<T>;
    union<const T extends readonly [Schema<any>, ...Schema<any>[]]>(options: T): Schema<Infer<T[number]>>;
    nullable<T>(schema: Schema<T>): Schema<T | null>;
    fn<A = void, R = void>(argument: Schema<A> | VoidSchema, result: Schema<R> | VoidSchema): FnSchema<A, R>;
  }>;

  export type ContractMethods = Readonly<Record<string, FnSchema<any, any>>>;
  /**
   * A contract: its name, unique within the package whose contract module
   * defines it, and its methods. The host identifies it by that package and
   * name, and validates every call against the schemas of both sides.
   */
  export interface Contract<M extends ContractMethods> {
    readonly name: string;
    readonly methods: M;
    /**
     * Serves the contract from this package. Any package may serve any
     * contract; a client binds to the latest live registration, and disposing
     * it uncovers the previous one.
     * @throws TypeError when `implementation` lacks one of the methods.
     */
    serve(implementation: ContractImplementation<M>): Promise<Disposable>;
    /**
     * Binds to the latest registration serving the contract. The client stays
     * bound to it: once that registration is disposed or its package stops,
     * calls reject with ContractNotFound, and a new client binds to whatever
     * serves the contract then.
     * @throws HostError ContractNotFound when no package serves the contract.
     */
    client(): Promise<ContractClient<M>>;
  }

  /**
   * What a consumer calls: each method runs in the serving package and
   * resolves with its result.
   * @throws HostError ContractViolation when the argument or result does not
   * match the consumer's or the provider's schema, naming the method and the
   * path of the offending value; ProviderFailure when the method throws;
   * ContractNotFound when the registration is gone; UnsupportedOperation when
   * the package calls a registration it serves itself.
   */
  export type ContractClient<M extends ContractMethods> = {
    readonly [K in keyof M]: M[K] extends FnSchema<infer A, infer R>
      ? [A] extends [void] ? () => Promise<R> : (argument: A) => Promise<R>
      : never;
  };
  /** The command a call runs in: its window, or null outside a window. */
  export interface ContractCallContext {
    readonly window: WindowTarget | null;
  }
  /** What a provider serves: each method receives the validated argument. */
  export type ContractImplementation<M extends ContractMethods> = {
    readonly [K in keyof M]: M[K] extends FnSchema<infer A, infer R>
      ? (argument: A, context: ContractCallContext) => R | Promise<R>
      : never;
  };

  /**
   * Defines a contract from `s.fn` method schemas.
   * @throws TypeError for an empty name, no methods, a method that is not
   * an `s.fn` schema, or a name the package already defined.
   */
  export function defineContract<const M extends ContractMethods>(name: string, methods: M): Contract<M>;
}
