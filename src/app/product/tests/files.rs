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

#[gpui::test]
fn files_list_applies_the_ignore_settings_to_the_next_listing(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (filesystems, provider) = memory_filesystems_with_provider();
    for directory in ["mem://product/.git", "mem://product/dist"] {
        provider
            .seed_directory(ResourceUri::parse(directory).unwrap())
            .unwrap();
    }
    for (path, text) in [
        ("mem://product/.git/HEAD", "ref"),
        ("mem://product/.gitignore", "*.log\n"),
        ("mem://product/.git/config", ""),
        ("mem://product/build.log", ""),
        ("mem://product/dist/out.js", ""),
    ] {
        provider
            .seed_file(ResourceUri::parse(path).unwrap(), text)
            .unwrap();
    }
    cx.set_global(ApplicationFileSystems(filesystems));
    let host = cx.update(extensions::host::install);
    let key = install_operation_fixture(
        &host,
        r#"
import { commands, files, settings } from "knot";
async function names() {
  const entries = await files.list("mem://product/");
  return entries.map(entry => entry.name + (entry.ignored ? "!" : "")).sort().join();
}
await commands.register("files.policy", async () => {
  const base = ".gitignore,build.log!,dist,invalid.txt,notes.txt,src";
  if (await names() !== base) throw new Error(`default ${await names()}`);
  if (!await files.isIgnored("mem://product/.git/HEAD")) throw new Error("hidden");
  if (!await files.isIgnored("mem://product/build.log")) throw new Error("ignored");
  const walked = async () => (await files.walk("mem://product/")).map(entry => entry.name).sort().join();
  if (await walked() !== ".gitignore,invalid.txt,notes.txt,out.js") throw new Error(`walk ${await walked()}`);
  await settings.set("files.ignored", ["dist"]);
  if (await walked() !== ".gitignore,invalid.txt,notes.txt") throw new Error(`walk ignored ${await walked()}`);
  if (await names() !== ".gitignore,build.log!,dist!,invalid.txt,notes.txt,src") throw new Error(`ignored ${await names()}`);
  await settings.set("files.respectGitignore", false);
  if (await names() !== ".gitignore,build.log,dist!,invalid.txt,notes.txt,src") throw new Error(`respect ${await names()}`);
  await settings.set("files.hidden", ["notes.txt"]);
  if ((await names()).includes("notes.txt") || !(await names()).includes(".git")) throw new Error(`hidden ${await names()}`);
});
"#,
        "files.policy",
        cx,
    );
    let (shell, window) = empty_product_window(&documents, cx);
    let root = ResourceUri::parse("mem://product/").unwrap();
    shell.update(cx, |shell, cx| {
        shell.replace_workspace_roots(vec![root], cx)
    });
    assert_eq!(
        wait_for_command(
            dispatch_product_command(&shell, window, "files.policy", cx),
            cx
        ),
        CommandOutcome::Completed
    );
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
}

