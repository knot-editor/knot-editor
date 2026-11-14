//! The product launched with windows and durable state, as the application
//! runs it.
//!
//! A launched product keeps package storage, recovery, and the window record
//! in files of its own, so a relaunch in the same process reads back what
//! the previous run wrote.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gpui::{AppContext, TestAppContext};
use serde_json::Value;

use super::product::TestProduct;
use super::setup::{install_globals, install_launch_gate, wait_for_product_startup};
use super::window::TestWindow;
use crate::app::extensions::storage::{ExtensionStorage, StorageOwner};
use crate::app::filesystem::{
    FileSystemProvider, FileSystemProviderRegistry, LocalFileSystemProvider, ProviderFuture,
    ResourceEntry, ResourceFile, ResourceStat, ResourceVersion,
};
use crate::app::product::{
    ApplicationDocuments, ApplicationFileSystems, ApplicationLaunchGate, ApplicationOpenDialog,
    ApplicationWindows, DocumentCollection, HeadlessWindows, OpenDialogFuture, OpenDialogOutcome,
    ProductOpenDialog, active_window, native_window, windows,
};
use crate::app::record_store::StateDatabase;
use crate::app::recovery::{ApplicationRecovery, RecoveryService, RecoveryStore};
use crate::app::resource::ResourceUri;
use crate::test_support::wait_until;
use knot_model::entry::{LaunchBatch, OpenRequest};

/// The batch `knot` sends for `paths`.
fn launch_batch(paths: &[&Path]) -> LaunchBatch {
    LaunchBatch::new(
        paths
            .iter()
            .map(|path| OpenRequest::from_path(path, Path::new("/")).unwrap())
            .collect(),
    )
}

/// A document's text that recovery retains for a package.
#[derive(Clone, Debug)]
pub struct Retained {
    /// The key the package captured it under.
    pub capture_key: String,
    pub text: String,
}

/// File reads held until [`HeldReads::release`].
pub struct HeldReads(tokio::sync::oneshot::Sender<()>);

impl HeldReads {
    /// Let the held read, and every later one, through.
    pub fn release(self) {
        self.0.send(()).unwrap();
    }
}

impl TestProduct {
    /// Launch the application with `packages`, as the binary does but with
    /// windows that have no native window, and require that each one loaded.
    /// Startup opens its window when nothing is recorded to restore.
    pub fn launch(
        packages: &'static [knot_bundled::BundledSource],
        cx: &mut TestAppContext,
    ) -> Self {
        cx.set_global(HeadlessWindows::default());
        Self::launch_windows(packages, None, cx)
    }

    /// [Launch](Self::launch) with native windows, for tests of painting,
    /// pointer input, typed platform input, and native prompts.
    pub fn launch_windowed(
        packages: &'static [knot_bundled::BundledSource],
        cx: &mut TestAppContext,
    ) -> Self {
        Self::launch_windows(packages, None, cx)
    }

    /// [Launch](Self::launch) as `knot` does with `paths` on its command
    /// line: the request waits for startup and is placed with the restored
    /// windows.
    pub fn launch_opening(
        packages: &'static [knot_bundled::BundledSource],
        paths: &[&Path],
        cx: &mut TestAppContext,
    ) -> Self {
        cx.set_global(HeadlessWindows::default());
        Self::launch_windows(packages, Some(launch_batch(paths)), cx)
    }

