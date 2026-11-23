// What is checked out: the branch name, or the short commit ID when HEAD is
// detached. A branch with no commit yet still has a name.
import { git } from "./git.js";

export type Ref = { readonly label: string; readonly tooltip?: string };

/** The ref of `repository`, or null when it has none to show. */
export async function ref(repository: string): Promise<Ref | null> {
  const branch = await git(repository, ["symbolic-ref", "--short", "-q", "HEAD"]);
  const name = branch?.exitCode === 0 ? branch.stdout.trim() : "";
  if (name !== "") return { label: name, tooltip: `Branch ${name}` };
  const commit = await git(repository, ["rev-parse", "--short", "HEAD"]);
  const id = commit?.exitCode === 0 ? commit.stdout.trim() : "";
  return id === "" ? null : { label: id, tooltip: `Detached at ${id}` };
}
