# Documents and persistence decisions

Resource identity, workspaces, providers, and unsaved-text recovery. The
[decisions index](../decisions.md) lists every topic, and the
[documentation guide](../README.md#decisions) sets the entry rules.

## URI workspaces and filesystem providers

Normalized URIs and an async byte-oriented provider boundary keep generic
workspace and document code independent of platform paths. Documents remain
independent of workspace containment so Open and Save As can select any resource.
Local roots are trusted application boundaries, not symlink sandboxes.

No-clobber creation, conditional replacement, and atomic local installation
protect existing content. Persistence identity changes only after successful I/O;
saving the captured content state leaves racing edits dirty. Application-wide URI
ownership prevents two open documents from claiming the same destination.

Reference: [documents and persistence](../architecture/documents-and-persistence.md).

## Multi-root workspaces

A window holds an ordered list of roots. Proposed sets are deduplicated after
normalization, keeping the first occurrence, rather than rejected: two spellings
of one folder are the same root, so an error would only make callers normalize
first. Retained URIs keep their root IDs, which makes add, remove, and reorder
plain replacements of the whole list with no separate mutation API.

Nested and overlapping roots are allowed, because a monorepo and one of its
packages are both reasonable roots. Each root is browsed independently: explorer
expansion and selection are keyed by root ID and URI so the two copies of a
folder stay independent, while directory listings are cached by URI. Every root
is a top-level row, even when there is only one, so root actions always have an
obvious target and there is one layout and keyboard model. Folder Open keeps
replacing every root, matching its single-root meaning; Add Folder appends.
Removing a root never closes documents, since documents are independent of
workspace containment.

Reference: [documents and persistence](../architecture/documents-and-persistence.md#workspaces-and-resource-trees).

## Application document and root identity

Document UUIDs separate logical identity from mutable URIs, collection IDs, and
transport handles. Window-owned root identities and monotonic generations
prevent clear/reinstall from making old asynchronous directory work current
again.

Reference: [documents and persistence](../architecture/documents-and-persistence.md).

## Semantic resource operations

Share product operations between native commands and extension APIs, returning
committed semantic data independently of execution outcomes. A status string or
later focus query cannot reliably identify an opened presentation. Root mutations
address windows independently of selected documents; protected closure preserves
native dirty-document decisions. Caller cancellation prevents later mutation but
does not roll back a save already accepted by the shared persistence path.

References: [documents](../architecture/documents-and-persistence.md) and
[protected closure](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure).

## Folder selection

Expose folder selection through `workspaces.pickFolder` rather than a general
resource picker or implicit reuse of the Open dialog. A workflow that manages
project roots needs a normalized directory before deciding whether to open or
replace them, and the operation deliberately never commits roots itself.

The native directory prompt occupies the existing per-window transient interaction
slot, so ownership, cancellation, and target resolution match other extension
interactions. The selected path is normalized and stat-checked through registered
providers, so callers receive a usable URI or a typed `ResourceNotFound` /
`ResourceWrongKind` failure instead of revalidating the path themselves.

## Unsaved-document recovery store

Protect unsaved text in an application-owned recovery database, independently of
extension health and separate from the extension record stores. An `automatic`
row per document UUID is replaceable protection for a live dirty document; a
`retained` row is an immutable opaque-key checkpoint owned by a suspended session
or another extension owner. Later automatic captures never overwrite retained
rows, and saving retires the automatic row without invalidating a retained one.

Capture copies text on the foreground and commits off it. A trailing-edge
debounce settles about a second after edits stop and fires at least every five
seconds while typing continues. Each fired capture commits with FULL
synchronization before it is accepted, so the advertised crash-loss window is the
cadence itself. Deferring durability to save is rejected: a FULL commit off the
foreground is far cheaper than losing text to avoid it. The quit, close, and
command paths only drain a capture still inside its debounce window.

The database opens lazily off the foreground, and a missing, corrupt, or
incompatible database is preserved and reported rather than replaced. Dirty
documents are observed through their shared model, so multiple views produce one
row and no extension or session metadata is required.

Reference: [documents and persistence](../architecture/documents-and-persistence.md#durable-unsaved-document-recovery).

## Unsaved-document restore and hot exit

Rebuild a recovered document by loading its current resource as revision 0 and
applying the recovered text as one edit with an empty undo history. Dirty state
then comes from the ordinary content comparison, so no revisions, counters, or
history are persisted, and text equal to its baseline restores clean. Restore
never writes a resource. A changed file keeps the capture-time provider version,
so Save reaches the existing Reload / Save As / Cancel conflict prompt instead of
silently clobbering, as VS Code and Zed do; a deleted file becomes a dirty
destination.

A live document is never replaced. When another document owns the URI, or a
retained capture's document is open, the text opens as a separate untitled copy;
for an automatic snapshot one transaction moves the protection to that copy, so
the text is never unprotected and is not listed twice. Automatic restores keep
the document UUID so later captures replace the same row; retained restores take
a fresh one because the automatic role may still protect newer text.

Startup restores unclaimed automatic snapshots before the launch gate opens
windows, so launch requests deduplicate against them deterministically instead
of racing them. Only the newest snapshot per URI is restored; older duplicates,
unreadable baselines, and retained captures remain for Recover Unsaved
Documents, which reuses the native picker and prompt rather than adding a
dedicated recovery UI.

Quit is a hot exit, matching Sublime Text, VS Code, and Zed: once every dirty
document is durably committed, quit skips the Save / Don't Save / Cancel prompts
and the next launch restores them. Without this, the prompt's Don't Save is an
explicit discard and recovery would only help after crashes. A store that fails
to open, write, or drain falls back to the prompts so quit never loses text
silently. Closing a tab or window still prompts because it is a deliberate
removal rather than leaving the application.

Reference: [documents and persistence](../architecture/documents-and-persistence.md#durable-unsaved-document-recovery).

## Retained recovery captures

Extensions retain document text through application-stamped ownership: the
owner is the canonical package or personal-config identity bound at admission,
and the capture ID is application-generated. An extension therefore cannot name
another owner, choose a table, or overwrite a capture, and a foreign ID behaves
like a missing one. Owning by identity rather than lifecycle keeps captures
addressable across reload, failure, and restart, which is what a session needs.

Each retain creates a new immutable capture, so an owner may hold several
captures of one document: a suspended session and its replacement can reference
different text until the old record is released. Retain resolves only after its
FULL-synchronized commit, which gives sessions the ordering they need without a
separate barrier call: commit the record after retain, release the old capture
after the record.

Prefer orphans to premature deletion. Nothing removes a retained capture except
its owner's release or the user's explicit discard; a cancelled retain may leave
one behind. Owners reconcile their listing against their records, and the native
listing shows every retained capture with its owner, so orphans stay inspectable.
Startup neither restores nor counts retained captures, since their owners decide
when to restore them. Release and discard are the same deletion, so the public
API exposes only release.

Reference: [documents and persistence](../architecture/documents-and-persistence.md#retained-captures).
