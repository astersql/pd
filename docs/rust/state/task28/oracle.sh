#!/bin/bash
set -eu
: "${CARGO_TARGET_DIR:?claim slot first}"
# Reuse the immutable task27 test binary, copied only if a different slot is acquired.
if [ "$CARGO_TARGET_DIR" != "$PWD/target/rust-slot-1" ]; then
 cp target/rust-slot-1/statistics-oracle27.test "$CARGO_TARGET_DIR/statistics-oracle27.test"
fi
docker run --rm -v "$CARGO_TARGET_DIR:/build:ro" alpine:3.22 /build/statistics-oracle27.test -test.run '^Test(ResetStoreStatisticsClearsPlacementStatusGauge|StoreLimitMetricsIncludeTransferLeaderIn)$' -test.v
