#!/bin/sh

. ./ci/preamble.sh

cargo_publish() {
    for name in elb elb-dl elb-cli; do
        cargo publish --quiet --package "$name"
    done
}

if test "$GITHUB_ACTIONS" = "true" && test "$GITHUB_REF_TYPE" != "tag"; then
    exit 0
fi
cargo_publish
