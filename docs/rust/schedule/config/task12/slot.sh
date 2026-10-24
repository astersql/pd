#!/bin/bash
set -u
cd /Users/Shared/work/dir/data/codes/pd
mkdir -p target/rust-slot-locks
for n in {1..10}; do
 if mkdir "target/rust-slot-locks/slot-$n.lock" 2>/dev/null; then
  lock="target/rust-slot-locks/slot-$n.lock"
  echo "plan=调度配置 task=12 session=01a16d20 PID=$$" > "$lock/owner"
  export CARGO_TARGET_DIR="$PWD/target/rust-slot-$n"
  trap 'rm -f "$lock/owner"; rmdir "$lock"' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  export GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off
  echo "SLOT=$n CARGO_TARGET_DIR=$CARGO_TARGET_DIR"
  "$@"
  exit $?
 fi
done
exit 75
