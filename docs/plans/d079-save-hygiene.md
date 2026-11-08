# D079 — Save hygiene

Status: 🚧 both checkpoints implemented; waiting on the review gates. Item of the
[daily-driver plan](daily-driver.md); depends on D070 (settings).

## Goal

Saving a file can trim trailing whitespace and ensure it ends in a newline,
each controlled by a setting that resolves per language and per document. The
cleanup is one ordinary undoable edit applied before the save captures its
content, so what is written is what the editor shows and Undo takes it back.
A palette command trims on demand without saving.

## Design

- **Settings.** `files.trimTrailingWhitespace`, a string of `"off"` or `"all"`
  (default `"all"`; other text means `"all"`, as `editor.lineNumbers` does),
  and `files.insertFinalNewline`, a boolean defaulting to `true`. They are
  defined natively beside the `editor.*` keys and read with the document's settings context, so a language or one
  document can opt out (Markdown, say). They join a "Files" table in
  [settings.md](../settings.md).
- **Where it runs.** Save and Save As reach `start_save_to_uri`; the cleanup
  runs there, on the foreground, immediately before `text` and `revision` are
  read from the model. Autosave-like paths (recovery snapshots) never run it.
  The cleanup goes through the target's editor view, so it is an editor edit
  with the usual undo step that restores the view, mirroring the buffer-wide
  line commands (`reindent_lines`, `convert_indentation` and
  `commit_line_edits` in `src/app/editor/indentation.rs`). Cleanup that
  changes text makes the document dirty until the save completes; edits that
  race the I/O still leave it dirty, as today.
- **Trim.** `"all"` trims every line; Knot has no VCS layer to define modified
  lines yet, and the string setting leaves room for a `"modified"` value
  (D159). A language or document that must keep its whitespace sets `"off"`. Spaces and tabs go; the `\r` of a CRLF
  terminator stays. As in IntelliJ and VS Code with typing in progress, the
  line of each cursor is skipped on save, so saving mid-line never eats the
  indentation just typed.
- **Final newline.** Appended with `model.line_ending()` (the buffer's first
  terminator, so CRLF files stay CRLF) when the text is non-empty and does not
  already end in a terminator. Trailing blank lines are left alone.
- **Read-only models.** The cleanup is skipped and the save proceeds; a failed
  cleanup never blocks a save.
- **Manual command.** `editor.trim-trailing-whitespace`, palette title "Trim
  Trailing Whitespace", trims every line including the cursor's, as one undo
  step, regardless of the setting.

## Exclusions and deferred decisions

- No formatter hook. [D115](../roadmap.md) adds awaited before-save hooks and
  format on save follows with D067; the cleanup is one function called before
  capture so the hooks can run first.
- No `"modified"` value; it is tracked in the roadmap as D159 and waits for
  a VCS or diff layer.
- No collapsing of trailing blank lines, and no line-ending conversion (D149).

## Checkpoints

### 1. ✅ Cleanup edits and settings

Deliver the two settings with their resolver functions, the edit computation,
and the manual command.

- Add `FILES_TRIM_TRAILING_WHITESPACE` (string) and
  `FILES_INSERT_FINAL_NEWLINE` (boolean) definitions and `files_*(context, cx)`
  resolvers in `src/app/settings.rs`,
  beside `editor_soft_wrap`; extend `src/app/settings/tests.rs`.
- Add a `src/app/editor/` method (new `save_hygiene.rs` next to
  `indentation.rs`) that computes the trim and final-newline edits from the
  model and commits them with `commit_line_edits`; it takes a flag for
  skipping cursor lines.
- Register `editor.trim-trailing-whitespace` in
  `src/app/editor/native_commands.rs` and dispatch it in
  `src/app/editor/editing.rs`.
- Tests in `src/app/editor/tests/`: trailing spaces and tabs, CRLF kept,
  cursor-line skip, empty buffer, existing final newline, CRLF final newline,
  one undo step, read-only no-op.

**Review gate** (`Try: Save hygiene sample` in the personal config): the manual command behaves right in the real app and Undo
restores the text and the cursor (`try.*` command in the personal config).

### 2. ✅ Cleanup on save

- In `src/app/product/save.rs`, before the capture in `start_save_to_uri`,
  call `Editor::clean_up_for_save` on the target's editor view, which resolves
  the two settings for the document and does nothing for a read-only model.
- Product tests in `src/app/product/tests/`: save writes the cleaned text;
  Undo restores it and marks the document dirty; a per-language `"off"` and a
  per-document opt-out leave the text alone; Save As cleans too; a read-only
  document saves unchanged.
- Docs: the Saving section of
  [documents and persistence](../architecture/documents-and-persistence.md),
  the Files table in [settings.md](../settings.md), the settings section of
  the [extension guide](../extension-api.md#read-and-write-settings) if keys
  are listed there, and the D079 entry in the daily-driver plan. Note under
  D149 in the roadmap that conversion should keep `line_ending()` consistent
  with final-newline insertion.

**Review gate:** save a file with trailing whitespace and no final newline,
undo, toggle each setting for a language, and confirm the tab dirty state.
Remove this plan and mark D079 ✅ in the daily-driver plan once approved.
