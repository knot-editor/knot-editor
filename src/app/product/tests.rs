//! Product window integration tests and their shared fixtures.

mod closure;
mod commands;
mod document_watch;
mod editing;
mod editor_api;
mod editor_bars;
mod extension_api;
mod extension_restart;
mod files;
mod headless;
mod icons;
mod key_hold;
mod keybindings;
mod languages;
mod navigation;
mod observation;
mod operations;
mod persistence;
mod previews;
mod recovery;
mod reopen;
mod save_hygiene;
mod sessions;
mod settings;
mod startup;
mod status_bar;
mod stripes;
mod terminal;
mod text_frames;
mod themes;
mod tool_layout;
mod tools;
mod transient;
mod tree_context_menu;
mod window_restore;
mod workbench;
mod workspaces;
mod zoom;
use std::path::Path;
use std::sync::{Arc, Mutex};

use gpui::{
    AppContext, Focusable, Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px,
};

use super::super::commands::keymaps::{ApplicationKeymaps, BindingOwner, BindingResolution};
use knot_protocol::{
    Command, CommandArgumentValue, CommandOutcome, ExtensionId, ExtensionLifecycleId,
};

pub(super) use super::driver::setup::{
    install_globals, install_launch_gate, product_window, wait_for_product_startup,
};
use super::{
    ApplicationDocuments, ApplicationFileSystems, ApplicationOpenDialog,
    ApplicationProductCommands, ApplicationProtectedClosure, ApplicationSaveDialog,
    ApplicationTerminalSessions, ApplicationWindows, Buffer, BufferModel, CLOSE_TAB_COMMAND,
    CLOSE_WINDOW_COMMAND, DocumentCollection, Entity, KnotWindow, LaunchGate, NEW_COMMAND,
    NEW_TERMINAL_COMMAND, OpenDialogFuture, OpenDialogOutcome, OpenRequest, OpenTarget,
    ProductCommandSource, ProductOpenDialog, ProductSaveDialog, QUIT_COMMAND, REDO_COMMAND,
    RefCell, SAVE_AS_COMMAND, SAVE_COMMAND, SHOW_EXTENSION_REPORT_COMMAND,
    SPLIT_HORIZONTAL_COMMAND, SaveDialogFuture, SaveDialogOutcome, SplitDirection, UNDO_COMMAND,
    Workbench, WorkbenchLayout, create_untitled_document, install_protected_window_close,
    open_tab_transfer_window, product_filesystems,
};
use crate::app::commands::dispatch::{NEW_TERMINAL_IN_CENTER_COMMAND, TOGGLE_TERMINAL_COMMAND};
use crate::app::{
    documents::DocumentState,
    filesystem::{
        FileSystemProvider, FileSystemProviderRegistry, MemoryFileSystemProvider, ProviderFuture,
        ResourceEntry, ResourceFile, ResourceStat, ResourceVersion,
    },
    resource::ResourceUri,
    workbench::TabSurfaceId,
};

struct GatedFileSystemProvider {
    inner: Arc<dyn FileSystemProvider>,
    read_release: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
    replace_release: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
}

