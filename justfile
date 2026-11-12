set default-list := true

# Format Rust sources
fmt:
    cargo fmt

# Lint the product build (no test features) and all Rust targets
lint:
    cargo clippy --locked --workspace -- -D warnings
    cargo clippy --locked --workspace --all-targets -- -D warnings

# Run all Rust tests, each in its own process, then doc tests
test:
    cargo nextest run --workspace --all-targets
    cargo test --workspace --doc

# Time a file walk over a directory in release mode
walk-bench directory runs="5":
    cargo run --release -p knot-model --example walk_bench -- {{directory}} {{runs}}

# Run Knot
run:
    cargo run -p knot-app

# Build a local macOS app bundle
app:
    scripts/build-macos-app.sh

# Build a release macOS app bundle
app-release:
    scripts/build-macos-app.sh --release

# Archive the release app as a nightly (the version defaults to the current UTC time)
app-package version=`date -u +nightly.%Y%m%d.%H%M`:
    scripts/package-macos-app.sh {{version}}

# Type-check and test extension packages
ext:
    scripts/check-extensions.sh

# Format, lint, and test all Rust targets and extension packages
check: fmt lint test ext

