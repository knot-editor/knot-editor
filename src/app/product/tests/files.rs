//! The extension file API through the public host.

use super::operations::{empty_product_window, install_operation_fixture};
use super::*;
use crate::app::documents::{DocumentDirty as _, ExternalChange};
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
  const searched = await files.search("mem://product/", "notes");
  if (searched.more || searched.results.map(hit => hit.path).join() !== "notes.txt") throw new Error("search");
  await rejects(() => files.search("mem://product/notes.txt", "x"), "ResourceWrongKind");
  await rejects(() => files.search("mem://product/missing", "x"), "ResourceNotFound");
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
  const walked = async () => (await files.search("mem://product/", "", { limit: 1000 })).results.map(hit => hit.name).sort().join();
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
fn files_search_ranks_several_directories_as_one_whether_or_not_they_are_indexed(
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
  const found = await files.search(["mem://product/src/", "mem://product/"], "notes");
  const empty = await files.search([], "notes");
  await workbench.writeClipboardText([
    ...found.results.map(hit => `${hit.directory}:${hit.path}`),
    empty.results.length,
  ].join("|"));
});
"#,
        "files.report",
        cx,
    );
    provider
        .seed_file(
            ResourceUri::parse("mem://product/src/notes.md").unwrap(),
            "x",
        )
        .unwrap();
    let (shell, window) = empty_product_window(&documents, cx);
    // Only the first directory is an indexed root; the second is walked.
    shell.update(cx, |shell, cx| {
        shell.replace_workspace_roots(vec![ResourceUri::parse("mem://product/src/").unwrap()], cx)
    });
    assert_eq!(
        wait_for_command(
            dispatch_product_command(&shell, window, "files.report", cx),
            cx
        ),
        CommandOutcome::Completed
    );
    assert_eq!(
        cx.read(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
            .unwrap(),
        "mem://product/src/:notes.md|mem://product/:notes.txt|mem://product/:src/notes.md|0"
    );
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
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
  const walked = (await files.search("mem://product/src/", "")).results.map(hit => hit.name).sort().join();
  const listed = (await files.list("mem://product/src/")).map(entry => entry.name).sort().join();
  const outside = (await files.list("mem://product/")).map(entry => entry.name).sort().join();
  const found = await files.search("mem://product/src/", "kept", { limit: 1 });
  const hit = found.results[0];
  const searched = hit ? `${hit.path}:${hit.ranges.map(range => range.join("-"))}:${found.more}` : "none";
  await workbench.writeClipboardText([walked, listed, outside, searched].join("|"));
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
    assert_eq!(
        report(cx),
        "kept.txt|kept.txt|invalid.txt,notes.txt,src|kept.txt:0-4:false"
    );

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
        "added.txt,kept.txt|added.txt,kept.txt|invalid.txt,notes.txt,src|kept.txt:0-4:false"
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

fn document_at(
    documents: &Entity<DocumentCollection>,
    uri: &str,
    cx: &TestAppContext,
) -> Option<(super::super::DocumentId, Entity<Buffer>)> {
    cx.read(|cx| {
        let documents = documents.read(cx);
        let id = documents.document_for_resource(&ResourceUri::parse(uri).unwrap())?;
        Some((id, documents.get(id)?.model().clone()))
    })
}

#[gpui::test]
fn files_create_rename_and_delete_change_the_stored_resources(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (filesystems, _) = memory_filesystems_with_provider();
    cx.set_global(ApplicationFileSystems(filesystems));
    let host = cx.update(extensions::host::install);
    let key = install_operation_fixture(
        &host,
        r#"
import { commands, files } from "knot";
async function rejects(action, code) {
  try { await action(); } catch (error) { if (error.code === code) return; throw error; }
  throw new Error(`expected ${code}`);
}
await commands.register("files.mutate", async () => {
  if (await files.createFile("mem://product/new/deep/a.txt") !== "mem://product/new/deep/a.txt") throw new Error("create");
  if ((await files.stat("mem://product/new/deep")).kind !== "directory") throw new Error("parents");
  if (await files.readText("mem://product/new/deep/a.txt") !== "") throw new Error("empty");
  await files.createDirectory("mem://product/made");
  await rejects(() => files.createFile("mem://product/new/deep/a.txt"), "ResourceAlreadyExists");
  await rejects(() => files.createDirectory("mem://product/made"), "ResourceAlreadyExists");
  await rejects(() => files.createFile("mem://product/notes.txt/x"), "ResourceWrongKind");
  await rejects(() => files.rename("mem://product/notes.txt", "mem://product/made"), "ResourceAlreadyExists");
  await rejects(() => files.rename("mem://product/src", "mem://product/src/in"), "InvalidUri");
  await rejects(() => files.rename("mem://product/gone", "mem://product/x"), "ResourceNotFound");
  if (await files.rename("mem://product/notes.txt", "mem://product/made/sub/notes.txt") !== "mem://product/made/sub/notes.txt") throw new Error("rename");
  if (await files.stat("mem://product/notes.txt") !== null) throw new Error("source remains");
  if (await files.readText("mem://product/made/sub/notes.txt") !== "loaded text") throw new Error("moved text");
  if (await files.copy("mem://product/made", "mem://product/dup/made") !== "mem://product/dup/made") throw new Error("copy");
  if (await files.readText("mem://product/dup/made/sub/notes.txt") !== "loaded text") throw new Error("copied text");
  if (await files.readText("mem://product/made/sub/notes.txt") !== "loaded text") throw new Error("source kept");
  await rejects(() => files.copy("mem://product/made", "mem://product/dup/made"), "ResourceAlreadyExists");
  await rejects(() => files.copy("mem://product/made", "mem://product/made/in"), "InvalidUri");
  await rejects(() => files.copy("mem://product/gone", "mem://product/x"), "ResourceNotFound");
  await rejects(() => files.reveal("mem://product/made"), "ProviderFailure");
  await rejects(() => files.reveal("not a URI"), "InvalidUri");
  await files.delete("mem://product/made");
  if (await files.stat("mem://product/made") !== null) throw new Error("delete");
  await rejects(() => files.delete("mem://product/made"), "ResourceNotFound");
  await files.delete("mem://product/new", { permanent: true });
  if (await files.stat("mem://product/new") !== null) throw new Error("permanent delete");
});
"#,
        "files.mutate",
        cx,
    );
    let (shell, window) = empty_product_window(&documents, cx);
    assert_eq!(
        wait_for_command(
            dispatch_product_command(&shell, window, "files.mutate", cx),
            cx
        ),
        CommandOutcome::Completed
    );
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
}

#[gpui::test]
fn open_documents_follow_a_rename_and_keep_their_edits_history_and_save_path(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    cx.set_global(save_without_cleanup());
    let (filesystems, provider) = memory_filesystems_with_provider();
    provider
        .seed_file(
            ResourceUri::parse("mem://product/src/lib.txt").unwrap(),
            "lib",
        )
        .unwrap();
    cx.set_global(ApplicationFileSystems(filesystems));
    let host = cx.update(extensions::host::install);
    let key = install_operation_fixture(
        &host,
        r#"
import { commands, documents, files, windows } from "knot";
await commands.register("files.open", async () => {
  const [window] = await windows.list();
  await documents.open("mem://product/src/lib.txt", { target: window.target });
  const opened = await documents.open("mem://product/notes.txt", { target: window.target });
  await opened.document.buffer.applyEdits([{ range: { startByteOffset: 0, endByteOffset: 0 }, text: "edited " }], { ifRevision: 0 });
});
await commands.register("files.rename", async () => {
  await files.rename("mem://product/notes.txt", "mem://product/docs/renamed.md");
  await files.rename("mem://product/src", "mem://product/lib");
});
"#,
        "files.open",
        cx,
    );
    let (shell, window) = empty_product_window(&documents, cx);
    let run = |name: &'static str, cx: &mut TestAppContext| {
        assert_eq!(
            wait_for_command(dispatch_product_command(&shell, window, name, cx), cx),
            CommandOutcome::Completed
        );
    };
    run("files.open", cx);
    let (notes, notes_model) = document_at(&documents, "mem://product/notes.txt", cx).unwrap();
    let (lib, _) = document_at(&documents, "mem://product/src/lib.txt", cx).unwrap();
    let identity = cx.read(|cx| documents.read(cx).get(notes).unwrap().identity());

    run("files.rename", cx);

    assert_eq!(
        document_at(&documents, "mem://product/docs/renamed.md", cx).map(|found| found.0),
        Some(notes)
    );
    assert_eq!(
        document_at(&documents, "mem://product/lib/lib.txt", cx).map(|found| found.0),
        Some(lib)
    );
    assert!(document_at(&documents, "mem://product/notes.txt", cx).is_none());
    cx.read(|cx| {
        let documents = documents.read(cx);
        let document = documents.get(notes).unwrap();
        assert_eq!(document.title(), "renamed.md");
        assert_eq!(document.identity(), identity);
        assert_eq!(document.model(), &notes_model);
        assert!(document.is_dirty(cx));
        assert_eq!(notes_model.read(cx).text(), "edited loaded text");
        assert_eq!(documents.get(lib).unwrap().title(), "lib.txt");
        assert!(!documents.get(lib).unwrap().is_dirty(cx));
    });

    // Saving writes the new path, and Undo still reverts the edit.
    run("file.save", cx);
    assert_eq!(
        pollster::block_on(
            provider.read(ResourceUri::parse("mem://product/docs/renamed.md").unwrap())
        )
        .unwrap()
        .bytes,
        b"edited loaded text"
    );
    assert!(
        pollster::block_on(provider.stat(ResourceUri::parse("mem://product/notes.txt").unwrap()))
            .unwrap()
            == ResourceStat::Missing
    );
    cx.update(|cx| {
        notes_model.update(cx, |model, _| model.undo().unwrap());
    });
    assert_eq!(cx.read(|cx| notes_model.read(cx).text()), "loaded text");
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
}