#[gpui::test]
fn files_watch_delivers_visible_batches_until_disposed_or_unloaded(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (filesystems, provider) = memory_filesystems_with_provider();
    provider
        .seed_directory(ResourceUri::parse("mem://product/src/.git").unwrap())
        .unwrap();
    cx.set_global(ApplicationFileSystems(filesystems));
    let host = cx.update(extensions::host::install);
    let key = install_operation_fixture(
        &host,
        r#"
import { commands, files, workbench } from "knot";
async function rejects(action, code) {
  try { await action(); } catch (error) { if (error.code === code) return; throw error; }
  throw new Error(`expected ${code}`);
}
let handle;
await commands.register("files.watch", async () => {
  await rejects(() => files.watch("mem://product/missing", () => {}), "ResourceNotFound");
  await rejects(() => files.watch("not a URI", () => {}), "InvalidUri");
  handle = await files.watch("mem://product/src", async (uris) => {
    await workbench.writeClipboardText(uris.join());
  });
});
await commands.register("files.unwatch", async () => { await handle.dispose(); });
"#,
        "files.watch",
        cx,
    );
    let (shell, window) = empty_product_window(&documents, cx);
    let run = |name: &'static str, cx: &mut TestAppContext| {
        assert_eq!(
            wait_for_command(dispatch_product_command(&shell, window, name, cx), cx),
            CommandOutcome::Completed
        );
    };
    let clipboard = |cx: &gpui::App| cx.read_from_clipboard().and_then(|item| item.text());
    run("files.watch", cx);
    assert_eq!(cx.read(extensions::host::file_watch_count), 1);

    // Hidden entries are dropped, and the rest arrive as one batch.
    for path in ["mem://product/src/.git/HEAD", "mem://product/src/a.txt"] {
        provider
            .seed_file(ResourceUri::parse(path).unwrap(), "x")
            .unwrap();
    }
    wait_for_api_condition(cx, |cx| {
        clipboard(cx).as_deref() == Some("mem://product/src/a.txt")
    });

    run("files.unwatch", cx);
    assert_eq!(cx.read(extensions::host::file_watch_count), 0);
    assert_eq!(provider.watch_count(), 0);

    run("files.watch", cx);
    assert_eq!(provider.watch_count(), 1);
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
    assert_eq!(cx.read(extensions::host::file_watch_count), 0);
    assert_eq!(provider.watch_count(), 0);
}

#[gpui::test]
fn indexed_roots_answer_from_memory_follow_the_disk_and_release_with_the_root(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    let (filesystems, provider) = memory_filesystems_with_provider();
    cx.set_global(ApplicationFileSystems(filesystems));
    let host = cx.update(extensions::host::install);
    let key = install_operation_fixture(
        &host,
        r#"
import { commands, files, workbench } from "knot";
await commands.register("files.report", async () => {
  const walked = (await files.walk("mem://product/src/")).map(entry => entry.name).sort().join();
  const listed = (await files.list("mem://product/src/")).map(entry => entry.name).sort().join();
  const outside = (await files.list("mem://product/")).map(entry => entry.name).sort().join();
  await workbench.writeClipboardText([walked, listed, outside].join("|"));
});
"#,
        "files.report",
        cx,
    );
    provider
        .seed_file(
            ResourceUri::parse("mem://product/src/kept.txt").unwrap(),
            "x",
        )
        .unwrap();
    let (shell, window) = empty_product_window(&documents, cx);
    let root = ResourceUri::parse("mem://product/src/").unwrap();
    let report = |cx: &mut TestAppContext| {
        assert_eq!(
            wait_for_command(
                dispatch_product_command(&shell, window, "files.report", cx),
                cx
            ),
            CommandOutcome::Completed
        );
        cx.read(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
            .unwrap()
    };
    let indexed = |cx: &mut TestAppContext| {
        cx.read(|cx| crate::app::file_index::ApplicationFileIndex::covering(cx, &root).is_some())
    };

    assert!(!indexed(cx));
    shell.update(cx, |shell, cx| {
        shell.replace_workspace_roots(vec![root.clone()], cx)
    });
    assert!(indexed(cx));
    assert_eq!(provider.watch_count(), 1);
    assert_eq!(report(cx), "kept.txt|kept.txt|invalid.txt,notes.txt,src");

    // An external change arrives through the watch.
    provider
        .seed_file(
            ResourceUri::parse("mem://product/src/added.txt").unwrap(),
            "x",
        )
        .unwrap();
    let mut seen = String::new();
    for _ in 0..100 {
        seen = report(cx);
        if seen.starts_with("added.txt,") {
            break;
        }
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(100));
        cx.run_until_parked();
    }
    assert_eq!(
        seen,
        "added.txt,kept.txt|added.txt,kept.txt|invalid.txt,notes.txt,src"
    );

    // Keeping the root keeps the index; dropping it releases the watch.
    shell.update(cx, |shell, cx| {
        shell.replace_workspace_roots(vec![root.clone()], cx)
    });
    assert_eq!(provider.watch_count(), 1);
    shell.update(cx, |shell, cx| {
        shell.replace_workspace_roots(Vec::new(), cx)
    });
    assert!(!indexed(cx));
    assert_eq!(provider.watch_count(), 0);
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
}
