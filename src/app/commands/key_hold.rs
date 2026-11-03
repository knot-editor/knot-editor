//! Input that arrives while a key-invoked extension command runs, kept in
//! arrival order so it can be delivered once the command has committed.

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use gpui::Keystroke;

pub(crate) enum HeldInput {
    Key(Keystroke),
    /// Committed text input, outside any key event the window intercepted.
    Text(String),
}

/// One queue shared by the window, which holds key-downs, and the held
/// editor, which holds its committed text input.
#[derive(Clone, Default)]
pub(crate) struct HeldInputQueue(Rc<RefCell<VecDeque<HeldInput>>>);

impl HeldInputQueue {
    pub(crate) fn push(&self, input: HeldInput) {
        self.0.borrow_mut().push_back(input);
    }

    pub(crate) fn take(&self) -> VecDeque<HeldInput> {
        std::mem::take(&mut self.0.borrow_mut())
    }

    /// Put `inputs` ahead of anything queued since, keeping their order.
    pub(crate) fn prepend(&self, mut inputs: VecDeque<HeldInput>) {
        let mut queue = self.0.borrow_mut();
        inputs.append(&mut queue);
        *queue = inputs;
    }
}

/// Routes text an editor's text binding claims to its command. The product
/// shell installs it; returning `true` means the text is held in `queue`
/// behind the command, so the editor must queue later text there too.
pub(crate) struct TextCommandRouter(
    pub(crate)  fn(
        gpui::WeakEntity<crate::app::editor::EditorView>,
        crate::host::protocol::CommandName,
        String,
        HeldInputQueue,
        &mut gpui::Window,
        &mut gpui::App,
    ) -> bool,
);

impl gpui::Global for TextCommandRouter {}
