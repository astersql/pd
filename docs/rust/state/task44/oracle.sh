#!/bin/bash
set -eu
: "${CARGO_TARGET_DIR:?claim a slot}"
test -f "$CARGO_TARGET_DIR/cluster-oracle41.test"
shasum -a 256 "$CARGO_TARGET_DIR/cluster-oracle41.test"
docker run --rm -v "$CARGO_TARGET_DIR:/build:ro" alpine:3.22 /build/cluster-oracle41.test -test.run '^Test(HeartbeatSplit|RegionSplitAndMerge|OfflineAndMerge)$' -test.v
