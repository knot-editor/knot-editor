// The Terminal tool window and its commands: New Terminal and New Terminal in
// Editor. New Terminal owns where a shell starts.
import { commands, files, terminals, windows, workbench } from "knot";
import type { CommandContext, WindowInfo } from "knot";

export const TERMINAL_VIEW = "builtin.terminal";
export const NEW_TERMINAL = "terminal.new";
export const NEW_TERMINAL_IN_EDITOR = "terminal.new-in-center";

const withSlash = (uri: string) => (uri.endsWith("/") ? uri : `${uri}/`);

/** The root a new terminal starts in: the innermost containing `document`,
 * else the first. Roots are existing local directories. */
export function chooseRoot(roots: readonly string[], document: string | null): string | null {
  const contains = (root: string) => document !== null && document.startsWith(withSlash(root));
  const inner = roots.filter(contains).sort((a, b) => b.length - a.length)[0];
  return inner ?? roots[0] ?? null;
}

/** The directory a new terminal starts in: the focused terminal's, else the
 * root containing the last focused document, else the first root, else
 * undefined, which starts the shell in the home directory. */
async function startDirectory(window: WindowInfo): Promise<string | undefined> {
  const focused = window.panes
    .flatMap((pane) => pane.tabs)
    .find((tab) => tab.target === window.currentTarget);
  if (focused?.kind === "terminal" && focused.directory !== null) return focused.directory;
  const roots: string[] = [];
  for (const { uri } of window.workspace.roots) {
    if (!uri.startsWith("file:")) continue;
    if ((await files.stat(uri))?.kind === "directory") roots.push(uri);
  }
  const document = await workbench.lastFocusedDocument(window.target);
  return chooseRoot(roots, document?.resourceUri ?? null) ?? undefined;
}

async function open(context: CommandContext, location: string) {
  const window = await windows.get(context.target.windowTarget);
  if (window === null) commands.unavailable();
  const cwd = await startDirectory(window);
  await terminals.open(cwd === undefined ? { location } : { location, cwd });
}

export async function registerTerminal() {
  await workbench.registerView({
    id: TERMINAL_VIEW,
    title: "Terminal",
    newTab: NEW_TERMINAL,
    placement: { location: "bottomLeft", visible: false },
  });
  await commands.register(NEW_TERMINAL, (context) => open(context, TERMINAL_VIEW), {
    title: "New Terminal",
    description: "Open a terminal beside the focused one in the Terminal window.",
  });
  await commands.register(NEW_TERMINAL_IN_EDITOR, (context) => open(context, "editor"), {
    title: "New Terminal in Editor",
    description: "Open a terminal in the last focused editor pane.",
  });
}
