#!/bin/bash
set -eu
: "${CARGO_TARGET_DIR:?claim shared slot first}"
# Existing binary built from unchanged Go source in task25.
test -f "$CARGO_TARGET_DIR/cluster-oracle25.test"
docker run --rm --cap-add SYS_PTRACE --security-opt seccomp=unconfined \
 -v "$CARGO_TARGET_DIR:/build:ro" -v "$PWD/docs/rust/state/task26:/oracle:ro" \
 alpine:3.22 sh -c 'apk add --no-cache --repository https://dl-cdn.alpinelinux.org/alpine/edge/community delve >/dev/null && dlv --check-go-version=false exec /build/cluster-oracle25.test --allow-non-terminal-interactive --init=/oracle/oracle.commands -- -test.run "^TestCollectMetrics$"'
