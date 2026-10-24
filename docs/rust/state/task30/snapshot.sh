#!/bin/bash
set -eu
: "${CARGO_TARGET_DIR:?claim shared slot first}"
if [ ! -f "$CARGO_TARGET_DIR/history-oracle30.test" ]; then
 cp target/rust-slot-1/history-oracle30.test "$CARGO_TARGET_DIR/history-oracle30.test"
fi
docker run --rm --cap-add SYS_PTRACE --security-opt seccomp=unconfined \
 -v "$CARGO_TARGET_DIR:/build:ro" -v "$PWD/docs/rust/state/task30:/oracle:ro" \
 alpine:3.22 sh -c 'apk add --no-cache --repository https://dl-cdn.alpinelinux.org/alpine/edge/community delve >/dev/null && dlv --check-go-version=false exec /build/history-oracle30.test --allow-non-terminal-interactive --init=/oracle/oracle.commands -- -test.run "^TestHistoryBufferKeepsRingBehaviorWithoutRetain$"'
