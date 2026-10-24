#!/bin/bash
set -uo pipefail
run() {
  printf '\nCOMMAND:'; printf ' %q' "$@"; printf '\n'
  "$@"
  code=$?
  printf 'EXIT=%s\n' "$code"
  if [ "$code" -ne 0 ]; then exit "$code"; fi
}
run cargo fmt --all
run cargo metadata --format-version 1 --no-deps
run cargo fmt --all -- --check
run cargo test --locked -p astersql-pd-compat-tests schedule_foundation -- --nocapture
run cargo test --locked -p astersql-pd-compat-tests
run cargo tree --locked -p astersql-pd-compat-tests
run cargo check --workspace --locked
run cargo test --workspace --locked
run git diff --check
