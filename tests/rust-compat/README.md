# PD compatibility acceptance entrypoint

`run-pd-compat.sh --mode go|rust|dual --case health-member` runs the same
reviewed fixture against isolated real PD processes. Set absolute
`PD_COMPAT_GO_BIN` / `PD_COMPAT_RUST_BIN` to use existing binaries. Without a Go
binary the script builds the current source with `without_dashboard`; it installs
only the module-pinned failpoint tool and invokes the existing make restoration
target before and after testing. Rust must implement the same server arguments
and APIs before using rust/dual. The current lifecycle shell is not a PD oracle.

Before **any** Cargo command, atomically claim the first available repository
slot 1–10 with `mkdir target/rust-slot-locks/slot-N.lock` (create the parent first).
Record plan, task, session and holder PID in that directory. Export the absolute
`CARGO_TARGET_DIR=$PWD/target/rust-slot-N`. Keep the lock until Cargo and every
child using its artifacts have exited, then remove your owner record and `rmdir`
your lock on success, failure and interruption. Keep compilation caches. Never
remove another session's lock without checking its processes.

Run `cargo test -p astersql-pd-compat-tests -- --nocapture` for the harness tests,
then `./scripts/run-pd-compat.sh --mode go --case health-member` for the actual
Go oracle. The deliberately incompatible Rust fixture service tests orchestration
only; its passing results are not evidence of Rust PD business compatibility.

## Reviewed oracle and normalization

Sources: `server/api/health.go`, `server/api/member.go`,
`pkg/schedule/config/config.go`, `server/config/config.go`, and
`tests/server/api/health_test.go`. The singleton must be healthy, named `compat`,
have a positive ID and its advertised client URL. Health, membership and leader
IDs must agree. HTTP status is exact. Defaults are snapshot limit 64 and replica
count 3. The JSON fixture is hand-reviewed against these sources, not a recording.
The first config sample intentionally selects these two stable defaults;
additional config behavior needs additional explicit cases.

JSON object ordering is irrelevant. Arrays remain ordered. Only registered
member/cluster IDs and advertised client/peer URLs in named fields are replaced;
ID equality relationships and nonidentity values survive. Missing fields, unknown
fields, errors, health, member count and all selected config values remain exact.
Full health/member responses are compared in dual mode, including version and
deploy metadata: expected future platform differences require reviewed new rules.

The protobuf TSO adapter decodes the pinned generated type, retaining optional
message presence, errors, count, logical time and suffix bits. Explicit physical
clock origins remove wall-clock offsets, preserving physical deltas. No automatic
timestamp deletion occurs. Metrics ignore sample line ordering only; labels,
help/type, values and timestamps stay exact. More relaxed metric handling must be
justified per metric. JSON differences report field paths and both values.

Every server owns a unique temporary directory, client and peer ports, and a log
file. Readiness and HTTP requests are bounded. On failure the readiness error
includes logs; dropping the guard kills/reaps its direct process and removes its
directory. Port reservations are released immediately before launch, so a rare
external bind race is an explicit startup failure. CI should retry the case if
needed. These are direct foreground processes, not daemonizing launchers.
