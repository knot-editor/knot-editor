# Settings

Part of the [extension guide](README.md).

## Read and write settings

Settings are one application-wide store that personal config and every package
share. Define your own keys with a type, a default, and a description, and
declare their types by merging into the `Settings` interface so `set` and `get`
stay typed:

```ts
import { settings } from "knot";

declare module "knot" {
  interface Settings {
    "lsp.timeout.launch": number;
  }
}

await settings.define({
  key: "lsp.timeout.launch",
  type: "integer",
  default: 52,
  description: "Timeout in seconds for launching the LSP process",
});
const timeout = await settings.get("lsp.timeout.launch");
await settings.observe("lsp.timeout.launch", async () => {
  /* read the new value with get */
});
```

A definition's `type` is `"boolean"`, `"integer"`, `"number"`, `"string"`,
`"strings"` (a list of strings), or `"integers"` (a list of integers).

A write applies everywhere, or only where every fact of its scope matches:
`language` is a document's language ID and `view` a surface kind such as
`editor` or a contributed view ID, and `document` a document's ID. Values
scoped to a document end when it closes. A read picks the matching value with the
most facts, so `settings.set("editor.tabSize", 8, { language: "rust" })` beats
an unscoped value however late that one was written. Among equally specific
values the latest write wins, whoever made it, so personal overrides belong in
`post-init`.

Native keys are declared in the `Settings` interface of `knot.d.ts`. The text
size and wrapping of editors are scoped like any other value: `post-init` can
size terminals apart from editors and wrap prose, for example:

```ts
await settings.set("font.size", 13, { view: "terminal" });
await settings.set("editor.softWrap", true, { language: "plaintext" });
```

The bundled Markdown package wraps `{ language: "markdown" }` this way, and
Toggle Soft Wrap writes `editor.softWrap` scoped to one document. Zoom is not a
setting: it scales every size and is remembered by the application.

Values may be written before their key is defined, which lets `pre-init`
configure a package that starts later. Definition drops a value of the wrong
type and notes it in the extension report; after definition, a mismatching
write rejects `SettingTypeMismatch`. Values live in memory and are written
again on every launch: teardown and Restart Extension remove only your
definitions and observers. To remember a choice, such as one a command makes,
keep it in [storage](application.md#durable-storage) and set it again in `start()`.
