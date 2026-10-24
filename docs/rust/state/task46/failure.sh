#!/bin/bash
set -eu
: "${CARGO_TARGET_DIR:?claim a slot}"
name="pd-state46-failure-$$"
trap 'docker rm -f "$name" >/dev/null 2>&1 || true' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
docker run --rm --name "$name" --cap-add SYS_PTRACE --security-opt seccomp=unconfined -v "$CARGO_TARGET_DIR:/build:ro" -v "$PWD/docs/rust/state/task46:/oracle:ro" alpine:3.22 sh -c 'apk add --no-cache --repository https://dl-cdn.alpinelinux.org/alpine/edge/community delve >/dev/null && dlv --check-go-version=false exec /build/cluster-oracle41.test --allow-non-terminal-interactive --init=/oracle/failure.commands -- -test.run "^TestRegionHeartbeat$"'