    /// Deliver `paths` as a later `knot` invocation does to the running
    /// product, and let the opens finish.
    pub fn open_from_command_line(&self, paths: &[&Path], cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::app::product::launch::dispatch_launch_request(Some(launch_batch(paths)), cx)
        });
        cx.run_until_parked();
    }

    fn launch_windows(
        packages: &'static [knot_bundled::BundledSource],
        batch: Option<LaunchBatch>,
        cx: &mut TestAppContext,
    ) -> Self {
        let product = Self::new_directories();
        install_globals(cx);
        product.install_durable_state(cx);
        cx.update(crate::app::product::launch::bind_product_keys);
        install_launch_gate(cx);
        if let Some(batch) = batch {
            cx.update(|cx| {
                cx.global::<ApplicationLaunchGate>()
                    .0
                    .borrow_mut()
                    .request(Some(batch))
            });
        }
        let host = cx.update(crate::app::extensions::host::install);
        let storage = product.storage_root();
        host.update(cx, |host, cx| {
            host.set_test_storage_root(storage, cx);
            host.set_bundled_packages(packages);
            host.start_product_startup(
                Ok(product.config.path().to_path_buf()),
                Ok(product.extensions.path().to_path_buf()),
                cx,
            )
        });
        assert!(wait_for_product_startup(cx));
        product.require_loaded(cx);
        activate_last_window(cx);
        product
    }

    /// Every window, in the order they opened.
    pub fn windows(&self, cx: &mut TestAppContext) -> Vec<TestWindow> {
        let windows = cx.read(windows);
        windows
            .into_iter()
            .map(|window| TestWindow::of(window, cx))
            .collect()
    }

    /// Whether a window is focused.
    pub fn has_active_window(&self, cx: &mut TestAppContext) -> bool {
        cx.read(active_window).is_some()
    }

    /// The focused window.
    pub fn active_window(&self, cx: &mut TestAppContext) -> TestWindow {
        let window = cx.read(active_window).expect("an active window");
        TestWindow::of(window, cx)
    }

    /// Open a window with an untitled document and no roots.
    pub fn new_window(&self, cx: &mut TestAppContext) -> TestWindow {
        let documents = cx.read(|cx| cx.global::<ApplicationDocuments>().0.clone());
        let document =
            cx.update(|cx| crate::app::product::create_untitled_document(&documents, cx));
        let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
        let window = if cx.has_global::<HeadlessWindows>() {
            super::setup::headless_window(document, model, cx)
        } else {
            super::setup::product_window(document, model, cx).0
        };
        cx.run_until_parked();
        TestWindow::of(window, cx)
    }

    /// Make watches on local files listen to the operating system, for a test
    /// that changes files outside the product and waits for the product to
    /// notice. Starting an operating system watch takes about 100 ms, so
    /// watches report nothing by default.
    pub fn watch_files(&self, cx: &mut TestAppContext) {
        let mut providers = FileSystemProviderRegistry::default();
        providers
            .register(
                "file",
                Arc::new(
                    LocalFileSystemProvider::unrestricted_with_scratch_trash().with_os_events(),
                ),
            )
            .unwrap();
        cx.set_global(ApplicationFileSystems(Arc::new(providers)));
    }

    /// Answer every folder or file dialog with `path`.
    pub fn choose_in_open_dialog(&self, path: &Path, cx: &mut TestAppContext) {
        cx.set_global(ApplicationOpenDialog(Arc::new(ChosenPath(
            path.to_path_buf(),
        ))));
    }

    /// Hold the next local file read until the returned handle releases it.
    pub fn hold_file_reads(&self, cx: &mut TestAppContext) -> HeldReads {
        let (release, held) = tokio::sync::oneshot::channel();
        let mut providers = FileSystemProviderRegistry::default();
        providers
            .register(
                "file",
                Arc::new(HeldReadProvider {
                    inner: Arc::new(LocalFileSystemProvider::unrestricted_with_scratch_trash()),
                    held: Mutex::new(Some(held)),
                }),
            )
            .unwrap();
        cx.set_global(ApplicationFileSystems(Arc::new(providers)));
        HeldReads(release)
    }

    /// Start every package again, as a reload of the installed packages
    /// does, and require that each one loaded.
    pub fn reload_packages(&self, cx: &mut TestAppContext) {
        let root = self.extensions.path().to_path_buf();
        cx.update(|cx| {
            let host = crate::app::extensions::host::entity(cx).unwrap();
            host.update(cx, |host, cx| host.start_installed_extensions(root, cx));
        });
        wait_until(cx, |cx| {
            cx.read(crate::app::extensions::host::startup_report)
                .is_some_and(|report| {
                    report.state == crate::app::extensions::host::StartupState::Complete
                })
        });
        self.require_loaded(cx);
    }

    /// Stop the package `name` without its replacement, as quit stops it.
    pub fn stop_package(&self, name: &str, cx: &mut TestAppContext) {
        cx.update(|cx| {
            let host = crate::app::extensions::host::entity(cx).unwrap();
            host.update(cx, |host, cx| host.unload_package_for_test(name, cx));
        });
        cx.run_until_parked();
    }

    /// The application events packages were sent since the last call, as
    /// the JSON of each change.
    pub fn take_application_events(&self, cx: &mut TestAppContext) -> Vec<Value> {
        cx.update(|cx| {
            let host = crate::app::extensions::host::entity(cx).unwrap();
            host.update(cx, |host, _| host.take_test_application_events())
        })
        .into_iter()
        .map(|event| serde_json::to_value(&event.change).unwrap())
        .collect()
    }

    /// The documents recovery retains for the package `owner`.
    pub fn retained(&self, owner: &str, cx: &mut TestAppContext) -> Vec<Retained> {
        let listing = cx.read(|cx| crate::app::recovery::retained(owner.into(), cx));
        await_task(listing, cx)
            .unwrap()
            .into_iter()
            .map(|snapshot| Retained {
                capture_key: snapshot.capture_key,
                text: snapshot.text,
            })
            .collect()
    }

    /// The record `key` of the bundled package `owner` as stored on disk.
    pub fn stored_record(&self, owner: &str, key: &str) -> Option<Value> {
        let storage = self.open_storage();
        let value = pollster::block_on(
            storage
                .for_owner(StorageOwner::bundled(owner).unwrap())
                .read(key.to_owned()),
        )
        .unwrap();
        pollster::block_on(storage.shutdown());
        value
    }

    /// Store `value` as the record `key` of the bundled package `owner`,
    /// which the package reads once it starts again.
    pub fn write_record(&self, owner: &str, key: &str, value: Value) {
        let storage = self.open_storage();
        pollster::block_on(
            storage
                .for_owner(StorageOwner::bundled(owner).unwrap())
                .write(key.to_owned(), value),
        )
        .unwrap();
        pollster::block_on(storage.shutdown());
    }

    /// How many documents are open.
    pub fn document_count(&self, cx: &mut TestAppContext) -> usize {
        cx.read(|cx| {
            cx.global::<ApplicationDocuments>()
                .0
                .read(cx)
                .documents()
                .count()
        })
    }

    /// Whether a fresh window shows the tool `id`.
    pub fn fresh_window_shows(&self, id: &str, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| {
            cx.global::<crate::app::tool_layout::ApplicationToolLayout>()
                .0
                .current()
                .placement(id)
                .is_some_and(|placement| {
                    matches!(placement, knot_model::workbench::StoredPlacement::Docked(_))
                })
        })
    }

    /// The tool a fresh window selects in its left dock's first pane.
    pub fn fresh_window_left_dock(&self, cx: &mut TestAppContext) -> Option<String> {
        use knot_model::workbench::StoredNode;
        cx.read(|cx| {
            let snapshot = cx
                .global::<crate::app::tool_layout::ApplicationToolLayout>()
                .0
                .current();
            let mut node = snapshot.docks[0].root.clone()?;
            loop {
                match node {
                    StoredNode::Pane { selected, tools } => return tools.get(selected).cloned(),
                    StoredNode::Split { first, .. } => node = *first,
                }
            }
        })
    }

    /// Quit and launch again in this process over the same files, as if
    /// only `windows` had been recorded. The first of them is the one that
    /// was active, and every previous window handle is closed.
    pub fn relaunch_restoring(&self, windows: &[&TestWindow], cx: &mut TestAppContext) {
        let kept: Vec<Vec<String>> = windows.iter().map(|window| window.roots(cx)).collect();
        let drained = cx.read(crate::app::recovery::drain);
        await_task(drained, cx).unwrap();
        cx.update(crate::app::window_record::record);
        let mut record = cx.read(crate::app::window_record::current).unwrap();
        record.windows.retain(|window| kept.contains(&window.roots));
        record.active = Some(0);
        cx.update(|cx| {
            crate::app::window_record::stop_for_test(cx);
            cx.global::<crate::app::window_record::ApplicationWindowRecord>()
                .publish_for_test(record);
        });
        let flushed = cx.read(crate::app::window_record::flush);
        await_task(flushed, cx).unwrap();

        // Quit stops packages before windows close, so none sees the closure.
        for entry in self.load_report(cx) {
            if let Some(name) = entry.name {
                self.stop_package(&name, cx);
            }
        }
        close_all_windows(cx);

        let documents = cx.new(|_| DocumentCollection::new());
        cx.set_global(ApplicationDocuments(documents));
        cx.set_global(ApplicationWindows::default());
        self.install_durable_state(cx);
        cx.update(crate::app::extensions::host::reset_launched_for_test);
        self.reload_packages(cx);
        install_launch_gate(cx);
        cx.update(|cx| crate::app::product::launch::finish_product_startup(Ok(()), cx));
        assert!(wait_for_product_startup(cx));
        activate_last_window(cx);
    }

    fn install_durable_state(&self, cx: &mut TestAppContext) {
        let executor = cx.background_executor.clone();
        cx.set_global(ApplicationRecovery(RecoveryService::new(
            RecoveryStore::open(self.state.path().join("recovery.sqlite3"), executor.clone()),
        )));
        let state = StateDatabase::open(self.state.path().join("state.sqlite3"), executor);
        cx.update(|cx| crate::app::window_record::install(&state, cx));
    }

    fn storage_root(&self) -> PathBuf {
        self.state.path().join("storage")
    }

    fn open_storage(&self) -> ExtensionStorage {
        ExtensionStorage::new(
            self.storage_root(),
            crate::app::extensions::storage::test_executor(),
        )
    }

    fn require_loaded(&self, cx: &mut TestAppContext) {
        let failures: Vec<_> = self
            .load_report(cx)
            .into_iter()
            .filter(|entry| !entry.loaded)
            .collect();
        assert!(failures.is_empty(), "{failures:?}");
    }
}

