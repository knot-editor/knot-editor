//! Completion requests, aggregation, and acceptance for one editor.

use super::*;

impl EditorState {
    /// Open a completion at the caret over the word before it, asking
    /// `providers`; returns the requests to send them. `handle` names
    /// `buffer` to the providers.
    pub fn start_completion(
        &mut self,
        handle: BufferHandle,
        providers: Vec<CompletionProviderRegistration>,
        model: &BufferModel,
    ) -> Vec<(CompletionProviderRegistration, CompletionRequest)> {
        self.dismiss_completion();
        if !model.is_editable() {
            return Vec::new();
        }
        let revision = model.revision();
        let line_start = model.line_range(self.cursor_line).start;
        let cursor_byte_offset = line_start + self.cursor_col;
        let line_prefix = model.read(line_start..cursor_byte_offset);
        let prefix_len = line_prefix
            .bytes()
            .rev()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        let prefix_start = cursor_byte_offset - prefix_len;
        let generation = self.next_completion_generation;
        self.next_completion_generation = self
            .next_completion_generation
            .checked_add(1)
            .expect("completion generation space exhausted");
        let controller = CompletionController::new(
            handle,
            revision,
            cursor_byte_offset,
            prefix_start..cursor_byte_offset,
            line_prefix[line_prefix.len() - prefix_len..].into(),
            generation,
            providers,
        );
        let requests = controller.requests();
        self.completion_surface = Some(CompletionSurface::attach(controller.snapshot().clone()));
        self.completion = Some(controller);
        requests
    }

    /// Merge a provider's `response` for `handle` into the open completion,
    /// unless the completion moved on or `buffer` changed since it opened.
    pub fn apply_completion_response(
        &mut self,
        handle: BufferHandle,
        response: CompletionResponse,
        buffer: &BufferModel,
    ) -> bool {
        let revision = buffer.revision();
        let Some(controller) = self.completion.as_mut() else {
            return false;
        };
        if controller.buffer() != handle || controller.revision() != revision {
            return false;
        }
        if controller.apply_response(response) {
            if let Some(surface) = self.completion_surface.as_mut() {
                surface.update(controller.snapshot().clone());
            }
            true
        } else {
            false
        }
    }

    /// Settle provider `registration` as unavailable in the open completion.
    pub fn cancel_completion_provider(&mut self, registration: CompletionProviderRegistrationId) {
        if let Some(controller) = self.completion.as_mut()
            && controller.provider_unavailable(registration)
            && let Some(surface) = self.completion_surface.as_mut()
        {
            surface.update(controller.snapshot().clone());
        }
    }

    pub fn dismiss_completion(&mut self) {
        self.completion_surface = None;
        self.completion = None;
    }

    pub fn move_completion_selection(&mut self, delta: isize) {
        if let Some(surface) = self.completion_surface.as_mut() {
            surface.move_selection(delta);
        }
    }

    pub fn accept_selected_completion(&mut self, buffer: &mut BufferModel) {
        let selected = self
            .completion_surface
            .as_ref()
            .and_then(|surface| surface.selected_item());
        let accepted = selected.and_then(|selected| {
            let controller = self.completion.as_ref()?;
            let item = controller.item(selected)?;
            Some((
                controller.revision(),
                controller.prefix_range(),
                item.insert_text.clone(),
            ))
        });
        self.dismiss_completion();
        let Some((revision, range, insert_text)) = accepted else {
            return;
        };
        let edit = TextEdit {
            range: ByteRange {
                start_byte_offset: range.start,
                end_byte_offset: range.end,
            },
            text: insert_text,
        };
        // A stale revision leaves the text alone.
        let _ = buffer.apply_edits(&[edit], revision);
    }
}
