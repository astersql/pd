#!/bin/bash
set -u
failed=0
run() {
  echo "COMMAND: $*"
  "$@"
  code=$?
  echo "EXIT_CODE=$code"
  if [ "$code" -ne 0 ]; then failed=1; fi
}
run cargo fmt --all
run cargo metadata --format-version 1 --no-deps
run cargo fmt --all -- --check
run cargo test --locked -p astersql-pd-schedule-labeler label_rule -- --nocapture
run cargo test --locked -p astersql-pd-schedule-labeler
run cargo tree --locked -p astersql-pd-schedule-labeler
run cargo check --workspace --locked
run cargo test --workspace --locked
run git diff --check
exit "$failed"
