#!/bin/sh

main() {
    . ./ci/preamble.sh
    cargo_tests
    check_no_std_compatibility
}

cargo_tests() {
    cargo fmt --all --check
    cargo clippy --quiet --all-targets --all-features --workspace -- -D warnings
    cargo test --workspace --lib -- --nocapture
    cargo test --workspace --test '*' -- --nocapture
}

check_no_std_compatibility() {
    cargo build --no-default-features --package elb
    no_std_target=thumbv7m-none-eabi
    rustup target add "$no_std_target"
    cargo build --target "$no_std_target" --no-default-features --package elb
}

main