#[gpui::test]
fn deleting_a_file_marks_its_open_documents_deleted_and_keeps_their_text(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (filesystems, provider) = memory_filesystems_with_provider();
    provider
        .seed_file(
            ResourceUri::parse("mem://product/src/lib.txt").unwrap(),
            "lib",
        )
        .unwrap();
    cx.set_global(ApplicationFileSystems(filesystems));
    let host = cx.update(extensions::host::install);
    let key = install_operation_fixture(
        &host,
        r#"
import { commands, documents, files, windows } from "knot";
await commands.register("files.delete", async () => {
  const [window] = await windows.list();
  const clean = await documents.open("mem://product/notes.txt", { target: window.target });
  const dirty = await documents.open("mem://product/src/lib.txt", { target: window.target });
  await dirty.document.buffer.applyEdits([{ range: { startByteOffset: 0, endByteOffset: 0 }, text: "kept " }], { ifRevision: 0 });
  await files.delete("mem://product/notes.txt");
  await files.delete("mem://product/src");
});
"#,
        "files.delete",
        cx,
    );
    let (shell, window) = empty_product_window(&documents, cx);
    assert_eq!(
        wait_for_command(
            dispatch_product_command(&shell, window, "files.delete", cx),
            cx
        ),
        CommandOutcome::Completed
    );
    for (uri, text) in [
        ("mem://product/notes.txt", "loaded text"),
        ("mem://product/src/lib.txt", "kept lib"),
    ] {
        let (id, model) = document_at(&documents, uri, cx).unwrap();
        cx.read(|cx| {
            let documents = documents.read(cx);
            let document = documents.get(id).unwrap();
            assert!(matches!(
                document.state(),
                DocumentState::Destination { .. }
            ));
            assert_eq!(document.external_change(), Some(&ExternalChange::Deleted));
            assert!(document.is_dirty(cx));
            assert_eq!(model.read(cx).text(), text);
        });
    }
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
}
