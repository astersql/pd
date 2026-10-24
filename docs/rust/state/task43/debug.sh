#!/bin/bash
set -eu
: "${CARGO_TARGET_DIR:?claim a slot}"
docker run --rm --cap-add SYS_PTRACE --security-opt seccomp=unconfined \
  -e "ORACLE_COMMANDS=${ORACLE_COMMANDS:-oracle.commands}" \
  -v "$CARGO_TARGET_DIR:/build:ro" -v "$PWD/docs/rust/state/task43:/oracle:ro" \
  alpine:3.22 sh -c 'apk add --no-cache --repository https://dl-cdn.alpinelinux.org/alpine/edge/community delve >/dev/null && dlv --check-go-version=false exec /build/cluster-oracle41.test --allow-non-terminal-interactive --init="/oracle/$ORACLE_COMMANDS" -- -test.run "^TestRegionHeartbeat$"'
