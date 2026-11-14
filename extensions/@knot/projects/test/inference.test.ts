import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { windows } from "knot";
import * as host from "knot/testing";
import { automaticRoot, repositoryRoot } from "../src/inference.js";

beforeEach(() => host.reset());

const DOC = "file:///repo/src/main.rs";

test("nearest ancestor wins and a .git file counts", async () => {
  host.seedFile("file:///repo/.git/HEAD", "ref: refs/heads/main");
  host.seedFile("file:///repo/sub/.git", "gitdir: ../.git/worktrees/sub");
  assert.equal(await repositoryRoot("file:///repo/sub/src/main.rs"), "file:///repo/sub/");
});

test("a .git directory counts", async () => {
  host.seedFile("file:///repo/.git/HEAD", "ref: refs/heads/main");
  assert.equal(await repositoryRoot(DOC), "file:///repo/");
});

test("no marker resolves to null", async () => {
  host.seedFile("file:///repo/src/main.rs", "fn main() {}");
  assert.equal(await repositoryRoot(DOC), null);
});

test("a repository at the filesystem root never counts", async () => {
  host.seedFile("file:///.git", "gitdir: /elsewhere");
  assert.equal(await repositoryRoot("file:///main.rs"), null);
  assert.equal(await repositoryRoot(DOC), null);
});

test("a repository at the home directory never counts", async () => {
  host.seedFile("file:///Users/me/.git/HEAD", "ref: refs/heads/main");
  const uri = "file:///Users/me/dotfiles/todo.txt";
  assert.equal(await repositoryRoot(uri), "file:///Users/me/");
  host.setHomeUri("file:///Users/me/");
  assert.equal(await repositoryRoot(uri), null);
});

test("unstored and non-file documents resolve to null", async () => {
  host.seedFile("file:///repo/.git/HEAD", "ref: refs/heads/main");
  assert.equal(await repositoryRoot(null), null);
  assert.equal(await repositoryRoot("https://example.com/repo/src/main.rs"), null);
});

test("a stat failure resolves to null", async () => {
  host.seedFile("file:///repo/.git/HEAD", "ref: refs/heads/main");
  host.failNext("files.stat", "StorageUnavailable");
  assert.equal(await repositoryRoot(DOC), null);
});

async function automatic(document: ReturnType<typeof host.addDocument>, window: ReturnType<typeof host.openWindow>) {
  const list = await windows.list();
  const info = list.find((candidate) => candidate.target === window)!;
  return automaticRoot({ id: document, resourceUri: host.document(document).resourceUri }, info, list);
}

test("a rootless window presenting only the document adopts automatically", async () => {
  host.seedFile("file:///repo/.git/HEAD", "ref: refs/heads/main");
  const document = host.addDocument({ uri: DOC });
  const window = host.openWindow({ documents: [document] });
  assert.equal(await automatic(document, window), "file:///repo/");
});

test("rooted windows, other tabs, and already open roots never adopt automatically", async () => {
  host.seedFile("file:///repo/.git/HEAD", "ref: refs/heads/main");

  const inRooted = host.addDocument({ uri: DOC });
  const rooted = host.openWindow({ roots: ["file:///other/"], documents: [inRooted] });
  assert.equal(await automatic(inRooted, rooted), null);
  host.closeWindow(rooted);

  const inCrowded = host.addDocument({ uri: DOC });
  const crowded = host.openWindow({ documents: [host.addDocument({ uri: "file:///loose.txt" }), inCrowded] });
  assert.equal(await automatic(inCrowded, crowded), null);
  host.closeWindow(crowded);

  const alone = host.addDocument({ uri: DOC });
  const window = host.openWindow({ documents: [alone] });
  host.openWindow({ roots: ["file:///repo"] });
  assert.equal(await automatic(alone, window), null);
});
