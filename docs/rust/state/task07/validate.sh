#!/bin/bash
set -u
run() {
    printf '\nCOMMAND:'
    printf ' %q' "$@"
    printf '\n'
    "$@"
    code=$?
    printf 'EXIT=%s\n' "$code"
    return "$code"
}
run cargo fmt --all || exit $?
run cargo metadata --format-version 1 --no-deps > docs/rust/state/task07/metadata.out || exit $?
run cargo fmt --all -- --check || exit $?
run cargo test -p astersql-pd-statistics-buckets --locked --lib state_task_07_bucket_degree -- --nocapture || exit $?
run cargo test -p astersql-pd-statistics-buckets --locked || exit $?
run cargo check --workspace --locked || exit $?
run cargo test --workspace --locked || exit $?
