set -e
cargo generate-lockfile --offline
cargo fmt --all
set +e
cargo test --locked -p astersql-pd-schedule-affinity affinity_values -- --nocapture
r=$?
echo "EXIT=$r"
test "$r" = 101
