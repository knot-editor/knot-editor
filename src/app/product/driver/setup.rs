//! The application globals and windows tests start from, shared by the
//! driver and the library's own tests.

use std::cell::RefCell;

use gpui::{AppContext, Entity, TestAppContext};

use super::super::{
    ApplicationDocuments, ApplicationFileSystems, ApplicationLaunchGate,
    ApplicationProductCommands, ApplicationProtectedClosure, ApplicationSessions,
    ApplicationTerminalSessions, BufferModel, DocumentCollection, DocumentId, LaunchGate,
    ProductCommandDispatcher, ProductShell, TerminalSessions, Workbench,
    install_protected_window_close, product_filesystems,
};

/// Install the globals a product session needs, each test with its own
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
    cx.set_global(ApplicationSessions::default());
    cx.update(super::super::key_routing::intercept_keys);
    cx.set_global(ApplicationProtectedClosure(RefCell::new(false)));
    cx.set_global(ApplicationTerminalSessions(RefCell::new(
        TerminalSessions::default(),
    )));
    cx.update(|cx| crate::app::allow_thread_wakeups(cx));
    cx.set_global(ApplicationFileSystems(product_filesystems()));
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
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        let state = cx.read(
            |cx| match &*cx.global::<ApplicationLaunchGate>().0.borrow() {
                LaunchGate::Pending(_) => None,
                LaunchGate::Ready => Some(true),
                LaunchGate::Failed => Some(false),
            },
        );
        if let Some(ready) = state {
            return ready;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "product startup did not finish"
        );
        std::thread::yield_now();
    }
}

/// A product window over `document`, whose text `model` holds.
pub(crate) fn product_window(
    document: DocumentId,
    model: Entity<BufferModel>,
    cx: &mut TestAppContext,
) -> (Entity<ProductShell>, gpui::AnyWindowHandle) {
    let mut window = None;
    let (shell, _) = cx.add_window_view(|native, cx| {
        window = Some(native.window_handle());
        let workbench = cx.new(|cx| Workbench::new_for_document(document, model, cx));
        ProductShell::new(workbench, cx)
    });
    let window = window.unwrap();
    cx.update_window(window, |_, native_window, cx| {
        shell.update(cx, |shell, cx| {
            shell.observe_extension_host(native_window, cx)
        });
        install_protected_window_close(&shell, native_window, cx);
    })
    .unwrap();
    (shell, window)
}
