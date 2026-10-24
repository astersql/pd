#!/bin/bash
set -eu
: "${CARGO_TARGET_DIR:?claim shared slot first}"
# Existing binary built from unchanged Go source in task25.
if [ ! -f "$CARGO_TARGET_DIR/cluster-oracle25.test" ]; then
 cp target/rust-slot-1/cluster-oracle25.test "$CARGO_TARGET_DIR/cluster-oracle25.test"
fi
docker run --rm --cap-add SYS_PTRACE --security-opt seccomp=unconfined \
 -v "$CARGO_TARGET_DIR:/build:ro" -v "$PWD/docs/rust/state/task28:/oracle:ro" \
 alpine:3.22 sh -c 'apk add --no-cache --repository https://dl-cdn.alpinelinux.org/alpine/edge/community delve >/dev/null && dlv --check-go-version=false exec /build/cluster-oracle25.test --allow-non-terminal-interactive --init=/oracle/cluster.commands -- -test.run "^TestCollectMetrics$"'
