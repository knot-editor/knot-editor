# D216 — Git operations

Status: 🚧 planned. Source: [roadmap D216](../roadmap.md), milestone 5 of the
[version control plan](vcs.md). Background:
[version control](../architecture/version-control.md),
[packages](../extension-api/packages.md#contracts-knot-defines),
[prompts](../extension-api/ui.md#ask-the-user-transiently).

## Outcome

- The change gutter shows which hunks are already staged, and the hunk popup
  stages or unstages one hunk, including unsaved edits as they show.
- Clicking the branch in the status bar opens a Git picker: Pull, Push, Fetch,
  New Branch…, Stash…, then local and remote branches to check out.
- Branches can be created, checked out, renamed, and deleted; the repository
  can be fetched, pulled, and pushed, with ahead/behind counts on the branch;
  changes can be stashed and stashes applied, popped, or dropped.
- Every operation is also a `Git: …` palette command, and every `git` call it
  makes shows in the Version Control output.

## Design

### Shared through the contract

- **Staged text.** `baseline` answers `{ text, staged? }`. `staged` is the text
  between the baseline and the working file (Git: the index entry, read with
  `git cat-file --filters :<path>`), omitted when it equals `text` or the
  backend has none. `@knot/vcs` caches it with the baseline and fetches both
  together, so the existing index watch keeps it fresh.
- **Staged marks.** A hunk of the baseline-against-buffer diff is *staged* when
  the staged-against-buffer diff has no hunk overlapping its new lines, that is
  the buffer equals the staged text there. A partly staged hunk counts as
  unstaged. Staged hunks paint with `diff.added.staged`,
  `diff.modified.staged`, and `diff.removed.staged`, dimmer color keys
  `@knot/vcs` defines; the gutter styles stay native `bar` and `deletion`.
- **Hunk actions.** A backend method `hunkActions(repository)` returns
  `{ label, icon, command, on: ("staged" | "unstaged")[] }[]`, appended to the
  popup's toolbar for a hunk in the matching state. `command` is one of the
  backend's commands, run in the editor with `{ repository, file, line }`, the
  hunk's first current line (the boundary line for a removal), like the
  popup's own buttons.
- **Ref command.** `ref` answers may carry `command: { name, arguments? }`,
  which the `vcs.ref` status item runs on click.
- **Busy state.** `vcs.busy({ backend, repository, label: string | null })`
  shows a label such as "Pushing…" in the ref item's tooltip, with a busy
  icon, while the backend runs a long operation; null clears it.

### Git only

- **Stage and Unstage Hunk** (`git.stageHunk`, `git.unstageHunk`) read HEAD,
  the index entry, and the buffer, find the hunk at `line` with the extension
  API's `diff`, and compute the new index text: staging replaces the index's
  lines that overlap the hunk with the buffer's lines; unstaging replaces them
  with HEAD's. The text is written with `git hash-object -w --stdin --path`
  (clean filters apply) and `git update-index --cacheinfo`, keeping the
  entry's mode, or HEAD's when the index lacks the file.
- **Git picker** (`git.menu`, the ref's command): actions first, then local
  branches (current marked), then remote branches. Choosing a branch runs
  `git switch <name>` (a remote one with `--track` unless a local branch of
  that name exists, which is switched to instead).
- **Branches:** New Branch (`git switch -c`, from HEAD, name by input with
  validation through `git check-ref-format --branch`), Rename Branch
  (`git branch -m`, current branch by default), Delete Branch (picker over
  local branches except the current; `git branch -d`, and on "not fully
  merged" a confirmation to retry with `-D`).
- **Remotes:** Fetch (`git fetch --all --prune`), Pull (`git pull`, honoring
  the user's `pull.*` config; no upstream is an error notice), Push
  (`git push`; with no upstream, `git push -u <remote> HEAD`, picking the
  remote when there are several). All run with `GIT_TERMINAL_PROMPT=0`, report
  busy state, and end with a notice (success or the failure, with Show
  Output). The ref tooltip and label gain ahead/behind counts (`↑2 ↓1`, only
  when non-zero) from `git rev-list --left-right --count @{u}...HEAD`.
- **Stash:** Stash and Stash Including Untracked (`git stash push [-u] -m`,
  message by input, empty allowed), and Apply, Pop, and Drop Stash over a
  picker of `git stash list` (Drop confirms). A conflicting pop keeps the
  stash, as Git does, and the conflicts show in status.

Every command acts on the repository of the focused editor's file, else the
window's first root, as the ref item does, and logs through `gitLogged`.
Failures show Git's message in an error notice with Show Output; Git's own
refusals (a dirty tree blocking a checkout, a rejected push) are not
pre-checked.

## Exclusions

Merge and rebase (they lead into conflicts, D219), tags, cherry-pick,
staging from the diff view or by selection, an askpass bridge so Knot can ask
for credentials, real progress reporting (D200), and push or pull buttons in
the Changes view.

## Deferred decisions

- Whether the busy state should later move to D200's progress reporting.
- Whether "Commit and Push" belongs in the commit panel.

## Checkpoints

### 1. Staged hunks in the gutter ✅

Done: `baseline` answers `{ text, staged? }`, and staged hunks paint with the
dimmer `diff.*.staged` colors ([version control](../architecture/version-control.md#gutter),
[Git baseline](../decisions/bundled-packages.md#git-baseline)). A Changes-view
diff whose file leaves its group now moves to the group that lists it. Showing
staged hunks in the diff view's Working Copy sides is left to checkpoint 2,
which needs per-hunk staged state there anyway.

### 2. Hunk actions and Stage/Unstage Hunk

Deliverables: `hunkActions` in the contract and the popup toolbar, Git's
`git.stageHunk` and `git.unstageHunk` with the index computation, also
runnable from the palette on the hunk at the caret, and staged hunks drawn
dimmer in the diff view of Compare with Last Commit (`open` takes `staged`,
`setText` replaces it, and `@knot/diff` now defines the staged colors).

Validation: unit tests for the index computation (pure function over HEAD,
index, and buffer texts and a line), including a partly staged hunk, a
removal, a missing final newline, and CRLF through filters; product tests in
`popup.rs` over a temporary repository that stage and unstage a hunk of a
dirty buffer and check `git diff --cached`.

Review gate: staging from the popup in the real app.

**Implementation context:** the toolbar in `openPopup` and `hunkOf` in
`extensions/@knot/vcs/src/extension.ts`; hunk lookup in `src/hunk.ts`
(`hunkAt`, reusable in Git as a copy, since a bundled package compiles alone);
Git commands registered outside the palette as in
`extensions/@knot/git/src/actions.ts`, plus palette titles; `git()` and
`gitLogged` in `src/git.ts` (`git()` takes stdin for `hash-object`). Record
the staged-hunk choice in `docs/decisions/bundled-packages.md` (Git baseline
or a new "Git staging" section) and update the hunk popup and Git sections of
the architecture reference.

### 3. Ref command and branches

Deliverables: `command` on the ref answer and the status item click, the Git
picker with branches and checkout, New, Rename, and Delete Branch.

Validation: `ref.test.ts` for the command passing through; product tests in
`ref.rs` (or a new `branches.rs`) driving the picker and inputs over a
temporary repository with a local bare remote.

Review gate: the picker from the status bar.

**Implementation context:** `extensions/@knot/vcs/src/ref.ts` (the item's
value), `extensions/@knot/git/src/ref.ts`; a new `src/branches.ts` and
`src/menu.ts` in `@knot/git`; picker and input calls as in `commit.ts`'s
history picker in `@knot/vcs`; prompt-answering helpers in
`crates/knot-ext-vcs/tests/vcs/commit.rs`.

### 4. Fetch and ahead/behind

Deliverables: `vcs.busy` and its display on the ref item, Fetch with its
completion notice, ahead/behind counts on the ref, and the Fetch picker entry.

Validation: `ref.test.ts` for the busy state; product tests over a temporary
repository with a local bare remote: fetch picking up a remote commit updates
the counts, and a failing fetch raises an error notice.

Review gate: fetching a real remote and seeing the counts and busy state.

**Implementation context:** `vcs.busy` beside `log` in `contract.ts` and the
`vcs.d.ts` restatement; the ref item in `@knot/vcs/src/ref.ts`; Git's ref in
`extensions/@knot/git/src/ref.ts`; a new `src/remotes.ts` in `@knot/git`;
`GIT_TERMINAL_PROMPT=0` in `git()`'s environment; a bare-remote helper beside
the temporary-repository helpers in `crates/knot-ext-vcs/tests/vcs/`.

### 5. Pull and Push

Deliverables: Pull, Push with upstream setup (picking the remote when there
are several), their failure notices, and their picker entries.

Validation: product tests over the bare remote: push with and without an
upstream, push with two remotes through the picker, pull of a remote commit,
pull without an upstream, and a rejected push raising an error notice.

Review gate: push and pull against a real remote, including one that needs
credentials, to see the failure notice.

**Implementation context:** `src/remotes.ts` and the busy state from
checkpoint 4; the remote picker as in checkpoint 3.

### 6. Stash

Deliverables: Stash, Stash Including Untracked, Apply, Pop, Drop, and the
Stash… picker entry.

Validation: product tests for push, apply, pop, drop, and a conflicting pop.

Review gate: the stash flow in the real app. Then move durable behavior into
the architecture reference and decisions, mark D216 done in `vcs.md`, and
delete this plan.

**Implementation context:** a new `src/stash.ts` in `@knot/git`, picker as in
checkpoint 3.