impl FileSystemProvider for GatedFileSystemProvider {
    fn normalize(&self, uri: ResourceUri) -> ProviderFuture<'_, ResourceUri> {
        self.inner.normalize(uri)
    }

    fn enumerate(&self, uri: ResourceUri) -> ProviderFuture<'_, Vec<ResourceEntry>> {
        self.inner.enumerate(uri)
    }

    fn read(&self, uri: ResourceUri) -> ProviderFuture<'_, ResourceFile> {
        let release = self.read_release.lock().unwrap().take();
        Box::pin(async move {
            if let Some(release) = release {
                let _ = release.await;
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
        let release = self.replace_release.lock().unwrap().take();
        Box::pin(async move {
            if let Some(release) = release {
                let _ = release.await;
            }
            self.inner.replace(uri, expected, bytes).await
        })
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

/// Focus the surface owning `handle` in the window's session, as a click
/// in the view would.
fn focus_view(handle: &gpui::FocusHandle, window: &mut gpui::Window, cx: &mut gpui::App) {
    let window = window.root::<KnotWindow>().flatten().expect("a window");
    window.update(cx, |window, cx| window.focus_view(handle, cx));
}

/// Settings under which a save does not touch the text.
pub(super) fn save_without_cleanup() -> crate::app::settings::ApplicationSettings {
    use crate::app::settings::{
        ApplicationSettings, FILES_INSERT_FINAL_NEWLINE, FILES_TRIM_TRAILING_WHITESPACE,
        SettingsScope,
    };
    let mut settings = ApplicationSettings::default();
    settings
        .0
        .set(
            FILES_TRIM_TRAILING_WHITESPACE,
            "off".into(),
            SettingsScope::default(),
        )
        .unwrap();
    settings
        .0
        .set(
            FILES_INSERT_FINAL_NEWLINE,
            false.into(),
            SettingsScope::default(),
        )
        .unwrap();
    settings
}

/// Replace recovery with a store that cannot write, so quit falls back to
/// asking about each dirty document.
pub(super) fn install_failing_recovery(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "not a directory").unwrap();
    cx.set_global(crate::app::recovery::ApplicationRecovery(
        crate::app::recovery::RecoveryService::new(crate::app::recovery::RecoveryStore::open(
            blocker.join("recovery.sqlite3"),
            cx.background_executor.clone(),
        )),
    ));
    cx.set_global(TestRecoveryDir(dir));
}

/// Replace recovery with a store in a file of its own, under
/// `TestRecoveryDir` as `recovery.sqlite3`, for a test that reopens it as a
/// restart would.
pub(super) fn install_file_recovery(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    cx.set_global(crate::app::recovery::ApplicationRecovery(
        crate::app::recovery::RecoveryService::new(crate::app::recovery::RecoveryStore::open(
            dir.path().join("recovery.sqlite3"),
            cx.background_executor.clone(),
        )),
    ));
    cx.set_global(TestRecoveryDir(dir));
}

#[allow(dead_code, reason = "ownership keeps the test database path alive")]
struct TestRecoveryDir(tempfile::TempDir);

impl gpui::Global for TestRecoveryDir {}

fn keymap_command(cx: &gpui::App, key: &str, view: Option<&str>) -> Option<String> {
    match cx.global::<ApplicationKeymaps>().0.resolve(
        &crate::app::keys::Keystroke::parse(key).unwrap().unparse(),
        view,
    ) {
        Some(BindingResolution::Command(command)) => Some(command.name.to_string()),
        Some(BindingResolution::Unbound) => Some("<unbound>".into()),
        None => None,
    }
}

fn two_dirty_untitled_documents_in_one_window(
    documents: &Entity<DocumentCollection>,
    cx: &mut TestAppContext,
) -> (
    super::DocumentId,
    super::DocumentId,
    Entity<KnotWindow>,
    gpui::AnyWindowHandle,
) {
    let first = cx.update(|cx| create_untitled_document(documents, cx));
    let first_model = cx.read(|cx| documents.read(cx).get(first).unwrap().model().clone());
    first_model.update(cx, |model, _| model.replace(0..0, "first").unwrap());
    let second_model = cx.new(|_| Buffer::new(BufferModel::from_text("second")));
    let second = documents.update(cx, |documents, cx| {
        documents.create_untitled("Second", second_model.clone(), cx)
    });
    second_model.update(cx, |model, _| model.replace(0..0, "changed ").unwrap());
    let (window, native_window) = product_window(first, first_model, cx);
    window.update(cx, |window, cx| {
        let pane = window.workbench.read(cx).focused_pane_id().unwrap();
        window.workbench.update(cx, |workbench, cx| {
            workbench
                .open_tab_for_document(pane, second, second_model, cx)
                .unwrap();
        });
    });
    (first, second, window, native_window)
}

fn memory_filesystems() -> Arc<FileSystemProviderRegistry> {
    memory_filesystems_with_provider().0
}

fn memory_filesystems_with_provider() -> (
    Arc<FileSystemProviderRegistry>,
    Arc<MemoryFileSystemProvider>,
) {
    let root = ResourceUri::parse("mem://product/").unwrap();
    let provider = Arc::new(MemoryFileSystemProvider::new(root).unwrap());
    provider
        .seed_file(
            ResourceUri::parse("mem://product/notes.txt").unwrap(),
            b"loaded text",
        )
        .unwrap();
    provider
        .seed_file(
            ResourceUri::parse("mem://product/invalid.txt").unwrap(),
            [0xff],
        )
        .unwrap();
    provider
        .seed_directory(ResourceUri::parse("mem://product/src").unwrap())
        .unwrap();
    let mut providers = FileSystemProviderRegistry::default();
    providers.register("mem", provider.clone()).unwrap();
    (Arc::new(providers), provider)
}

fn current_open_target(window: &Entity<KnotWindow>, cx: &gpui::App) -> OpenTarget {
    let session = window.downgrade();
    let window = window.read(cx);
    let workbench = window.workbench.read(cx);
    let pane = workbench.focused_pane().unwrap();
    let tab = pane.active_tab();
    OpenTarget {
        window: session,
        workbench: window.workbench.downgrade(),
        tab: Some(crate::app::commands::dispatch::TargetTab {
            pane: pane.id(),
            tab: tab.id(),
            surface: tab.surface_id(),
            presentation: tab.presentation(),
        }),
    }
}

fn place_explorer(
    window: &Entity<KnotWindow>,
    window_handle: gpui::AnyWindowHandle,
    dock: crate::app::workbench::DockLocation,
    cx: &mut TestAppContext,
) {
    cx.update_window(window_handle, |_, _, cx| {
        window.update(cx, |window, cx| {
            window.ensure_test_tool(false, cx);
            let id = window.test_tool_frame(cx).unwrap();
            let (pane, tab) = window.workbench.read(cx).tool_tab(id).unwrap();
            let area = crate::app::workbench::Area::Dock(dock);
            if window.workbench.read(cx).pane_area(pane) != Some(area) {
                window.move_tab_to_area(pane, tab, area, cx).unwrap();
            }
        })
    })
    .unwrap();
}

fn dispatch_product_command(
    window: &Entity<KnotWindow>,
    window_handle: gpui::AnyWindowHandle,
    name: &'static str,
    cx: &mut TestAppContext,
) -> super::super::CommandExecution {
    dispatch_product_command_with(window, window_handle, name, CommandArgumentValue::Null, cx)
}

fn dispatch_product_command_with(
    window: &Entity<KnotWindow>,
    window_handle: gpui::AnyWindowHandle,
    name: &'static str,
    arguments: CommandArgumentValue,
    cx: &mut TestAppContext,
) -> super::super::CommandExecution {
    let target = cx
        .update_window(window_handle, |_, _, cx| {
            window.update(cx, |window, cx| {
                window.focus_active_surface(cx);
                window.capture_command_target(cx)
            })
        })
        .unwrap();
    cx.update(|cx| {
        let dispatcher = cx.global::<ApplicationProductCommands>().0.clone();
        dispatcher.update(cx, |dispatcher, cx| {
            dispatcher.dispatch(
                Command {
                    name: name.into(),
                    arguments,
                },
                target,
                cx,
            )
        })
    })
}

fn install_extension(root: &Path, name: &str, requires: &[&str], files: &[(&str, &str)]) {
    let directory = root.join(name);
    std::fs::create_dir_all(&directory).unwrap();
    let main = files[0].0;
    std::fs::write(
        directory.join("knot.jsonc"),
        serde_json::json!({
            "name": name,
            "apiVersion": 1, "version": "1.0.0",
            "main": main,
            "requires": requires,
        })
        .to_string(),
    )
    .unwrap();
    for (path, source) in files {
        let destination = directory.join(path);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(destination, source).unwrap();
    }
}

fn wait_for_extension_report(
    _host: &Entity<super::super::extensions::host::ProductExtensionHost>,
    cx: &mut TestAppContext,
) -> super::super::extensions::host::StartupReportSnapshot {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if let Some(report) = cx.read(super::super::extensions::host::startup_report)
            && report.state == super::super::extensions::host::StartupState::Complete
        {
            return report;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "extension startup did not finish"
        );
        std::thread::yield_now();
    }
}

struct FixedOpenDialog(OpenDialogOutcome);

impl ProductOpenDialog for FixedOpenDialog {
    fn select(&self) -> OpenDialogFuture {
        let outcome = self.0.clone();
        Box::pin(async move { outcome })
    }
}

struct FixedSaveDialog(SaveDialogOutcome);

impl ProductSaveDialog for FixedSaveDialog {
    fn select(&self, _: String) -> SaveDialogFuture {
        let outcome = self.0.clone();
        Box::pin(async move { outcome })
    }
}

fn wait_for_command(
    mut execution: crate::app::CommandExecution,
    cx: &mut TestAppContext,
) -> CommandOutcome {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        match execution.completion.try_recv() {
            Ok(outcome) => return outcome,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "command did not settle; pending prompt: {:?}",
                    cx.pending_prompt()
                );
                std::thread::yield_now();
            }
            Err(error) => panic!("command completion closed: {error}"),
        }
    }
}

