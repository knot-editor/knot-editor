//! The application globals and windows tests start from, shared by the
//! driver and the library's own tests.

use std::{cell::RefCell, sync::Arc};

use knot_model::filesystem::{FileSystemProviderRegistry, LocalFileSystemProvider};

use gpui::{AppContext, Entity, TestAppContext};

use super::super::{
    ApplicationDocuments, ApplicationFileSystems, ApplicationLaunchGate,
    ApplicationProductCommands, ApplicationProtectedClosure, ApplicationTerminalSessions,
    ApplicationWindows, Buffer, DocumentCollection, DocumentId, KnotWindow, LaunchGate,
    ProductCommandDispatcher, TerminalSessions, Workbench, install_protected_window_close,
};

/// Install the globals a window needs, each test with its own
/// in-memory recovery and state databases.
pub(crate) fn install_globals(cx: &mut TestAppContext) -> Entity<DocumentCollection> {
    cx.update(crate::app::components::init);
    let documents = cx.new(|_| DocumentCollection::new());
    cx.set_global(ApplicationDocuments(documents.clone()));
    cx.set_global(crate::app::recovery::ApplicationRecovery(
        crate::app::recovery::RecoveryService::new(crate::app::recovery::RecoveryStore::in_memory(
            cx.background_executor.clone(),
        )),
    ));
    cx.set_global(ApplicationWindows::default());
    cx.update(super::super::key_routing::intercept_keys);
    cx.set_global(ApplicationProtectedClosure(RefCell::new(false)));
    cx.set_global(ApplicationTerminalSessions(RefCell::new(
        TerminalSessions::default(),
    )));
    cx.update(|cx| crate::app::allow_thread_wakeups(cx));
    let mut filesystems = FileSystemProviderRegistry::default();
    filesystems
        .register(
            "file",
            Arc::new(LocalFileSystemProvider::unrestricted_with_scratch_trash()),
        )
        .expect("the test file provider is unique");
    cx.set_global(ApplicationFileSystems(Arc::new(filesystems)));
    let commands = cx.new(ProductCommandDispatcher::new);
    cx.set_global(ApplicationProductCommands(commands));
    cx.set_global(crate::app::tool_layout::ApplicationToolLayout(
        crate::app::tool_layout::ToolLayoutStore::open(
            &crate::app::record_store::StateDatabase::in_memory(cx.background_executor.clone()),
        ),
    ));
    documents
}

/// Hold launch until product startup finishes, as the application does.
pub(crate) fn install_launch_gate(cx: &mut TestAppContext) {
    cx.set_global(ApplicationLaunchGate(RefCell::new(LaunchGate::Pending(
        Vec::new(),
    ))));
}

/// Wait until product startup finishes; whether it succeeded.
pub(crate) fn wait_for_product_startup(cx: &mut TestAppContext) -> bool {
    let mut outcome = None;
    crate::test_support::wait_until(cx, |cx| {
        outcome = cx.read(
            |cx| match &*cx.global::<ApplicationLaunchGate>().0.borrow() {
                LaunchGate::Pending(_) => None,
                LaunchGate::Ready => Some(true),
                LaunchGate::Failed => Some(false),
            },
        );
        outcome.is_some()
    });
    outcome.unwrap()
}

/// A window over `document`, whose text `model` holds, and its native window.
pub(crate) fn product_window(
    document: DocumentId,
    model: Entity<Buffer>,
    cx: &mut TestAppContext,
) -> (Entity<KnotWindow>, gpui::AnyWindowHandle) {
    let mut native_window = None;
    let (window, _) = cx.add_window_view(|native, cx| {
        native_window = Some(native.window_handle());
        let workbench = cx.new(|cx| Workbench::new_for_document(document, model, cx));
        KnotWindow::new(workbench, cx)
    });
    let native_window = native_window.unwrap();
    cx.update_window(native_window, |_, native_window, cx| {
        window.update(cx, |window, cx| {
            window.observe_extension_host(native_window, cx)
        });
        install_protected_window_close(&window, native_window, cx);
    })
    .unwrap();
    (window, native_window)
}
