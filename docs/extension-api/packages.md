# Sharing between packages

Part of the [extension guide](README.md).

## Share a feature with other packages

A package offers a feature to others through a contract defined once in code:
the provider publishes a schema-built contract, a consumer imports it and calls
typed functions that run in the provider's package, and the host validates
every argument and result against the schemas. The contract module is named by
`contract` in the manifest, next to `main`. It is pure schema code: it may
import only `"knot/contract"` and its own relative modules, so a consumer can
evaluate it without running the provider.

```ts
// contract.ts, the provider's "contract": "dist/contract.js"
import { defineContract, s } from "knot/contract";
export const greeter = defineContract("greeter", {
  greet: s.fn(s.string(), s.string()),
});

// main.ts, in the providing package.
import { greeter } from "./contract.js";
await greeter.serve({
  async greet(name, context) {
    return `hello ${name}`; // context.window is the caller's window, or null
  },
});

// In another package that lists "@example/greeter" in `requires`.
import { greeter } from "@example/greeter";
const text = await (await greeter.client()).greet("knot");
```

A runnable version is in [`contract.js`](../../scripts/extensions/examples/contract.js)
and [`serve.js`](../../scripts/extensions/examples/serve.js). A contract is identified by its package and name, so the author writes only the
name. Types are inferred from the schemas, which accept JSON-shaped values
(`s` in [`knot.d.ts`](../../crates/knot-host/src/js/knot.d.ts)). `client()`
binds the latest registration, so a later package, or personal config from
post-init, replaces a provider for everyone and disposing it uncovers the
previous one. A consumer that can do without the provider catches the
`ContractNotFound` of `client()` or of a call, which a held client also throws
once its provider unloads or restarts; look the client up again. A call whose
argument or result breaks a schema rejects `ContractViolation`, and a method
that throws rejects `ProviderFailure`. Call your own package's code directly:
a package cannot call a contract it serves.

### Contracts Knot defines

Knot asks these contracts itself. Any package serves them with `serve`, and
none needs `requires`. Each type is in `knot.d.ts`.

| Served with | Type | Asked | Registrations used |
| --- | --- | --- | --- |
| `completionContract.serve(provider)` | `CompletionProvider` | When editor completion opens; items from every registration merge, and a request names its document, which a provider reads through `documents.get`. | All, oldest first |
| `pasteContract(languageId).serve(provider)` | `PasteProvider` | When native paste runs in that language, to shape the inserted text; a language with none pastes without calling the host. Bundled: the Markdown package turns a URL pasted over selected text into a link. | Latest for the language |

Knot checks each result against the contract's schema, so a provider that
returns a malformed list or a non-string paste fails only its own part.

Bundled packages publish contracts for each other:

| Package | Contract | Methods |
| --- | --- | --- |
| `@knot/go-to-file` | `workspace` | `files(query)`, `buffers(query)`, `actions(query)`: the rows of that Search Everywhere tab for the caller's window |
| `@knot/vcs` | `vcs`, `backendContract(id)` | `vcs`, served by `@knot/vcs`: backends `register(id)`, `unregister(id)`, and report `changed(...)` baselines or `statusChanged(...)` files. `backendContract(id)`, served by the backend `id` and called by `@knot/vcs`: `repositories(root)`, `repositoryOf(file)`, `baseline(...)`, `status(repository)`, `ref(repository)` |

A version control backend lists `@knot/vcs` in `requires`, serves its own
`backendContract(id)`, and registers; `@knot/vcs` then draws the change gutter
from its baselines and paints file status and the current ref from `status` and
`ref` (see [version control](../architecture/version-control.md)). Call
`changed` when baselines may differ, such as after a checkout, and
`statusChanged` when only the working tree did:

```js
import { backendContract, vcs } from "@knot/vcs";

await backendContract("mine").serve({
  repositories: (root) => [root],
  repositoryOf: (file) => null,
  baseline: ({ repository, file }) => "text of the file at the baseline\n",
  status: (repository) => [{ file: "file:///work/a.txt", status: "modified" }],
  ref: (repository) => ({ label: "main" }),
});
const client = await vcs.client();
await client.register("mine");
// Later: await client.changed({ backend: "mine", repository: "file:///work/" });
```

## Identity and lifetime

| Identity | Ownership and validity | Persist? |
| --- | --- | --- |
| Package name | Canonical installed identity, independent of activation | Yes, as a package name |
| Runtime/config lifecycle | Hidden; config phases share their own lifecycle | No |
| Window/view IDs | Opaque live identities in a command capture | No |
| Command target/invocation | Invocation-scoped authority; invocation ID is hidden | No |
| Document ID | Application-allocated; stable across Save As and presentations | Only to refer to a document restored from automatic recovery, which keeps its ID across restart |
| Live window/pane/view target | Exact discoverable product presentation, independent of commands | No |
| Root ID | Window-owned; retained while its normalized URI remains installed | No |
| Buffer proxy | Live shared model, independent of any one presentation | No |
| Registration/subscription | Extension-owned until disposal, resource loss or teardown; IDs hidden | No |
| Contribution set | One set per buffer and extension lifecycle | No |
| View decoration set | One set per editor and extension lifecycle | No |
| Revision/generation | Content/request correlation, not reusable authority | No |
| Tree item ID | Author-defined identity within a provider tree | Provider-defined |

A buffer proxy or URI is not persistent document identity. Command targets expose
no native handles and cannot be reconstructed from saved IDs. Retained explicit
buffers support asynchronous work while live; retained command targets do not
extend the invocation's authority. Lifecycle teardown releases ownership without
requiring the extension to await cleanup or receive a final callback.

## Native contributors

Handler annotation and conversion contracts live in the
[host handler module](../../src/app/extensions/host/handlers.rs) and
[command declarations](../../src/app/commands/declarations.rs). See
[host architecture](../architecture/extension-host.md) and
[commands](../architecture/commands.md) for ownership and routing.
