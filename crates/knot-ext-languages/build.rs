fn main() {
    knot_bundled::build::compile_packages(&[
        "language-markdown",
        "language-others",
        "language-python",
        "language-rust",
        "language-toml",
        "language-typescript",
    ]);
}
