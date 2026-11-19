//! Paste requests and the application of a provider's answer.

use super::*;

const URL: &str = "https://example.com";

fn markdown(text: &str) -> Fixture {
    let language = Language {
        injections: "(fenced_code_block (info_string (language) @injection.language) \
                     (code_fence_content) @injection.content)",
        ..Default::default()
    }
    .compile("markdown");
    let rust = Language::default().compile("rust");
    let resolver: LanguageResolver = Arc::new(move |id| (id == "rust").then(|| rust.clone()));
    let mut buffer = BufferModel::from_text(text);
    set_syntax(&mut buffer, language, resolver);
    Fixture::over(buffer, EditorSettings::default())
}

#[test]
fn the_request_describes_the_selection_and_its_language() {
    let mut f = markdown("see word\n\n```rust\nlet x;\n```\n");
    f.place((0, 4), (0, 8));
    let request = f.editor.paste_request(URL, None, &f.buffer).unwrap();
    assert_eq!(request.language, "markdown");
    assert_eq!(request.range, 4..8);
    assert_eq!(request.selected_text, "word");
    assert_eq!(request.revision, f.buffer.revision());
    let argument: serde_json::Value = serde_json::from_str(&request.argument("doc")).unwrap();
    assert_eq!(
        argument,
        serde_json::json!({
            "document": "doc", "revision": request.revision, "text": URL,
            "range": [4, 8], "selectedText": "word",
        })
    );

    // A caret in a code fence reports the fence's language.
    f.place((3, 1), (3, 1));
    let request = f.editor.paste_request(URL, None, &f.buffer).unwrap();
    assert_eq!(request.language, "rust");
    assert_eq!(request.selected_text, "");

    // An explicit range replaces the selection.
    let request = f.editor.paste_request(URL, Some(0..3), &f.buffer).unwrap();
    assert_eq!(
        (request.range, request.selected_text.as_str()),
        (0..3, "see")
    );
}

#[test]
fn a_buffer_without_syntax_has_no_request() {
    let f = Fixture::new("text");
    assert_eq!(f.editor.paste_request(URL, None, &f.buffer), None);
}

#[test]
fn answers_decode_to_text_or_nothing() {
    let answer = |result: &str| PasteRequest::answer(Ok(result.into()));
    assert_eq!(answer("\"[a](b)\""), Some("[a](b)".into()));
    assert_eq!(answer("null"), None);
    assert_eq!(answer("42"), None);
    assert_eq!(answer("not json"), None);
    assert_eq!(PasteRequest::answer(Err("boom".into())), None);
}

#[test]
fn an_answer_pastes_as_one_undo_step() {
    let mut f = markdown("see word");
    f.place((0, 4), (0, 8));
    let request = f.editor.paste_request(URL, None, &f.buffer).unwrap();
    f.editor
        .paste_answered(&request, Some(format!("[word]({URL})")), &mut f.buffer);
    assert_eq!(f.text(), format!("see [word]({URL})"));
    assert_eq!(f.selection(), ((0, 31), (0, 31)));
    f.run("editor.undo");
    assert_eq!(f.text(), "see word");
    assert_eq!(f.selection(), ((0, 4), (0, 8)));
}

#[test]
fn no_answer_pastes_the_text_unchanged() {
    let mut f = markdown("see word");
    f.place((0, 4), (0, 8));
    let request = f.editor.paste_request(URL, None, &f.buffer).unwrap();
    f.editor.paste_answered(&request, None, &mut f.buffer);
    assert_eq!(f.text(), format!("see {URL}"));
}

#[test]
fn a_stale_answer_pastes_the_text_at_the_current_selection() {
    let mut f = markdown("see word");
    f.place((0, 4), (0, 8));
    let request = f.editor.paste_request(URL, None, &f.buffer).unwrap();

    // The selection moved.
    f.place((0, 0), (0, 3));
    f.editor
        .paste_answered(&request, Some("answer".into()), &mut f.buffer);
    assert_eq!(f.text(), format!("{URL} word"));

    // The text changed.
    let mut f = markdown("see word");
    f.place((0, 4), (0, 8));
    let request = f.editor.paste_request(URL, None, &f.buffer).unwrap();
    f.buffer.replace(0..1, "S").unwrap();
    f.editor
        .paste_answered(&request, Some("answer".into()), &mut f.buffer);
    assert_eq!(f.text(), format!("See {URL}"));
}
