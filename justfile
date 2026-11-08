set default-list := true

# Format Rust sources
fmt:
    cargo fmt

# Lint all Rust targets
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Run all Rust tests, each in its own process, then doc tests
test:
    cargo nextest run --workspace --all-targets
    cargo test --workspace --doc

# Run Knot
run:
    cargo run -p knot-app

# Build a local macOS app bundle
app:
    scripts/build-macos-app.sh

# Build a release macOS app bundle
app-release:
    scripts/build-macos-app.sh --release

# Type-check and test extension packages
ext:
    scripts/check-extensions.sh

# Format, lint, and test all Rust targets and extension packages
check: fmt lint test ext