/// Activate the window opened last, as the platform does with a window it
/// opens at launch.
fn activate_last_window(cx: &mut TestAppContext) {
    let window = cx.read(windows).pop().expect("a window");
    match cx.read(|cx| native_window(window.entity_id(), cx)) {
        Some(native_window) => cx
            .update_window(native_window, |_, native_window, _| {
                native_window.activate_window()
            })
            .unwrap(),
        None => window.update(cx, |window, cx| window.activate(cx)),
    }
    cx.run_until_parked();
}

/// Close every window, native or not.
pub(super) fn close_all_windows(cx: &mut TestAppContext) {
    for window in cx.read(windows) {
        window.update(cx, |window, cx| {
            let _ = window.close_window(cx);
        });
    }
    cx.run_until_parked();
}

/// Run `task` to completion on the test's executors.
fn await_task<T: 'static>(task: gpui::Task<T>, cx: &mut TestAppContext) -> T {
    let result = std::rc::Rc::new(std::cell::RefCell::new(None));
    let sink = result.clone();
    cx.spawn(async move |_| *sink.borrow_mut() = Some(task.await))
        .detach();
    wait_until(cx, |_| result.borrow().is_some());
    result.borrow_mut().take().unwrap()
}

struct ChosenPath(PathBuf);

impl ProductOpenDialog for ChosenPath {
    fn select(&self) -> OpenDialogFuture {
        let outcome = OpenDialogOutcome::Selected(self.0.clone());
        Box::pin(async move { outcome })
    }
}

