#!/bin/bash
set -eu
: "${CARGO_TARGET_DIR:?claim a slot}"
shasum -a 256 "$CARGO_TARGET_DIR/cluster-oracle41.test"
name="pd-state45-oracle-$$"
trap 'docker rm -f "$name" >/dev/null 2>&1 || true' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
docker run --rm --name "$name" -v "$CARGO_TARGET_DIR:/build:ro" alpine:3.22 /build/cluster-oracle41.test -test.run '^Test(BucketCompatibility|StaleBucketMeta|BucketHeartbeat)$' -test.v
