#!/bin/bash
set -eu
# Selected existing tests and setup/teardown need no failpoint injection.
GOOS=linux go test -mod=readonly -c -o "$CARGO_TARGET_DIR/etcdutil-oracle39-arm64.test" ./pkg/utils/etcdutil
docker run --rm -v "$CARGO_TARGET_DIR:/build:ro" alpine:3.22 /build/etcdutil-oracle39-arm64.test -test.run '^TestLoopWatcherTestSuite$/(TestWatcherConsistentLoadUsesSingleRevision|TestWatcherAtomicLoadCallbacksDiscardIncompleteLoad|TestWatcherReloadsAfterCompactionWhenEnabled|TestWatcherUsesCustomCompactionReload|TestWatcherLoadCancelsInFlightRequest|TestCallBack)$' -test.v
