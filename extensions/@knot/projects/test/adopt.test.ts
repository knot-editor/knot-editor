import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { documents } from "knot";
import type { DocumentId } from "knot";
import * as host from "knot/testing";
import { start } from "../src/extension.js";
import { ALPHA, registry, seedProject, seedSession, session, startLaunched } from "./support.js";

/** ALPHA as inference names it. */
const REPO = "file:///src/alpha";

beforeEach(() => {
  host.reset();
  host.seedFile(`${ALPHA}.git/HEAD`, "ref: refs/heads/main");
});

async function opened(id: DocumentId) {
  const document = await documents.get(id);
  assert.ok(document !== null);
  await host.emit({ type: "documentOpened", document });
}

test("a file opened alone in a rootless window adopts its new repository in place", async () => {
  const file = host.addDocument({ uri: `${ALPHA}src/main.rs` });
  const window = host.openWindow({ documents: [file] });
  await startLaunched();
  await opened(file);

  assert.deepEqual(host.windowTargets(), [window]);
  assert.deepEqual(host.rootsOf(window), [REPO]);
  assert.deepEqual(host.documentsIn(window), [file]);
  const workspace = host.setRoots(window, [REPO]);
  await host.emit({ type: "workspaceChanged", workspace });
  assert.deepEqual(registry().projects.map((project) => [project.name, project.roots]), [[null, [REPO]]]);
});

test("a known project restores its session with the file and releases the stray window", async () => {
  seedProject("a", "Alpha", [ALPHA]);
  seedSession("a", "suspended", [{ entries: [{ uri: `${ALPHA}notes.md` }], selected: 0 }]);
  const file = host.addDocument({ uri: `${ALPHA}src/main.rs`, dirty: true, text: "edited" });
  const stray = host.openWindow({ documents: [file] });
  await startLaunched();
  await opened(file);

  const [window, ...rest] = host.windowTargets();
  assert.deepEqual(rest, []);
  assert.notEqual(window, stray);
  assert.deepEqual(host.rootsOf(window!), [ALPHA]);
  const [notes, carried] = host.documentsIn(window!);
  assert.equal(host.document(notes!).resourceUri, `${ALPHA}notes.md`);
  assert.equal(carried, file);
  assert.equal(host.document(file).text, "edited");
  assert.equal(session("a")?.state, "active");
  assert.deepEqual(host.captureIds(), []);
});

test("documents opened before launch, in rooted windows, or beside other tabs stay put", async () => {
  const early = host.addDocument({ uri: `${ALPHA}early.rs` });
  const restored = host.openWindow({ documents: [early] });
  await start();
  await opened(early);
  await host.launch();

  const inRooted = host.addDocument({ uri: `${ALPHA}rooted.rs` });
  const rooted = host.openWindow({ roots: ["file:///src/beta/"], documents: [inRooted] });
  await opened(inRooted);

  const beside = host.addDocument({ uri: `${ALPHA}beside.rs` });
  const crowded = host.openWindow({ documents: [host.addDocument({ uri: "file:///loose.txt" }), beside] });
  await opened(beside);

  assert.deepEqual(host.rootsOf(restored), []);
  assert.deepEqual(host.rootsOf(rooted), ["file:///src/beta/"]);
  assert.deepEqual(host.rootsOf(crowded), []);
});

test("saving an untitled document into a repository adopts it", async () => {
  const draft = host.addDocument({ text: "notes" });
  const window = host.openWindow({ documents: [draft] });
  await startLaunched();
  await opened(draft);
  assert.deepEqual(host.rootsOf(window), []);

  Object.assign(host.document(draft), { kind: "persisted", resourceUri: `${ALPHA}notes.md` });
  const saved = await documents.get(draft);
  await host.emit({ type: "documentChanged", document: saved! });
  assert.deepEqual(host.rootsOf(window), [REPO]);
});

test("a failed adoption leaves the file where it opened", async () => {
  const file = host.addDocument({ uri: `${ALPHA}src/main.rs` });
  const window = host.openWindow({ documents: [file] });
  await startLaunched();
  host.failNext("workspaces.replaceRoots", "ProviderFailure");
  await opened(file);
  assert.deepEqual(host.rootsOf(window), []);
  assert.deepEqual(host.messages(), []);
});

const OPEN_REPOSITORY = "projects.open-containing-repository";
const BETA = "file:///src/beta/";

test("the command opens a new repository beside a rooted window", async () => {
  const file = host.addDocument({ uri: `${ALPHA}src/main.rs` });
  const source = host.openWindow({ roots: [BETA], documents: [file] });
  await startLaunched();
  await host.run(OPEN_REPOSITORY, { window: source });

  const [, window] = host.windowTargets();
  assert.deepEqual(host.rootsOf(window!), [REPO]);
  assert.deepEqual(host.documentsIn(window!), [file]);
  assert.deepEqual(host.rootsOf(source), [BETA]);
  assert.deepEqual(host.documentsIn(source), [file]);
});

test("the command restores a known project beside a rooted window and keeps its project", async () => {
  seedProject("a", "Alpha", [ALPHA]);
  seedProject("b", "Beta", [BETA]);
  seedSession("a", "suspended", [{ entries: [{ uri: `${ALPHA}notes.md` }], selected: 0 }]);
  seedSession("b", "active");
  const file = host.addDocument({ uri: `${ALPHA}src/main.rs` });
  const source = host.openWindow({ roots: [BETA], documents: [file] });
  await startLaunched();
  await host.run(OPEN_REPOSITORY, { window: source });

  const [kept, window, ...rest] = host.windowTargets();
  assert.equal(kept, source);
  assert.deepEqual(rest, []);
  assert.deepEqual(host.rootsOf(source), [BETA]);
  assert.deepEqual(host.documentsIn(source), [file]);
  assert.equal(session("b")?.state, "active");
  assert.deepEqual(host.rootsOf(window!), [ALPHA]);
  assert.equal(host.documentsIn(window!).at(-1), file);
  assert.equal(host.focusedWindow(), window);
  assert.equal(session("a")?.state, "active");
});

test("the command presents the document in a window that already has its repository", async () => {
  const file = host.addDocument({ uri: `${ALPHA}src/main.rs` });
  const source = host.openWindow({ roots: [BETA], documents: [file] });
  const existing = host.openWindow({ roots: [ALPHA] });
  await startLaunched();
  await host.run(OPEN_REPOSITORY, { window: source });

  assert.deepEqual(host.windowTargets(), [source, existing]);
  assert.deepEqual(host.documentsIn(existing), [file]);
  assert.equal(host.focusedWindow(), existing);
});

test("the command says when no repository contains the document", async () => {
  const file = host.addDocument({ uri: "file:///loose/notes.txt" });
  const window = host.openWindow({ documents: [file] });
  await startLaunched();
  await host.run(OPEN_REPOSITORY, { window });
  assert.deepEqual(host.messages(), ["notes.txt is not inside a Git repository"]);
  assert.deepEqual(host.rootsOf(window), []);
});
