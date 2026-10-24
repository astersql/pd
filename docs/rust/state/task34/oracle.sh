#!/bin/bash
set -eu
: "${CARGO_TARGET_DIR:?claim shared slot first}"
# Explicit existing files exclude client_test.go and its failpoint-dependent TestMain.
# Selected tests do not enable failpoints, run RunServer/client streaming, or cancel full sync mid-loop.
GOOS=linux go test -mod=readonly -c -gcflags='all=-N -l' -o "$CARGO_TARGET_DIR/syncer-oracle34.test" pkg/syncer/server.go pkg/syncer/client.go pkg/syncer/history_buffer.go pkg/syncer/metrics.go pkg/syncer/server_test.go pkg/syncer/history_buffer_test.go
docker run --rm -v "$CARGO_TARGET_DIR:/build:ro" alpine:3.22 /build/syncer-oracle34.test -test.run '^Test(SyncFullRegionsBuffersLiveRecords|BindStreamWaitsForAppendHistoryRecordsBoundary|SyncHistoryRegionStartIndexZeroDoesNotGrowHistoryWindow|ClientWaitsForEmptySnapshotCatchUpCompletion|SyncFullRegionsFailsWhenCatchUpHistoryExceedsMax)$' -test.v
