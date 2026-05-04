# Set the default recipe
default:
    just test
    cargo build --release

# Run linting and unit tests
test:
    just lint
    just test-unit

# Lint the codebase
lint:
    cargo clippy --all-features -- -D warnings
    cargo fmt -- --check

# Run unit tests with dynamic target
test-unit:
    RUST_LOG=${RUST_LOG} cargo test --target=`rustc -vV | sed -n 's|host: ||p'`

release:
    #!/usr/bin/env fish
    set this_version (grep '^version =' Cargo.toml | head -n 1 | sed -E 's/version = "(.*)"/\1/')
    git tag v$this_version
    git push origin v$this_version
    set -e this_version
