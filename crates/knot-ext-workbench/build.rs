fn main() {
    knot_bundled::build::compile_packages(&[
        "projects",
        "status-bar",
        "go-to-line",
        "find",
        "word-highlight",
        "go-to-file",
        "explorer",
        "themes",
    ]);
}
