#!/bin/bash
set -u
run() { printf '\nCOMMAND: %s\n' "$*"; "$@"; code=$?; echo "EXIT=$code"; if [ "$code" != 0 ]; then exit "$code"; fi; }
run cargo fmt --all
run cargo metadata --format-version 1 --no-deps
run cargo fmt --all -- --check
run cargo test --locked -p astersql-pd-schedule-affinity affinity_restore -- --nocapture
run cargo test --locked -p astersql-pd-schedule-affinity
run cargo tree --locked -p astersql-pd-schedule-affinity
run cargo check --workspace --locked
run cargo test --workspace --locked
run git diff --check