fn wait_for_contributed_view(
    window: &Entity<KnotWindow>,
    id: &str,
    cx: &mut TestAppContext,
) -> Entity<crate::app::tree_view::TreeView> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if let Some(view) = cx.read(|cx| {
            window
                .read(cx)
                .contributed_views
                .get(id)
                .map(|view| view.tree().clone())
        }) {
            return view;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "contributed view did not attach"
        );
        std::thread::yield_now();
    }
}

fn dispatch_tree_command(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    name: &'static str,
    cx: &mut TestAppContext,
) -> crate::app::CommandExecution {
    let target = cx
        .update_window(native_window, |_, _, cx| {
            window.update(cx, |window, cx| window.capture_command_target(cx))
        })
        .unwrap();
    let dispatcher = cx.read(|cx| cx.global::<ApplicationProductCommands>().0.clone());
    dispatcher.update(cx, |dispatcher, cx| {
        dispatcher.dispatch(
            Command {
                name: name.into(),
                arguments: CommandArgumentValue::Null,
            },
            target,
            cx,
        )
    })
}

fn dispatch_tree_copy(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    cx: &mut TestAppContext,
) -> crate::app::CommandExecution {
    dispatch_tree_command(window, native_window, super::COPY_COMMAND, cx)
}

fn wait_for_api_condition(cx: &mut TestAppContext, mut ready: impl FnMut(&gpui::App) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if cx.read(&mut ready) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "public API fixture did not settle"
        );
        std::thread::yield_now();
    }
}

/// Toggle the left test tool as the explorer's toggle command did, registering
/// it first.
fn toggle_test_tool(window: &Entity<KnotWindow>, cx: &mut TestAppContext) {
    window.update(cx, |window, cx| {
        window.ensure_test_tool(false, cx);
        window.toggle_tool(crate::app::product::tools::TEST_TOOL_ID, cx);
    });
    cx.run_until_parked();
}
