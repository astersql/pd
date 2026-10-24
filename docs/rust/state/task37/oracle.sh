#!/bin/bash
set -eu
GOOS=linux go test -mod=readonly -c -gcflags='all=-N -l' -o "$CARGO_TARGET_DIR/syncer-oracle37.test" pkg/syncer/server.go pkg/syncer/client.go pkg/syncer/history_buffer.go pkg/syncer/metrics.go pkg/syncer/server_test.go pkg/syncer/history_buffer_test.go pkg/syncer/client_test.go
docker run --rm -v "$CARGO_TARGET_DIR:/build:ro" alpine:3.22 /build/syncer-oracle37.test -test.run '^Test(ErrorCode|HandleRegionSyncResponseSkipsErrorResponse|ClientWaitsForFullSyncCompletionBeforeRunning|ClientFinishesFullSyncWithOldLeaderKeepAlive|ClientWaitsForEmptySnapshotCatchUpCompletion)$' -test.v
