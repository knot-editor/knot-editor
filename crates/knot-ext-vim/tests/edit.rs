//! Vim's `:e` lists the files Go to File provides, in the real editor. An
//! open Ex line keeps Vim busy, so keys go through `press_unsettled`.
use gpui::TestAppContext;
use knot::test_support::{TestEditor, TestProduct, wait_until};

const ENABLE: &str = r#"import { settings } from "knot";
await settings.set("vim.enabled", true);
"#;

/// Vim plus the packages providing its lists.
fn packages(with_files: bool) -> &'static [knot_bundled::BundledSource] {
    let mut packages = knot_ext_vim::PACKAGES.to_vec();
    if with_files {
        packages.extend(
            knot_ext_workbench::PACKAGES
                .iter()
                .filter(|package| package.manifest.contains("\"name\": \"@knot/go-to-file\"")),
        );
    }
    Box::leak(packages.into_boxed_slice())
}

fn file_uri(path: &std::path::Path) -> String {
    format!("file://{}", path.display())
}

struct Fixture {
    _directory: tempfile::TempDir,
    directory: std::path::PathBuf,
    editor: TestEditor,
    _product: TestProduct,
}

fn fixture(with_files: bool, cx: &mut TestAppContext) -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join("src/ui")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
    std::fs::write(root.join("src/ui/button.rs"), "struct Button;").unwrap();
    std::fs::write(root.join("README.md"), "# readme").unwrap();
    let product = TestProduct::start_with_post_init(packages(with_files), Some(ENABLE), cx);
    let editor = product.editor("scratch", None, (0, 0), cx);
    editor.set_roots(&[&file_uri(&root)], cx);
    Fixture {
        _directory: directory,
        directory: root,
        editor,
        _product: product,
    }
}

fn opened(fixture: &Fixture, text: &str, cx: &mut TestAppContext) {
    wait_until(cx, |cx| fixture.editor.active_text(cx) == text);
}

#[gpui::test]
fn enter_on_a_listed_file_opens_it(cx: &mut TestAppContext) {
    let fixture = fixture(true, cx);
    fixture.editor.press_unsettled(":e butt<cr>", cx);
    opened(&fixture, "struct Button;", cx);
}

#[gpui::test]
fn tab_puts_the_highlighted_path_in_the_line(cx: &mut TestAppContext) {
    let fixture = fixture(true, cx);
    // The path is then run as typed, resolved like any other.
    fixture.editor.press_unsettled(":e butt<tab><cr>", cx);
    opened(&fixture, "struct Button;", cx);
}

#[gpui::test]
fn bare_e_lists_files_and_enter_opens_the_first(cx: &mut TestAppContext) {
    let fixture = fixture(true, cx);
    fixture.editor.press_unsettled(":e<cr>", cx);
    wait_until(cx, |cx| fixture.editor.active_text(cx) != "scratch");
}

#[gpui::test]
fn with_no_match_enter_opens_the_typed_path(cx: &mut TestAppContext) {
    let fixture = fixture(true, cx);
    let keys = format!(":e {}/fresh.txt<cr>", fixture.directory.display());
    fixture.editor.press_unsettled(&keys, cx);
    opened(&fixture, "", cx);
    assert!(!fixture.directory.join("fresh.txt").exists());
}

#[gpui::test]
fn without_a_files_provider_the_path_still_opens(cx: &mut TestAppContext) {
    let fixture = fixture(false, cx);
    let keys = format!(":e {}/src/main.rs<cr>", fixture.directory.display());
    fixture.editor.press_unsettled(&keys, cx);
    opened(&fixture, "fn main() {}", cx);
}

fn read(fixture: &Fixture, name: &str) -> String {
    std::fs::read_to_string(fixture.directory.join(name)).unwrap()
}

/// Opens `name` of the fixture's directory in the active tab.
fn open_file(fixture: &Fixture, name: &str, text: &str, cx: &mut TestAppContext) {
    let keys = format!(":e {}/{name}<cr>", fixture.directory.display());
    fixture.editor.press_unsettled(&keys, cx);
    opened(fixture, text, cx);
    // Vim follows the opened editor a moment later.
    wait_until(cx, |cx| {
        fixture.editor.active_contexts(cx) == ["vim.normal"]
    });
}

