import assert from "node:assert/strict";
import { test } from "node:test";
import type { JsonValue } from "knot";
import {
  displayName,
  emptyRegistry,
  projectWithRoots,
  registryFromJson,
  renamed,
  rootsLabel,
  rootsPath,
  sameRoots,
  selected,
  suggestedName,
  withProject,
  withoutProject,
  type Project,
} from "../src/registry.js";

function project(id: string, name: string | null, roots: string[]): Project {
  return { id, name, roots, createdAt: null, lastOpenedAt: null };
}

const alpha = project("a", "Alpha", ["file:///alpha/"]);
const beta = project("b", " Beta ", ["file:///beta/", "file:///shared/"]);
const registry = { ...emptyRegistry(), projects: [alpha, beta], recentProjectId: "b" };

test("parses a stored registry and defaults optional fields", () => {
  const parsed = registryFromJson({
    schema: 2,
    projects: [{ id: "a", name: "Alpha", roots: ["file:///alpha/", 3], createdAt: "x" }],
    recentProjectId: 7,
  });
  assert.deepEqual(parsed, {
    schema: 2,
    projects: [{ id: "a", name: "Alpha", roots: ["file:///alpha/"], createdAt: null, lastOpenedAt: null }],
    recentProjectId: null,
  });
});

test("rejects unsupported schemas, shapes, and entries", () => {
  assert.throws(() => registryFromJson([]), /not an object/);
  assert.throws(() => registryFromJson({ schema: 1, projects: [] }), /unsupported projects record schema: 1/);
  assert.throws(() => registryFromJson({ schema: 2, projects: [{ id: "", name: "x", roots: [] }] }), /invalid entry/);
});

test("a stored name may be null, but not empty", () => {
  const stored = (name: JsonValue) => ({ schema: 2, projects: [{ id: "a", name, roots: [] }] });
  assert.equal(registryFromJson(stored(null)).projects[0]?.name, null);
  assert.throws(() => registryFromJson(stored("")), /invalid entry/);
  assert.throws(() => registryFromJson({ schema: 2, projects: [{ id: "a", roots: [] }] }), /invalid entry/);
});

test("display names fall back to the roots, and names need not be unique", () => {
  assert.equal(displayName(alpha), "Alpha");
  assert.equal(displayName(project("u", null, ["file:///src/app/"])), "app");
  assert.equal(displayName(project("m", null, ["file:///src/app/", "file:///src/lib/"])), "app +1");
  assert.equal(displayName(project("e", null, [])), "Empty project");
  const twins = withProject(registry, project("t", "Alpha", ["file:///other/"]));
  assert.equal(twins.projects.filter((candidate) => candidate.name === "Alpha").length, 2);
});

test("root paths tell apart folders with one name", () => {
  assert.equal(rootsPath(project("u", null, ["file:///src/my%20app/", "file:///lib/"])), "/src/my app, /lib");
});

test("a home directory shows as a tilde", () => {
  assert.equal(rootsPath(project("u", null, ["file:///Users/me/src/app/", "file:///home/me/"])), "~/src/app, ~");
});

test("renaming sets an explicit name and null restores the inferred one", () => {
  const named = renamed(registry, "a", "Mine");
  assert.equal(displayName(named.projects[0]!), "Mine");
  assert.equal(displayName(renamed(named, "a", null).projects[0]!), "alpha");
});

test("root sets match regardless of order, and an empty set matches nothing", () => {
  assert.equal(sameRoots(["x", "y"], ["y", "x"]), true);
  assert.equal(sameRoots(["x"], ["x", "y"]), false);
  assert.equal(projectWithRoots(registry, ["file:///shared/", "file:///beta/"]), beta);
  assert.equal(projectWithRoots(registry, ["file:///beta/"]), null);
  assert.equal(projectWithRoots({ ...registry, projects: [project("e", "Empty", [])] }, []), null);
});

test("suggested names decode the last path segment", () => {
  assert.equal(suggestedName("file:///src/my%20app/"), "my app");
  assert.equal(suggestedName("file:///src/bad%E0/"), "bad%E0");
  assert.equal(suggestedName("file:///"), "file:");
});

test("root labels name the first root and count the rest", () => {
  assert.equal(rootsLabel(alpha), "alpha");
  assert.equal(rootsLabel(beta), "beta +1");
  assert.equal(rootsLabel(project("e", "Empty", [])), null);
});

test("record updates keep the recent selection consistent", () => {
  const gamma = project("g", "Gamma", ["file:///gamma/"]);
  assert.equal(withProject(registry, gamma).recentProjectId, "g");
  assert.equal(withoutProject(registry, "b").recentProjectId, null);
  assert.equal(withoutProject(registry, "a").recentProjectId, "b");
  const next = selected(registry, "a", 42);
  assert.equal(next.recentProjectId, "a");
  assert.equal(next.projects[0]?.lastOpenedAt, 42);
  assert.equal(next.projects[1]?.lastOpenedAt, null);
});
