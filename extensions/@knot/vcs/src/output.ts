// The Version Control output tool window: a log of what backends ran on the
// user's behalf, with each command line, its output, and its result, kept to
// its last `LIMIT` characters and showing its end.
import { commands, icons, workbench } from "knot";
import type { Disposable, ViewRegistration } from "knot";

export const OUTPUT_VIEW = "vcs.output";
export const SHOW_OUTPUT = "vcs.showOutput";

/** The most text the log keeps. */
export const LIMIT = 256 * 1024;

/** One command a backend ran. */
export type LogEntry = {
  readonly backend: string;
  readonly repository: string;
  readonly command: string;
  readonly output: string;
  readonly failed: boolean;
};

export type Output = Disposable & {
  /** Appends the block of a command. */
  append(entry: LogEntry): Promise<void>;
};

/** The text of the block of `entry`. */
export function block({ backend, repository, command, output, failed }: LogEntry): string {
  const body = output === "" || output.endsWith("\n") ? output : `${output}\n`;
  return `[${backend}] ${repository}\n$ ${command}\n${body}${failed ? "Failed" : "Done"}\n\n`;
}

/** `text` cut to at most `LIMIT` characters, from the start of a line. */
export function trimmed(text: string): string {
  if (text.length <= LIMIT) return text;
  const cut = text.indexOf("\n", text.length - LIMIT);
  return cut === -1 ? text.slice(text.length - LIMIT) : text.slice(cut + 1);
}

export async function createOutput(): Promise<Output> {
  let text = "";
  let writing: Promise<unknown> = Promise.resolve();
  await icons.define("vcs.output", { lucide: "scroll-text" });
  const view: ViewRegistration = await workbench.registerView({
    id: OUTPUT_VIEW,
    title: "Version Control",
    content: { kind: "text" },
    follow: true,
    placement: { location: "bottomRight", visible: false },
  });
  const command = await commands.register(
    SHOW_OUTPUT,
    ({ target }) => view.focus(target.windowTarget),
    { title: "Show Version Control Output", description: "Show what version control ran on your behalf." },
  );
  return {
    append(entry) {
      text = trimmed(text + block(entry));
      const pushed = text;
      const write = writing.then(() => view.setText(pushed));
      writing = write.catch(() => {});
      return write;
    },
    async [Symbol.asyncDispose]() {
      await command[Symbol.asyncDispose]().catch(() => {});
      await view[Symbol.asyncDispose]().catch(() => {});
    },
  };
}
