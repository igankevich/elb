#!/bin/sh

main() {
    . ./ci/preamble.sh
    cargo fmt --all --check
    cargo clippy --quiet --all-targets --all-features --workspace -- -D warnings
    cargo test --workspace --lib -- --nocapture
    cargo test --workspace --test '*' -- --nocapture
}

main
