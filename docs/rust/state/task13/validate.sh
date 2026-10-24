#!/bin/bash
set -u
run() {
  echo "COMMAND: $*"
  "$@"
  result=$?
  echo "EXIT_CODE=$result"
  return "$result"
}
run cargo fmt --all || exit $?
echo 'COMMAND: cargo metadata --format-version 1 --no-deps'
cargo metadata --format-version 1 --no-deps > docs/rust/state/task13/metadata.out
result=$?
echo "EXIT_CODE=$result"
[ "$result" -eq 0 ] || exit "$result"
run cargo fmt --all -- --check || exit $?
run cargo test -p astersql-pd-statistics --locked --lib state_task_13_load_values -- --nocapture || exit $?
run cargo test -p astersql-pd-statistics --locked || exit $?
run cargo check --workspace --locked || exit $?
run cargo test --workspace --locked || exit $?
