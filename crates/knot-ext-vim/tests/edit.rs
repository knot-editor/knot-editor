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
