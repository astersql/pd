#!/usr/bin/env bash
# Run one synchronous validation phase from the repository root.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
mkdir -p target/rust-slot-locks
while :; do
  for slot in {1..10}; do
    lock="target/rust-slot-locks/slot-$slot.lock"
    if mkdir "$lock" 2>/dev/null; then
      trap 'rm -f "$lock/owner"; rmdir "$lock"' EXIT
      trap 'exit 130' INT
      trap 'exit 143' TERM
      export CARGO_TARGET_DIR="$PWD/target/rust-slot-$slot"
      printf 'plan=schedule-execution task=002 session=01a1714f-975b-7522-b165-5bc4edd7f096 pid=%s\n' "$$" > "$lock/owner"
      printf 'slot=%s CARGO_TARGET_DIR=%s command=' "$slot" "$CARGO_TARGET_DIR"
      printf '%q ' "$@"
      printf '\n'
      "$@"
      exit $?
    fi
  done
  sleep 2
done
