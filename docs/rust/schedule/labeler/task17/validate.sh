#!/bin/bash
set -u
run() { printf '\nCOMMAND:'; printf ' %q' "$@"; printf '\n'; "$@"; result=$?; echo "EXIT=$result"; if [ "$result" != 0 ]; then exit "$result"; fi; }
run cargo fmt --all
run cargo metadata --format-version 1 --no-deps
run cargo fmt --all -- --check
run cargo test --locked -p astersql-pd-schedule-labeler label_restore -- --nocapture
run cargo test --locked -p astersql-pd-schedule-labeler
run cargo tree --locked -p astersql-pd-schedule-labeler
run cargo check --workspace --locked
run cargo test --workspace --locked
run git diff --check
