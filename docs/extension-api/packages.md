# Sharing between packages

Part of the [extension guide](README.md).

## Share a feature with other packages

A package offers a feature to others by registering a named provider; others
find it by name and call it, so they reuse the feature rather than copy it:

```ts
import { providers } from "knot";

// In the providing package.
await providers.register("example.greeter", {
  async greet(name: string, context) {
    return `hello ${name}`; // context.window is the caller's window, or null
  },
});

// In another package, which declares the interface it expects.
interface Greeter { greet(name: string): Promise<string> }
const greeter = await providers.get<Greeter>("example.greeter");
const text = greeter === null ? "no greeter" : await greeter.greet("knot");
```

Each method takes one JSON argument and returns JSON, since the call runs in the
providing package. `get` returns the latest registration of a name, so a later
package or personal config replaces a provider for everyone; `all` returns
every registration, oldest first, for features several packages contribute to.
Look a provider up when the feature runs, and fall back when it is missing: a
handle rejects `ProviderNotFound` once its package unloads or restarts. Call
your own package's code directly, never through `providers`. The full contract
is `providers` in
[`knot.d.ts`](../../crates/knot-host/src/js/knot.d.ts).

### Provider names

Knot asks these providers itself. Each type is in `knot.d.ts`.

| Name | Type | Asked | Registrations used |
| --- | --- | --- | --- |
| `editor.completion` | `CompletionProvider` | When editor completion opens; items from every registration merge, and a request names its document, which a provider reads through `documents.get`. | All, oldest first |
| `editor.paste.<languageId>` | `PasteProvider` | When native paste runs in that language, to shape the inserted text; a language with none pastes without calling the host. Bundled: the Markdown package turns a URL pasted over selected text into a link. | Latest of that name |

Bundled packages register providers for each other:

| Name | Package | Methods |
| --- | --- | --- |
| `workspace.files`, `workspace.buffers`, `workspace.actions` | Go to File | `rows(query)`: the rows of that Search Everywhere tab for the caller's window |

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
