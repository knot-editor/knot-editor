//! Input that arrives while a key-invoked extension command runs, kept in
//! arrival order so it can be delivered once the command has committed.

use gpui::Keystroke;

pub(crate) enum HeldInput {
    Key(Keystroke),
    /// Committed text input, outside any key event the window intercepted.
    Text(String),
}

/// Offers text committed into an editor to the session showing it, which
/// holds it behind a running command or sends it to the editor's text
/// binding. The product shell installs it; `true` means the session took
/// the text, so the editor must not insert it.
pub(crate) struct TypedTextRouter(
    pub(crate)  fn(
        &gpui::Entity<crate::app::editor::Editor>,
        &str,
        &mut gpui::Window,
        &mut gpui::App,
    ) -> bool,
);

impl gpui::Global for TypedTextRouter {}
