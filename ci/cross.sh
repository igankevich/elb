#!/bin/sh

main() {
    . ./ci/preamble.sh
    for target in \
        aarch64-unknown-linux-gnu \
        armv5te-unknown-linux-gnueabi \
        armv7-unknown-linux-gnueabihf \
        armv7-unknown-linux-musleabi; do
        cross test --target $target --lib -- --nocapture
    done
    for target in \
        mipsel-unknown-linux-musl \
        mips-unknown-linux-musl; do
        cross +nightly test --target $target -Z build-std=std --lib -- --nocapture
    done
}

main
