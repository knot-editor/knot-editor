import { commands, documents, storage, windows, workbench } from "knot";
import type { JsonValue } from "knot";
import { adopt } from "./adopt.js";
import { syncBindings } from "./bindings.js";
import { repositoryRoot, windowWithRoot } from "./inference.js";
import { object } from "./json.js";
import { selectedDocument } from "./layout.js";
import {
  displayName,
  newProjectId,
  projectById,
  projectWithRoots,
  renamed,
  rootsLabel,
  rootsPath,
  suggestedName,
  withProject,
  withoutProject,
} from "./registry.js";
import { describe, report } from "./report.js";
import { capturesOf, sessionKey } from "./session.js";
import { commit, refreshTree, release, reload, type ProjectsState } from "./state.js";
import { switchTo } from "./switch.js";
import { VIEW_ID } from "./tree.js";

function idArgument(value: JsonValue): string | null {
  const record = object(value);
  return record !== null && typeof record.id === "string" ? record.id : null;
}

/** A typed name, or null when it is blank or just the inferred name. */
function explicitName(typed: string, inferred: string | null) {
  const name = typed.trim();
  return name.length === 0 || name === inferred ? null : name;
}

export async function registerCommands(state: ProjectsState) {
  // The tool starts hidden so it never displaces the native explorer selection on
  // fresh windows. A command-invoked show or hide is an explicit user arrangement,
  // so the native tool-layout snapshot remembers it as the fresh-window default.
  await commands.register("projects.show", async () => {
    if (state.view !== null) await state.view.show();
  }, { title: "Show Projects", description: "Show the Projects tool." });

  await commands.register("projects.hide", async () => {
    if (state.view !== null) await state.view.hide();
  }, { title: "Hide Projects", description: "Hide the Projects tool." });

  await commands.register("projects.select", async ({ target }) => {
    await reload(state);
    if (state.registry.projects.length === 0) {
      await report("no projects yet; create one first");
      return;
    }
    const picked = await workbench.showPicker({
      title: "Select project",
      items: state.registry.projects.map((project) => ({
        id: project.id,
        label: displayName(project),
        detail: rootsPath(project),
      })),
    });
    if (picked.kind === "dismissed") return;
    const project = projectById(state.registry, picked.id);
    if (project !== null) await switchTo(state, project, target.windowTarget);
  }, { title: "Switch Project…", description: "Replace this window's session with another project." });

  // Row activation routes through the focused tree instance. A click or Enter
  // switches to the selected row's project; other gestures do nothing.
  await commands.registerForView(VIEW_ID, "projects.select", async ({ target, arguments: args }) => {
    const gesture = object(args)?.gesture;
    if (gesture !== "click" && gesture !== "enter") return;
    await reload(state);
    const id = await workbench.selectedTreeItem();
    const project = id === null ? null : projectById(state.registry, id);
    if (project !== null) await switchTo(state, project, target.windowTarget);
  });

  // A window opened with the project's roots binds to it like any other window
  // with those roots.
  await commands.register("projects.new-window", async ({ target }) => {
    await reload(state);
    await syncBindings(state, await windows.list());
    const id = state.bindings.get(target.windowTarget);
    const project = id === undefined ? null : projectById(state.registry, id);
    if (project === null) {
      await report("this window is not part of a project; select a project first");
      return;
    }
    try {
      await windows.open({ roots: project.roots });
    } catch (error) {
      await report(`cannot open a window for ${displayName(project)}: ${describe(error)}`);
    }
  }, { title: "Open Project in New Window", description: "Open another window on this window's project." });

  // The repository of the active document, as automatic adoption would infer
  // it, for windows it never adopts automatically: a rooted one keeps its
  // project and the repository opens beside it.
  await commands.register("projects.open-containing-repository", async ({ target }) => {
    await reload(state);
    const list = await windows.list();
    const window = list.find((window) => window.target === target.windowTarget);
    const id = window === undefined ? null : selectedDocument(window);
    const document = id === null ? null : await documents.get(id);
    if (window === undefined || document === null) {
      await report("there is no document to find a repository for");
      return;
    }
    const root = await repositoryRoot(document.resourceUri);
    if (root === null) {
      await report(`${document.title} is not inside a Git repository`);
      return;
    }
    try {
      const open = windowWithRoot(root, list);
      if (open !== undefined) {
        await documents.present(document.id, { target: open.target });
        await windows.focus(open.target);
      } else {
        await adopt(state, root, window, document.id);
      }
    } catch (error) {
      await report(`cannot open ${root}: ${describe(error)}`);
    }
  }, {
    title: "Open Containing Repository as Project",
    description: "Open the Git repository that contains this document as a project.",
  });

  await commands.register("projects.rename", async ({ arguments: args }) => {
    await reload(state);
    const id = idArgument(args);
    const project = id === null ? null : projectById(state.registry, id);
    if (project === null) {
      await report("project not found");
      return;
    }
    const named = await workbench.showInput({ title: "Rename project", value: displayName(project) });
    if (named.kind === "dismissed") return;
    const name = explicitName(named.value, rootsLabel(project));
    try {
      await commit(state, renamed(state.registry, project.id, name));
    } catch (error) {
      await report(`cannot rename project: ${describe(error)}`);
    }
  }, { title: "Rename Project…", palette: false });

  await commands.register("projects.remove", async ({ arguments: args }) => {
    await reload(state);
    const id = idArgument(args);
    const project = id === null ? null : projectById(state.registry, id);
    if (project === null) {
      await report("project not found");
      return;
    }
    const session = state.sessions.get(project.id) ?? null;
    const captures = capturesOf(session);
    const discarded =
      session?.state === "suspended" && captures.size > 0
        ? ` ${captures.size} unsaved ${captures.size === 1 ? "document" : "documents"} in its suspended session will be discarded.`
        : "";
    const confirmed = await workbench.showConfirmation({
      title: `Remove ${displayName(project)}?`,
      message: `Open documents and folders stay open.${discarded}`,
      confirmLabel: "Remove",
      cancelLabel: "Cancel",
    });
    if (confirmed.kind !== "confirmed") return;
    try {
      await commit(state, withoutProject(state.registry, project.id));
      await storage.delete(sessionKey(project.id));
    } catch (error) {
      await report(`cannot remove project: ${describe(error)}`);
      return;
    }
    // Captures are released only after both deletes are durable.
    state.sessions.delete(project.id);
    await release(captures);
  }, { title: "Remove Project…", palette: false });

  await commands.register("projects.list", async () => {
    await reload(state);
    await refreshTree(state);
    // The tree is the list; showing it is the visible result.
    if (state.view !== null) await state.view.show();
    await workbench.writeClipboardText(
      JSON.stringify({
        schema: state.registry.schema,
        recentProjectId: state.registry.recentProjectId,
        failure: state.loadFailure,
        projects: state.registry.projects,
        sessions: Object.fromEntries(state.sessions),
      }),
    );
  }, { title: "Developer: Copy Projects Record", description: "Copy the stored projects and sessions as JSON." });

  await commands.register("projects.refresh", async () => {
    await reload(state);
    await refreshTree(state);
    if (state.loadFailure !== null) await report(`cannot read projects: ${state.loadFailure}`);
  }, { title: "Refresh Projects", description: "Reread the stored projects." });
}
