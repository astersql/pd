#!/bin/bash
set -u
result=0
for cmd in \
 'cargo fmt --all' \
 'cargo metadata --format-version 1 --no-deps' \
 'cargo fmt --all -- --check' \
 'cargo test --locked -p astersql-pd-server-config config_hot_update -- --nocapture' \
 'cargo test --locked -p astersql-pd-server-config' \
 'cargo tree --locked -p astersql-pd-server-config' \
 'cargo tree --locked -p astersql-pd-schedule-config' \
 'cargo check --workspace --locked' \
 'cargo test --workspace --locked' \
 'git diff --check'; do
 echo "COMMAND: $cmd"
 bash -c "$cmd"
 code=$?
 echo "EXIT_CODE: $code"
 if [ "$code" -ne 0 ]; then result=1; fi
done
exit "$result"
