# Documents and persistence decisions

Resource identity, workspaces, providers, and unsaved-text recovery. The
[decisions index](../decisions.md) lists every topic, and the
[documentation guide](../README.md#decisions) sets the entry rules.

## URI workspaces and filesystem providers

Normalized URIs and an async byte-oriented provider boundary keep generic
workspace and document code independent of platform paths. Documents stay
independent of workspace containment so Open and Save As can select any
resource. Local roots are trusted application boundaries, not symlink
sandboxes.

No-clobber creation, conditional replacement, and atomic installation protect
existing content, and identity changes only after successful I/O, so a failed
or racing save never leaves a document pointing at content it did not write.

Reference: [documents and persistence](../architecture/documents-and-persistence.md#uri-providers).

## Multi-root workspaces

A window's roots are one ordered list, replaced as a whole, so add, remove, and
reorder need no separate mutation API. Duplicates after normalization are
dropped rather than rejected: two spellings of one folder are the same root, so
an error would only make callers normalize first.

Nested and overlapping roots are allowed, because a monorepo and one of its
packages are both reasonable roots, and each is browsed independently. Every
root is a top-level row, even when there is only one, so root actions always
have an obvious target and there is one layout and keyboard model. Folder Open
opens a window without roots in place and any other window in a new one, as
Zed does, so root edits inside a window come only from adding and removing
folders and a window never changes which remembered folder it shows. Removing a root
never closes documents, since documents are independent of workspace
containment.

Reference: [documents and persistence](../architecture/documents-and-persistence.md#workspaces-and-resource-trees).

## Application document and root identity

Document UUIDs separate logical identity from mutable URIs, collection IDs, and
transport handles. Root identities with monotonic generations keep a clear and reinstall
from making old asynchronous directory work current again.

Reference: [documents and persistence](../architecture/documents-and-persistence.md#document-ownership).

## Semantic resource operations

Native commands and extension APIs share product operations that return
committed semantic data, because a status string or a later focus query cannot
reliably identify what was opened. Caller cancellation prevents later mutation
but does not roll back a save the shared persistence path already accepted.

References: [documents](../architecture/documents-and-persistence.md#opening-resources) and
[protected closure](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure).

## Folder selection

`workspaces.pickFolder` is a dedicated operation rather than a general resource
picker or implicit reuse of the Open dialog, because a workflow that manages
project roots needs a checked directory before deciding whether to open or
replace them; it therefore never commits roots itself. It uses the window's
transient interaction slot, so ownership, cancellation, and targeting match
other interactions, and checks the path through providers so callers need not
revalidate it.

Reference: [workspaces](../architecture/documents-and-persistence.md#workspaces-and-resource-trees).

## Extension file reads

Reads are ungated and text-only. D087's entitlements gate processes and the
network, the only ways file contents could leave the machine, so reading alone
exposes nothing a package cannot already leak by being installed; a read gate
would add consent prompts without a boundary behind them. Workspace trust
remains deferred. Reading ignores workspace roots because the consumers look
upward from a document (EditorConfig, project settings) and outside any root.

Bytes are not exposed because no consumer needs non-UTF-8 content, and a binary
transport would be designed without one. `list` takes no sort or filter options:
the explorer wants to own its order, and a caller-supplied filter would add a
second pattern syntax beside project settings' globs.

Reference: [URI providers](../architecture/documents-and-persistence.md#uri-providers).

## Workspace ignore policy

One native policy, set by three global settings, decides what `files.list`,
`files.walk`, and `files.isIgnored` show. Hidden entries (`.git`,
`.DS_Store`) never appear, so hiding is policy rather than a filter syntax a
caller passes; ignored entries still appear in listings, marked, so the
explorer can dim them, and walks skip them. Defaults match VS Code and Zed.
Both pattern lists use gitignore syntax, so there is no second pattern
language, and repository ignore files follow ripgrep's semantics, which is what
D085 gets by running `rg` with only `files.ignored` as globs.

Evaluation is native so that watching, walking, and listing never call
JavaScript. The walk is native rather than `rg --files` because the policy must
also serve listings and watching, and one classifier keeps all three
consistent; on a 97,336-file kernel checkout it takes about 1.1 s against
0.15 s for `rg --files`, acceptable for a one-shot Go to File index. Settings
replace an extension-contributed rule registration, which waits for a real
consumer, and no API exposes the effective rules, since `rg` reads repository
ignore files itself. The policy is global and tied to the filesystem, not to
workspace roots; project-local overrides wait for D089.

Reference: [URI providers](../architecture/documents-and-persistence.md#uri-providers),
[settings](../settings.md).

## File watching

Documents and extensions request watches; nothing watches workspace roots by
default. VS Code and Zed watch roots because they keep a tree snapshot to
update, which the workspace file index (D173) adds as one more recursive
watcher. Until then a root watch would cost OS watches and wake-ups nobody
reads.

Change detection compares the provider version, then the content, instead of
ignoring events for a time window after Knot's own save: a window drops a real
external write that lands inside it, while an equal version identifies Knot's
own write exactly.

A reload applies a full line diff rather than replacing the text, or replacing
the span between a common prefix and suffix as VS Code and Emacs do. Replacing
the text loses carets, folds, and anchored ranges everywhere; the span still
loses them between two distant hunks, as after `git checkout`. The diff costs
one dependency (`imara-diff`).

A document changed on disk while dirty prompts when it is, or next becomes, the
focused tab, rather than staying silent until save (VS Code) or prompting at
once whichever tab is showing. Silence hides the conflict until it costs a
rejected save; an immediate prompt interrupts work in another tab.

Reference: [detecting external changes](../architecture/documents-and-persistence.md#detecting-external-changes),
[watching](../architecture/documents-and-persistence.md#watching).

## Unsaved-document recovery store

Unsaved text is protected in an application-owned database, independent of
extension health and separate from extension record stores, so a broken package
can never lose text. Retained captures are immutable rows that automatic
protection never overwrites, so a suspended session's text survives later edits
of the live document.

Each capture commits with FULL synchronization before it is accepted, so the
crash-loss window is the capture cadence itself. Deferring durability to save was
rejected: a FULL commit off the foreground is far cheaper than losing text. A
damaged database is preserved and reported rather than replaced.

Reference: [durable recovery](../architecture/documents-and-persistence.md#durable-unsaved-document-recovery).

## Unsaved-document restore and hot exit

A recovered document loads its current resource and applies the text as one
edit, so dirty state comes from the ordinary content comparison and no
revisions, counters, or history are persisted. Keeping the capture-time provider
version means Save on a changed file reaches the conflict prompt rather than
silently clobbering it, as VS Code and Zed do. A live document is never replaced,
so a conflict opens an untitled copy rather than choosing between two texts.
Restoring before launch requests lets them deduplicate deterministically instead
of racing. Leftover snapshots reuse the native picker and prompt rather than a
dedicated recovery UI.

Quit is a hot exit, matching Sublime Text, VS Code, and Zed: without it, the
quit prompt's Don't Save is an explicit discard and recovery would only help
after crashes. A store failure falls back to the prompts, so quit never loses
text silently. Closing a tab or window still prompts, because it is a deliberate
removal rather than leaving the application.

Reference: [durable recovery](../architecture/documents-and-persistence.md#durable-unsaved-document-recovery).

## Retained recovery captures

The application stamps the owner and generates the capture ID, so an extension
cannot name another owner or overwrite a capture. Owning by package identity
rather than lifecycle keeps captures addressable across reload, failure, and
restart, which is what a session needs. Each retain is a new immutable capture
that resolves after its durable commit, which gives sessions the ordering they
need without a separate barrier call.

Orphans are preferred to premature deletion: only the owner's release or the
user's explicit discard removes a capture, and the native listing keeps orphans
inspectable. Release and discard are the same deletion, so the public API
exposes only release.

Reference: [retained captures](../architecture/documents-and-persistence.md#retained-captures).
