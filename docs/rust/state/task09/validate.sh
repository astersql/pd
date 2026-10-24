#!/bin/bash
set -u
run() {
  echo "COMMAND: $*"
  "$@"
  result=$?
  echo "EXIT: $result"
  if [ "$result" -ne 0 ]; then exit "$result"; fi
}
run cargo fmt --all
run cargo metadata --format-version 1 --no-deps > docs/rust/state/task09/metadata.out
run cargo fmt --all -- --check
run cargo test -p astersql-pd-statistics --locked --lib state_task_09_cpu -- --nocapture
run cargo test -p astersql-pd-statistics --locked
run cargo check --workspace --locked
run cargo test --workspace --locked
