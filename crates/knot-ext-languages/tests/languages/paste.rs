//! The Markdown package's paste provider, through the real `editor.paste`
//! command: a URL pasted over selected text becomes a link in one undo step.
use gpui::{ClipboardItem, TestAppContext};
use knot::test_support::TestProduct;

const URL: &str = "https://example.com/a";

/// Paste `clipboard` in `language` over `before`, with the selection its
/// `‸` markers give, and check the text and selection it leaves, then that
/// one undo restores `before`.
fn check(
    product: &TestProduct,
    language: Option<&str>,
    before: &str,
    clipboard: &str,
    after: &str,
    cx: &mut TestAppContext,
) {
    let (text, selection) = knot::test_support::cases::unmark(&[before.to_string()]);
    let editor = product.editor(&text, language, selection.unwrap(), cx);
    cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_string(clipboard.into())));
    editor.run_command("editor.paste", cx);
    assert_eq!(editor.marked(cx), after, "{before:?} pasting {clipboard:?}");
    editor.run_command("editor.undo", cx);
    assert_eq!(
        editor.marked(cx),
        before,
        "undo of {before:?} pasting {clipboard:?}"
    );
}

pub fn check_all(product: &TestProduct, cx: &mut TestAppContext) {
    let md = Some("markdown");
    check(
        product,
        md,
        "see ‸word‸ here",
        URL,
        &format!("see [word]({URL})‸ here"),
        cx,
    );
    check(
        product,
        md,
        "# ‸Title‸",
        URL,
        &format!("# [Title]({URL})‸"),
        cx,
    );
    // Inserted as is.
    check(
        product,
        md,
        "see ‸ here",
        URL,
        &format!("see {URL}‸ here"),
        cx,
    );
    check(
        product,
        md,
        "see ‸http://old.example‸",
        URL,
        &format!("see {URL}‸"),
        cx,
    );
    check(
        product,
        md,
        "see ‸word‸ here",
        "plain",
        "see plain‸ here",
        cx,
    );
    check(
        product,
        md,
        "```rust\nlet ‸x‸;\n```",
        URL,
        &format!("```rust\nlet {URL}‸;\n```"),
        cx,
    );
    check(
        product,
        None,
        "see ‸word‸ here",
        URL,
        &format!("see {URL}‸ here"),
        cx,
    );
}
