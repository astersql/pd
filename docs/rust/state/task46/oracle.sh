#!/bin/bash
set -eu
: "${CARGO_TARGET_DIR:?claim a slot}"
test -f "$CARGO_TARGET_DIR/cluster-oracle41.test" || cp target/rust-slot-2/cluster-oracle41.test "$CARGO_TARGET_DIR/cluster-oracle41.test"
name="pd-state46-oracle-$$"
trap 'docker rm -f "$name" >/dev/null 2>&1 || true' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
shasum -a 256 "$CARGO_TARGET_DIR/cluster-oracle41.test"
docker run --rm --name "$name" -v "$CARGO_TARGET_DIR:/build:ro" alpine:3.22 /build/cluster-oracle41.test -test.run '^Test(RegionHeartbeat|HeartbeatSplit|OfflineAndMerge|RegionSizeChanged)$' -test.v