#[gpui::test]
fn e_bang_reverts_the_document_to_its_file_and_undo_takes_it_back(cx: &mut TestAppContext) {
    let fixture = fixture(false, cx);
    open_file(&fixture, "README.md", "# readme", cx);
    fixture.editor.press("A!<esc>", cx);
    assert_eq!(fixture.editor.active_text(cx), "# readme!");
    assert!(fixture.editor.active_is_dirty(cx));
    fixture.editor.press_unsettled(":e!<cr>", cx);
    opened(&fixture, "# readme", cx);
    assert!(!fixture.editor.active_is_dirty(cx));
    wait_until(cx, |cx| {
        fixture.editor.active_contexts(cx) == ["vim.normal"]
    });
    fixture.editor.press("u", cx);
    assert_eq!(fixture.editor.active_text(cx), "# readme!");
}

#[gpui::test]
fn e_bang_with_a_path_discards_the_changes_and_opens_the_file(cx: &mut TestAppContext) {
    let fixture = fixture(false, cx);
    open_file(&fixture, "README.md", "# readme", cx);
    fixture.editor.press("A!<esc>", cx);
    let keys = format!(":e! {}/src/main.rs<cr>", fixture.directory.display());
    fixture.editor.press_unsettled(&keys, cx);
    opened(&fixture, "fn main() {}", cx);
    assert_eq!(read(&fixture, "README.md"), "# readme");
    wait_until(cx, |cx| {
        !fixture
            .editor
            .tab_titles(cx)
            .contains(&"README.md".to_owned())
    });
}

#[gpui::test]
fn w_with_a_path_writes_a_copy_and_keeps_the_document(cx: &mut TestAppContext) {
    let fixture = fixture(false, cx);
    open_file(&fixture, "README.md", "# readme", cx);
    fixture.editor.press("A!<esc>", cx);
    fixture.editor.press_unsettled(":w copy.md<cr>", cx);
    wait_until(cx, |_| fixture.directory.join("copy.md").exists());
    assert_eq!(read(&fixture, "copy.md"), "# readme!");
    assert_eq!(read(&fixture, "README.md"), "# readme");
    assert_eq!(fixture.editor.active_title(cx), "README.md");
    assert!(fixture.editor.active_is_dirty(cx));
}

#[gpui::test]
fn an_existing_target_needs_a_bang(cx: &mut TestAppContext) {
    let fixture = fixture(false, cx);
    open_file(&fixture, "README.md", "# readme", cx);
    fixture.editor.press("A!<esc>", cx);
    fixture.editor.press_unsettled(":w src/main.rs<cr>", cx);
    wait_until(cx, |cx| {
        fixture
            .editor
            .status(cx)
            .0
            .iter()
            .any(|item| item.starts_with("E13"))
    });
    assert_eq!(read(&fixture, "src/main.rs"), "fn main() {}");
    fixture.editor.press_unsettled(":w! src/main.rs<cr>", cx);
    wait_until(cx, |_| read(&fixture, "src/main.rs") == "# readme!");
    fixture
        .editor
        .press_unsettled(":sav src/ui/button.rs<cr>", cx);
    wait_until(cx, |cx| {
        fixture
            .editor
            .status(cx)
            .0
            .iter()
            .any(|item| item.starts_with("E13"))
    });
    assert_eq!(read(&fixture, "src/ui/button.rs"), "struct Button;");
    assert_eq!(fixture.editor.active_title(cx), "README.md");
}

#[gpui::test]
fn sav_writes_the_document_and_becomes_the_new_file(cx: &mut TestAppContext) {
    let fixture = fixture(false, cx);
    open_file(&fixture, "README.md", "# readme", cx);
    fixture.editor.press("A!<esc>", cx);
    fixture.editor.press_unsettled(":sav notes.md<cr>", cx);
    wait_until(cx, |_| fixture.directory.join("notes.md").exists());
    wait_until(cx, |cx| fixture.editor.active_title(cx) == "notes.md");
    assert_eq!(read(&fixture, "notes.md"), "# readme!");
    assert_eq!(read(&fixture, "README.md"), "# readme");
    assert!(!fixture.editor.active_is_dirty(cx));
}

#[gpui::test]
fn w_with_a_path_names_an_untitled_document(cx: &mut TestAppContext) {
    let fixture = fixture(false, cx);
    fixture.editor.press_unsettled(":w fresh.txt<cr>", cx);
    wait_until(cx, |cx| fixture.editor.active_title(cx) == "fresh.txt");
    assert_eq!(read(&fixture, "fresh.txt"), "scratch");
    assert!(!fixture.editor.active_is_dirty(cx));
}
