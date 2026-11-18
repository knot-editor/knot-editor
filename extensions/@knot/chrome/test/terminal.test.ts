import assert from "node:assert/strict";
import { test } from "node:test";
import { chooseRoot } from "../src/terminal.js";

const roots = ["file:///other/", "file:///repo/", "file:///repo/crates/app/"];

test("the innermost root containing the document wins", () => {
  assert.equal(chooseRoot(roots, "file:///repo/crates/app/src/main.rs"), "file:///repo/crates/app/");
  assert.equal(chooseRoot(roots, "file:///repo/README.md"), "file:///repo/");
});

test("a document outside every root, or none, falls back to the first root", () => {
  assert.equal(chooseRoot(roots, "file:///notes.md"), "file:///other/");
  assert.equal(chooseRoot(roots, null), "file:///other/");
  assert.equal(chooseRoot([], null), null);
});

test("a root is not a prefix of its sibling with the same name start", () => {
  assert.equal(chooseRoot(["file:///repo"], "file:///repository/a.rs"), "file:///repo");
  assert.equal(chooseRoot(["file:///a/", "file:///repo"], "file:///repository/a.rs"), "file:///a/");
});
