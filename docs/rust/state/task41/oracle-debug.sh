#!/bin/bash
set -eu
: "${CARGO_TARGET_DIR:?claim a shared slot first}"
for scenario in '' '-missing-region' '-missing-peer'; do
  docker run --rm --cap-add SYS_PTRACE --security-opt seccomp=unconfined \
    -e "COMMANDS=/oracle/oracle${scenario}.commands" \
    -v "$CARGO_TARGET_DIR:/build:ro" -v "$PWD/docs/rust/state/task41:/oracle:ro" \
    alpine:3.22 sh -c 'apk add --no-cache --repository https://dl-cdn.alpinelinux.org/alpine/edge/community delve >/dev/null && dlv --check-go-version=false exec /build/cluster-oracle41.test --allow-non-terminal-interactive --init="$COMMANDS" -- -test.run "^TestStoreHeartbeat$"' > "docs/rust/state/task41/oracle${scenario}-final.out" 2>&1
done
