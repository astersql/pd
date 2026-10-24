#!/bin/bash
set -eu
: "${CARGO_TARGET_DIR:?claim shared slot first}"
# Only existing files: excludes syncer client/server tests and their fault injection.
GOOS=linux go test -mod=readonly -c -gcflags='all=-N -l' -o "$CARGO_TARGET_DIR/history-oracle30.test" pkg/syncer/history_buffer.go pkg/syncer/metrics.go pkg/syncer/history_buffer_test.go
docker run --rm -v "$CARGO_TARGET_DIR:/build:ro" alpine:3.22 /build/history-oracle30.test -test.run '^Test(NormalizeHistoryBufferCapacity|HistoryBuffer.*)$' -test.v
