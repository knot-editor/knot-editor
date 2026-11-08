//! Every bundled package together under product startup.
use gpui::TestAppContext;

use knot::test_support::TestProduct;
use knot_app::packages;

#[gpui::test]
fn bundled_packages_load_with_product_discovery(cx: &mut TestAppContext) {
    let product = TestProduct::start(packages(), cx);
    let report = product.load_report(cx);
    for name in [
        "@knot/projects",
        "@knot/go-to-line",
        "@knot/language-markdown",
        "@knot/language-others",
        "@knot/language-python",
        "@knot/language-rust",
        "@knot/language-toml",
        "@knot/language-typescript",
        "@knot/themes",
        "@knot/vim",
    ] {
        let entry = report
            .iter()
            .find(|entry| entry.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("{name} was not discovered"));
        // A query that fails to compile rejects its registration and so
        // fails the package's start.
        assert!(entry.loaded, "{name}: {}", entry.detail);
    }
    // Every compiled-in grammar has a language, so every bundled query
    // compiled against its grammar.
    let languages = product.languages(cx);
    let mut grammars: Vec<_> = languages
        .iter()
        .map(|language| language.grammar.as_str())
        .collect();
    grammars.sort();
    grammars.dedup();
    assert_eq!(grammars, TestProduct::grammar_ids());
    assert!(
        languages
            .iter()
            .any(|language| language.id == "typescriptreact")
    );

    // The bundled themes include the native selection's defaults, so startup
    // resolves One Dark rather than the native fallback colors.
    product.apply_startup_theme(cx);
    assert!(product.theme_is_dark(cx));
    assert_eq!(
        Some(product.theme_color("editor.background", cx)),
        TestProduct::parse_color("#282c34")
    );
}

/// Each bundled package's heap once startup settles, as Show Extension
/// Memory reports it.
#[gpui::test]
#[ignore = "measurement: run in release with --ignored --nocapture"]
fn measure_bundled_package_memory(cx: &mut TestAppContext) {
    let product = TestProduct::start(packages(), cx);
    println!("{}", product.memory_report(cx));
}
