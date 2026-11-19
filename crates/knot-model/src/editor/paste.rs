//! Language paste providers: the request a provider receives and the
//! revalidated application of its answer.

use super::*;

/// What a language's paste provider is asked: the text to insert over
/// `range`, the selection it replaces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PasteRequest {
    /// The ID of the language at the start of `range`.
    pub language: String,
    pub revision: u64,
    pub range: Range<usize>,
    pub text: String,
    pub selected_text: String,
}

impl PasteRequest {
    /// The JSON argument `providePaste` receives for `document`.
    pub fn argument(&self, document: &str) -> String {
        serde_json::json!({
            "document": document,
            "revision": self.revision,
            "text": self.text,
            "range": [self.range.start, self.range.end],
            "selectedText": self.selected_text,
        })
        .to_string()
    }

    /// The text a provider call's `result` asks to insert: `None` when the
    /// provider keeps `text`, throws, or answers anything but a string.
    pub fn answer(result: Result<String, String>) -> Option<String> {
        serde_json::from_str::<Option<String>>(&result.ok()?).ok()?
    }
}

impl EditorState {
    /// The request for pasting `text` over `range`, default the selection.
    /// `None` when the buffer has no syntax state, and so no language.
    pub fn paste_request(
        &self,
        text: &str,
        range: Option<Range<usize>>,
        buffer: &BufferModel,
    ) -> Option<PasteRequest> {
        let range = range.unwrap_or_else(|| self.implicit_replacement_range(buffer));
        Some(PasteRequest {
            language: buffer.language_id_at(range.start)?,
            revision: buffer.revision(),
            selected_text: buffer.read(range.clone()),
            range,
            text: text.to_owned(),
        })
    }

    /// Paste what a provider answered `request` with, as [`Self::paste`]. An
    /// answer that arrives after the text or the selection changed is
    /// dropped for the request's own text.
    pub fn paste_answered(
        &mut self,
        request: &PasteRequest,
        answer: Option<String>,
        buffer: &mut BufferModel,
    ) {
        let current = buffer.revision() == request.revision
            && self.implicit_replacement_range(buffer) == request.range;
        let text = answer
            .filter(|_| current)
            .unwrap_or_else(|| request.text.clone());
        self.paste(Some(&text), buffer);
    }
}
