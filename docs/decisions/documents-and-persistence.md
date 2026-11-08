# Documents and persistence decisions

Resource identity, workspaces, providers, and unsaved-text recovery. The
[decisions index](../decisions.md) lists every topic, and the [documentation
guide](../README.md#decisions) sets the entry rules.

## URI workspaces and filesystem providers

Normalized URIs and an async byte-oriented provider boundary keep generic code
free of platform paths. Documents are independent of workspace containment.
Local roots are trusted, not symlink sandboxes. No-clobber creation,
conditional replacement, and atomic installation protect content; identity
changes only after successful I/O.

Reference: [documents and persistence](../architecture/documents-and-persistence.md).

## Multi-root workspaces

A window holds an ordered, deduplicated root list, replaced as a whole.
Nested roots are allowed and browsed independently. Every root is a top-level
row so root actions always have a target. Removing a root never closes
documents.

Reference: [documents and persistence](../architecture/documents-and-persistence.md#workspaces-and-resource-trees).

## Application document and root identity

Document UUIDs separate logical identity from mutable URIs and handles. Root
identities with monotonic generations keep stale directory work from becoming
current again.

Reference: [documents and persistence](../architecture/documents-and-persistence.md).

## Semantic resource operations

Native commands and extension APIs share product operations that return
committed semantic data, because a status string or later focus query cannot
identify what was opened. Cancellation prevents later mutation but does not
roll back an accepted save.

References: [documents](../architecture/documents-and-persistence.md) and
[protected closure](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure).

## Folder selection

`workspaces.pickFolder` returns a normalized, checked directory without
committing roots, so a project workflow decides what to do with it. It uses
the window's transient interaction slot.

## Unsaved-document recovery store

Unsaved text is protected in an application-owned database independent of
extension health. Automatic rows protect live dirty documents; retained rows
are immutable checkpoints that automatic captures never overwrite. Captures
debounce about a second, at least every five seconds while typing, and commit
with FULL synchronization off the foreground, which is cheap next to losing
text. A missing or damaged database is preserved and reported.

Reference: [documents and persistence](../architecture/documents-and-persistence.md#durable-unsaved-document-recovery).

## Unsaved-document restore and hot exit

A recovered document loads its current resource and applies the text as one
edit, so dirty state comes from content comparison and no history is
persisted. A changed file keeps its capture-time version, so Save reaches the
conflict prompt. A live document is never replaced; conflicts open an untitled
copy. Startup restores before launch requests so they deduplicate
deterministically.

Quit is a hot exit, as in VS Code and Zed: once dirty documents are durable,
quit skips save prompts, and a store failure falls back to the prompts. Closing
a tab or window still prompts.

Reference: [documents and persistence](../architecture/documents-and-persistence.md#durable-unsaved-document-recovery).

## Retained recovery captures

Extensions retain text through application-stamped owners and IDs, so they
cannot name another owner or overwrite a capture. Owning by identity keeps
captures addressable across reloads and restarts. Each retain is a new
immutable capture that resolves after its durable commit, which orders records
without a barrier. Orphans are preferred to premature deletion; only release or
explicit discard removes a capture.

Reference: [documents and persistence](../architecture/documents-and-persistence.md#retained-captures).
