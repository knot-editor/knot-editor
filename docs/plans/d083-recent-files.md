# D083 — Recent files

Status: 🚧 checkpoint 1 implemented ✅, awaiting its review gate. Item of the
[daily-driver plan](daily-driver.md#phase-4--files-and-workspace). Reopen closed
tab (D072) is unrelated: it is native per-window state.

## Outcome

`Open Recent…` (⌘E, Ctrl+E elsewhere, as in IntelliJ) lists every recently
active file, most recent first, and opens the chosen one. Unlike Go to File's
empty-query list, it is not limited to the window's roots and works in a window
without roots. `Clear Recent Files` empties the list.

## Design

- **Home.** `@knot/go-to-file` already records a file when an editor tab
  becomes active, whatever opened it (`recordActive` in `src/recent.ts`, the
  `recent-files` history, at most 50). Open Recent joins that package; no
  second package and no second recording.
- **Recent means active.** Files recorded as above; no `documents.open` hook.
- **Rows.** Picker `items` matched locally by the picker: label is the file
  name; description is the path below the file's root when a root of the
  window contains it (prefixed by the root name with several roots, as Go to
  File does), otherwise the decoded parent path of the URI. The current
  window's own last focused file is not excluded: it is a plain history view.
- **Missing files.** When the picker opens, each recorded file (at most 50)
  is checked for existence. Files under the window's roots are first looked
  up with `files.list` on their parent directory, once per distinct directory
  and answered by the workspace index; a file the listing names exists. The
  index can only confirm: its listings apply the hidden and ignore filters,
  so absence proves nothing. Every file not confirmed, including those
  outside the roots, goes to `files.stat` in parallel, and those that resolve
  to `null` are left out and removed from the history. A `ProviderFailure`
  from either keeps the entry. If a file vanishes between the check and the
  choice, opening it fails normally.
- **History primitive.** `history` has `recent`, `record`, and `order` but no
  way to drop entries (it is a `history/<name>` record in `storage`, in
  `bootstrap.js` ~line 1743). Add `history.forget(name, id?)`: with an ID it
  removes that entry (used to drop missing files), without it the whole
  list (used by Clear Recent Files). Declare it in `knot.d.ts`, `knot.js`,
  `bootstrap.js`, the fake host (`scripts/extensions/testing/host.ts`), and
  `docs/extension-api.md`.
- **Key.** `secondary-e` in `SHARED_BINDINGS`; `Clear Recent Files` unbound.

## Exclusions and deferred

- A start surface listing recent files is D061; it reads the stable
  `recent-files` history name.
- No per-row remove action and no `~` shortening.

## Checkpoint 1 — Open Recent and Clear Recent Files ✅ (review gate pending)

Deliverables: both commands; dropping missing files; the key binding; the
`history.forget` primitive; a package description update in `knot.jsonc`
(`docs/settings.md` needs no row).

Validation:
- Package tests (`test/`, fake host): ordering most recent first; roots-relative
  versus outside-root descriptions; selecting opens; dismissing opens nothing;
  files that no longer exist are left out and forgotten, whether in the index's listing or not; a file the listing hides (ignored) but that exists stays; a provider failure keeps the entry; clearing empties the list; a window
  without roots still lists.
- Real-editor test in `crates/knot-ext-workbench/tests/workbench/` mirroring
  `go_to_file.rs`: open files, press the key, the picker lists them newest
  first, Enter opens one.
- Keymap test in `src/app/commands/keymaps/tests.rs` for the new binding and
  `BUNDLED_PACKAGE_COMMANDS` in `presets.rs`.
- `just check`.

Implemented: the `history.forget` primitive ✅, both commands ✅, the key ✅, tests ✅, `just check` ✅. The real-editor test reopens the list until the
workspace index has seen the deletion, since the index follows the disk
through the file watcher.

Review gate: try `Open Recent…` in the real app (via the ⌘E key) across
two windows, after deleting a recorded file (it should vanish from the list), and with no folder open.

**Implementation context.** Modify `extensions/@knot/go-to-file/src/extension.ts`
(register beside `COMMAND`; both commands in `start`), add
`src/openRecent.ts` for rows and open/prune logic, reuse `files.list`, `files.stat` and `rootName` from
`src/rows.ts` and `history.recent` from the API (`knot.d.ts`, ~line 945;
history code in `crates/knot-host/src/js/{knot.js,bootstrap.js}`). Mirror
`test/extension.test.ts` and `test/recent.test.ts`. Key binding in
`src/app/commands/keymaps/presets.rs` next to `GO_TO_FILE_COMMAND`; add the
command name to `BUNDLED_PACKAGE_COMMANDS`. Fake host:
`scripts/extensions/testing/host.ts`.
