#!/bin/bash
set -u
run() { printf '\nCOMMAND:'; printf ' %q' "$@"; printf '\n'; "$@"; rc=$?; echo "EXIT=$rc"; return "$rc"; }
run cargo fmt --all || exit
run cargo metadata --format-version 1 --no-deps || exit
run cargo fmt --all -- --check || exit
run cargo test --locked -p astersql-pd-schedule-affinity affinity_lifecycle -- --nocapture || exit
run cargo test --locked -p astersql-pd-schedule-affinity || exit
run cargo tree --locked -p astersql-pd-schedule-affinity || exit
run cargo check --workspace --locked || exit
run cargo test --workspace --locked || exit
run git diff --check
