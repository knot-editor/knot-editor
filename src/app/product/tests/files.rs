//! The extension file API through the public host.

use super::operations::{empty_product_window, install_operation_fixture};
use super::*;
use crate::app::extensions;

#[gpui::test]
fn files_read_list_and_stat_stored_resources_and_ignore_unsaved_edits(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (filesystems, _) = memory_filesystems_with_provider();
    cx.set_global(ApplicationFileSystems(filesystems));
    let host = cx.update(extensions::host::install);
    let key = install_operation_fixture(
        &host,
        r#"
import { commands, documents, files, windows } from "knot";
async function rejects(action, code) {
  try { await action(); } catch (error) { if (error.code === code) return; throw error; }
  throw new Error(`expected ${code}`);
}
await commands.register("files.api", async () => {
  if (await files.readText("mem://product/notes.txt") !== "loaded text") throw new Error("read");
  const entries = [...await files.list("mem://product/")].sort((a, b) => a.name.localeCompare(b.name));
  const summary = entries.map(entry => `${entry.name}:${entry.kind}:${entry.uri}`).join();
  if (summary !== "invalid.txt:file:mem://product/invalid.txt,notes.txt:file:mem://product/notes.txt,src:directory:mem://product/src") throw new Error(`list ${summary}`);
  if (!Object.isFrozen(entries[0])) throw new Error("mutable entry");
  const file = await files.stat("mem://product/notes.txt");
  if (file.kind !== "file" || !Object.isFrozen(file)) throw new Error("stat file");
  if ((await files.stat("mem://product/src")).kind !== "directory") throw new Error("stat directory");
  if (await files.stat("mem://product/missing") !== null) throw new Error("stat missing");
  await rejects(() => files.readText("mem://product/missing"), "ResourceNotFound");
  await rejects(() => files.readText("mem://product/src"), "ResourceWrongKind");
  await rejects(() => files.readText("mem://product/invalid.txt"), "InvalidUtf8");
  await rejects(() => files.readText("not a URI"), "InvalidUri");
  await rejects(() => files.readText("other://product/file"), "ProviderFailure");
  await rejects(() => files.list("mem://product/notes.txt"), "ResourceWrongKind");
  await rejects(() => files.list("mem://product/missing"), "ResourceNotFound");
  await rejects(() => files.stat("not a URI"), "InvalidUri");
  try { await files.readText(42); throw new Error("nontext URI accepted"); } catch (error) { if (!(error instanceof TypeError)) throw error; }

  const wins = await windows.list();
  const opened = await documents.open("mem://product/notes.txt", { target: wins[0].target });
  await opened.document.buffer.applyEdits([{ range: { startByteOffset: 0, endByteOffset: 0 }, text: "unsaved " }], { ifRevision: 0 });
  if ((await opened.document.buffer.snapshot()).text !== "unsaved loaded text") throw new Error("edit");
  if (await files.readText("mem://product/notes.txt") !== "loaded text") throw new Error("read saw unsaved edits");
});
"#,
        "files.api",
        cx,
    );
    let (shell, window) = empty_product_window(&documents, cx);
    assert_eq!(
        wait_for_command(
            dispatch_product_command(&shell, window, "files.api", cx),
            cx
        ),
        CommandOutcome::Completed
    );
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
}
