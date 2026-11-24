//! Buffers that packages create: ownership, lifecycle, and persisted text.

use super::storage_tests::{finish, host};
use super::*;
use gpui::TestAppContext;
use knot_protocol::{ExtensionId, ExtensionLifecycleId};
use std::path::Path;

const PACKAGE: &str = "@example/buffers";

/// Lifecycles of one extension are sequential; a lifecycle from 10 on belongs
/// to another extension that runs beside it.
fn key(lifecycle: u64) -> ExtensionKey {
    ExtensionKey::new(
        ExtensionId::new(930 + lifecycle / 10),
        ExtensionLifecycleId::new(lifecycle),
    )
}

/// Load `source` as a lifecycle of the package and wait for its activation.
fn run(host: &Entity<ProductExtensionHost>, lifecycle: u64, source: &str, cx: &mut TestAppContext) {
    let execution = host.update(cx, |host, _| {
        host.load_test_source_with_storage(key(lifecycle), source, PACKAGE)
            .unwrap()
    });
    finish(execution, cx).unwrap();
}

fn unload(host: &Entity<ProductExtensionHost>, lifecycle: u64, cx: &mut TestAppContext) {
    host.update(cx, |host, cx| host.unload(key(lifecycle), cx).unwrap());
    cx.run_until_parked();
}

fn stored(data: &Path, record: &str) -> Option<String> {
    let database = data.join("packages/@example/buffers.sqlite3");
    let connection = rusqlite::Connection::open(database).ok()?;
    let json: String = connection
        .query_row(
            "SELECT value FROM records WHERE key = ?1",
            [record],
            |row| row.get(0),
        )
        .ok()?;
    serde_json::from_str(&json).ok()
}

fn live_buffers(host: &Entity<ProductExtensionHost>, cx: &mut TestAppContext) -> usize {
    cx.read(|cx| {
        host.read(cx)
            .extension_buffers
            .values()
            .filter(|state| state.model.read(cx).is_open())
            .count()
    })
}

const PRELUDE: &str = r#"
import { buffers, documents } from 'knot';
const check = (condition, message) => { if (!condition) throw new Error(message); };
const expectCode = async (operation, code) => {
  try { await operation(); } catch (error) { if (error.code === code) return; throw error; }
  throw new Error(`expected ${code}`);
};
const insert = async (buffer, text) => {
  const { revision } = await buffer.snapshot();
  await buffer.applyEdits([{ range: { startByteOffset: 0, endByteOffset: 0 }, text }], { ifRevision: revision });
};
const text = async (buffer) => (await buffer.snapshot()).text;
"#;

fn source(body: &str) -> String {
    format!("{PRELUDE}\n{body}")
}

#[gpui::test]
fn a_buffer_has_text_a_language_and_edits_unless_read_only(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let host = host(root.path(), cx);
    run(
        &host,
        1,
        &source(
            r#"
            const empty = await buffers.create();
            check(await text(empty) === "", "empty by default");
            const editable = await buffers.create({ text: "héllo", language: "rust" });
            check(await text(editable) === "héllo", "initial text");
            await insert(editable, "x");
            check(await text(editable) === "xhéllo", "edited");
            const fixed = await buffers.create({ text: "fixed", readOnly: true });
            await expectCode(() => insert(fixed, "x"), "BufferReadOnly");
            check(await text(fixed) === "fixed", "read-only text kept");
            let rejected = 0;
            for (const options of [{ text: 1 }, { language: 1 }, { readOnly: 1 }, { persist: 1 }, 7,
              { readOnly: true, persist: "key" }]) {
              try { await buffers.create(options); } catch (error) { if (error instanceof TypeError) rejected++; }
            }
            check(rejected === 6, "invalid options");
            check((await documents.list()).length === 0, "no document identity");
            "#,
        ),
        cx,
    );
    cx.read(|cx| {
        let host = host.read(cx);
        let mut languages = host
            .extension_buffers
            .values()
            .map(|state| state.model.clone())
            .map(|model| model.read(cx).language().map(str::to_owned))
            .collect::<Vec<_>>();
        languages.sort();
        assert_eq!(languages, [None, None, Some("rust".to_owned())]);
    });
}

