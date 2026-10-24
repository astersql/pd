#!/bin/bash
set -eu
: "${CARGO_TARGET_DIR:?claim shared slot first}"
case_name=${1:?fields, mismatch or load}
case "$case_name" in
 fields|mismatch) test_name=TestClientWaitsForEmptySnapshotCatchUpCompletion ;;
 load) test_name=TestLoadRegion ;;
 *) exit 2 ;;
esac
GOOS=linux GOARCH=arm64 go test -mod=readonly -c -gcflags='all=-N -l' -o "$CARGO_TARGET_DIR/syncer-oracle37-arm64.test" pkg/syncer/server.go pkg/syncer/client.go pkg/syncer/history_buffer.go pkg/syncer/metrics.go pkg/syncer/server_test.go pkg/syncer/history_buffer_test.go pkg/syncer/client_test.go
docker run --rm --cap-add SYS_PTRACE --security-opt seccomp=unconfined \
 -v "$CARGO_TARGET_DIR:/build:ro" -v "$PWD/docs/rust/state/task37:/oracle:ro" \
 alpine:3.22 sh -c 'apk add --no-cache --repository https://dl-cdn.alpinelinux.org/alpine/edge/community delve >/dev/null && dlv --check-go-version=false exec /build/syncer-oracle37-arm64.test --allow-non-terminal-interactive --init="/oracle/'"$case_name"'.commands" -- -test.run "^'"$test_name"'$" -test.v'
