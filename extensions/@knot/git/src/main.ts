import { workbench } from "knot";
import { backendContract, vcs } from "@knot/vcs";
import { baseline } from "./baseline.js";
import { git, isMissingProgram } from "./git.js";
import { repositories, repositoryOf } from "./repositories.js";

/** Whether Git runs; without it there is nothing to serve, and the user is told once. */
async function available(): Promise<boolean> {
  try {
    await git("file:///", ["--version"]);
    return true;
  } catch (error) {
    if (!isMissingProgram(error)) throw error;
    await workbench.showNotice({
      title: "Git",
      message: "Git was not found on the PATH, so the change gutter has no Git baselines.",
      severity: "warning",
    });
    return false;
  }
}

if (await available()) {
  await backendContract("git").serve({
    repositories: (root) => repositories(root).catch(() => []),
    repositoryOf: (directory) => repositoryOf(directory).catch(() => null),
    baseline: ({ repository, file }) => baseline(repository, file).catch(() => null),
  });
  await (await vcs.client()).register("git");
}
