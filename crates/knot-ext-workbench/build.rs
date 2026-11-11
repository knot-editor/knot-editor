fn main() {
    knot_bundled::build::compile_packages(&[
        "projects",
        "status-bar",
        "go-to-line",
        "find",
        "go-to-file",
        "explorer",
        "themes",
    ]);
}
