#!/bin/bash
set -eu
: "${CARGO_TARGET_DIR:?claim shared slot first}"
GOOS=linux go test -mod=readonly -c -gcflags="all=-N -l" -o "$CARGO_TARGET_DIR/statistics-oracle27.test" ./pkg/statistics
docker run --rm --cap-add SYS_PTRACE --security-opt seccomp=unconfined \
 -v "$CARGO_TARGET_DIR:/build:ro" -v "$PWD/docs/rust/state/task27:/oracle:ro" \
 alpine:3.22 sh -c 'apk add --no-cache --repository https://dl-cdn.alpinelinux.org/alpine/edge/community delve >/dev/null && dlv --check-go-version=false exec /build/statistics-oracle27.test --allow-non-terminal-interactive --init=/oracle/oracle.commands -- -test.run "^TestStoreStatistics$" -test.v'