/// Local files whose first read waits for a release.
struct HeldReadProvider {
    inner: Arc<dyn FileSystemProvider>,
    held: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
}

impl FileSystemProvider for HeldReadProvider {
    fn normalize(&self, uri: ResourceUri) -> ProviderFuture<'_, ResourceUri> {
        self.inner.normalize(uri)
    }

    fn enumerate(&self, uri: ResourceUri) -> ProviderFuture<'_, Vec<ResourceEntry>> {
        self.inner.enumerate(uri)
    }

    fn read(&self, uri: ResourceUri) -> ProviderFuture<'_, ResourceFile> {
        let held = self.held.lock().unwrap().take();
        Box::pin(async move {
            if let Some(held) = held {
                let _ = held.await;
            }
            self.inner.read(uri).await
        })
    }

    fn create(&self, uri: ResourceUri, bytes: Vec<u8>) -> ProviderFuture<'_, ResourceVersion> {
        self.inner.create(uri, bytes)
    }

    fn replace(
        &self,
        uri: ResourceUri,
        expected: ResourceVersion,
        bytes: Vec<u8>,
    ) -> ProviderFuture<'_, ResourceVersion> {
        self.inner.replace(uri, expected, bytes)
    }

    fn stat(&self, uri: ResourceUri) -> ProviderFuture<'_, ResourceStat> {
        self.inner.stat(uri)
    }

    fn create_directory(&self, uri: ResourceUri) -> ProviderFuture<'_, ()> {
        self.inner.create_directory(uri)
    }

    fn rename(&self, from: ResourceUri, to: ResourceUri) -> ProviderFuture<'_, ()> {
        self.inner.rename(from, to)
    }

    fn copy(&self, from: ResourceUri, to: ResourceUri) -> ProviderFuture<'_, ()> {
        self.inner.copy(from, to)
    }

    fn delete(&self, uri: ResourceUri, permanent: bool) -> ProviderFuture<'_, ()> {
        self.inner.delete(uri, permanent)
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::TestProduct;

    #[gpui::test]
    fn a_launch_opens_windows_without_native_ones_unless_asked(cx: &mut TestAppContext) {
        let product = TestProduct::launch(&[], cx);
        assert!(cx.windows().is_empty(), "no native window opened");
        let window = product.active_window(cx);
        assert!(window.is_open(cx));
        let second = product.new_window(cx);
        assert_eq!(product.windows(cx), [window.clone(), second.clone()]);
        assert_eq!(product.active_window(cx), second);
        second.run_command("window.close", cx);
        assert!(!second.is_open(cx));
        assert_eq!(product.active_window(cx), window);
    }

    #[gpui::test]
    fn a_windowed_launch_opens_native_windows(cx: &mut TestAppContext) {
        let product = TestProduct::launch_windowed(&[], cx);
        assert_eq!(cx.windows().len(), 1);
        assert!(product.active_window(cx).is_open(cx));
        assert_eq!(cx.windows().len(), product.windows(cx).len());
    }
}