#[gpui::test]
fn disposal_and_unload_close_the_buffers_of_a_lifecycle(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let host = host(root.path(), cx);
    run(
        &host,
        1,
        &source(
            r#"
            const gone = await buffers.create({ text: "a" });
            await gone[Symbol.asyncDispose]();
            await expectCode(() => text(gone), "BufferClosed");
            await expectCode(() => gone.clearPersisted(), "BufferClosed");
            await gone[Symbol.asyncDispose]();
            await buffers.create({ text: "kept" });
            await buffers.create({ text: "also kept" });
            "#,
        ),
        cx,
    );
    assert_eq!(live_buffers(&host, cx), 2);
    // Another extension's buffers are independent of the first's.
    run(
        &host,
        10,
        &source(r#"await buffers.create({ text: "other" });"#),
        cx,
    );
    assert_eq!(live_buffers(&host, cx), 3);
    unload(&host, 1, cx);
    assert_eq!(live_buffers(&host, cx), 1);
    unload(&host, 10, cx);
    assert_eq!(live_buffers(&host, cx), 0);
    cx.read(|cx| assert!(host.read(cx).extension_buffers.is_empty()));
}

#[gpui::test]
fn persisted_text_is_written_after_edits_and_starts_a_later_buffer(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().to_owned();
    let host = host(&data, cx);
    run(
        &host,
        1,
        &source(
            r#"
            const buffer = await buffers.create({ text: "draft", persist: "message" });
            check(await text(buffer) === "draft", "no stored text yet");
            await expectCode(() => buffers.create({ persist: "message" }), "PersistKeyInUse");
            await insert(buffer, "a");
            "#,
        ),
        cx,
    );
    assert_eq!(stored(&data, "knot.buffer:message"), None, "debounced");
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(600));
    cx.run_until_parked();
    wait_for_record(&data, "knot.buffer:message", Some("adraft"), cx);
    unload(&host, 1, cx);
    // Disposal and unload keep the text; a new buffer starts from it.
    run(
        &host,
        2,
        &source(
            r#"
            const buffer = await buffers.create({ text: "ignored", persist: "message" });
            check(await text(buffer) === "adraft", "starts from the stored text");
            const other = await buffers.create({ text: "own", persist: "other" });
            check(await text(other) === "own", "keys are independent");
            await insert(buffer, "b");
            await buffer[Symbol.asyncDispose]();
            const again = await buffers.create({ persist: "message" });
            check(await text(again) === "badraft", "disposal flushed the edit");
            await insert(again, "c");
            "#,
        ),
        cx,
    );
    // Unload inside the debounce window still writes.
    unload(&host, 2, cx);
    wait_for_record(&data, "knot.buffer:message", Some("cbadraft"), cx);
    run(
        &host,
        3,
        &source(
            r#"
            const buffer = await buffers.create({ text: "fresh", persist: "message" });
            check(await text(buffer) === "cbadraft", "teardown flushed the edit");
            await buffer.clearPersisted();
            check(await text(buffer) === "cbadraft", "clearing keeps the text");
            "#,
        ),
        cx,
    );
    wait_for_record(&data, "knot.buffer:message", None, cx);
    unload(&host, 3, cx);
    run(
        &host,
        4,
        &source(
            r#"
            const buffer = await buffers.create({ text: "fresh", persist: "message" });
            check(await text(buffer) === "fresh", "cleared text is gone");
            await insert(buffer, "d");
            await buffer.clearPersisted();
            "#,
        ),
        cx,
    );
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(600));
    cx.run_until_parked();
    assert_eq!(
        stored(&data, "knot.buffer:message"),
        None,
        "clearing cancels the pending write"
    );
}

#[gpui::test]
fn persisted_text_is_flushed_at_shutdown(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().to_owned();
    let host = host(&data, cx);
    run(
        &host,
        1,
        &source(
            r#"
            const buffer = await buffers.create({ persist: "message" });
            await insert(buffer, "last words");
            "#,
        ),
        cx,
    );
    assert_eq!(stored(&data, "knot.buffer:message"), None);
    let drain = host.update(cx, |host, cx| {
        host.flush_persisted_buffers(cx);
        host.shutdown_storage().unwrap()
    });
    finish(drain, cx);
    assert_eq!(
        stored(&data, "knot.buffer:message").as_deref(),
        Some("last words")
    );
}

#[gpui::test]
fn emptying_a_persisted_buffer_stores_nothing(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().to_owned();
    let host = host(&data, cx);
    run(
        &host,
        1,
        &source(
            r#"
            const buffer = await buffers.create({ text: "x", persist: "message" });
            await insert(buffer, "y");
            "#,
        ),
        cx,
    );
    unload(&host, 1, cx);
    wait_for_record(&data, "knot.buffer:message", Some("yx"), cx);
    run(
        &host,
        2,
        &source(
            r#"
            const buffer = await buffers.create({ persist: "message" });
            const { revision } = await buffer.snapshot();
            await buffer.applyEdits([{ range: { startByteOffset: 0, endByteOffset: 2 }, text: "" }], { ifRevision: revision });
            "#,
        ),
        cx,
    );
    unload(&host, 2, cx);
    wait_for_record(&data, "knot.buffer:message", None, cx);
}

/// Wait for a write on the storage executor to land.
fn wait_for_record(data: &Path, record: &str, expected: Option<&str>, cx: &mut TestAppContext) {
    crate::test_support::wait_until(cx, |_| stored(data, record).as_deref() == expected);
}
