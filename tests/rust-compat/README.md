# PD compatibility acceptance entrypoint

Use the read-only supervisor and existing Go binaries for Go/Rust compatibility.
`run-pd-compat-readonly.sh --mode go|rust|dual --case health-member` runs the
reviewed fixture against isolated real PD processes. Set absolute
`PD_COMPAT_GO_BIN` / `PD_COMPAT_RUST_BIN`; missing or invalid binaries fail before
startup. The legacy `run-pd-compat.sh` builds Go and rewrites failpoints and must
not be used under the Go read-only contract.

Before **any** Cargo command, atomically claim the first available repository
slot 1–10 with `mkdir target/rust-slot-locks/slot-N.lock` (create the parent first).
Record plan, task, session and holder PID in that directory. Export the absolute
`CARGO_TARGET_DIR=$PWD/target/rust-slot-N`. Keep the lock until Cargo and every
child using its artifacts have exited, then remove your owner record and `rmdir`
your lock on success, failure and interruption. Keep compilation caches. Never
remove another session's lock without checking its processes.

After claiming a slot, build the Rust binary and run the protocol regression:

```sh
cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server
export PD_COMPAT_MODE=dual
export PD_COMPAT_GO_BIN="$PWD/bin/pd-server"
export PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server"
cargo test --locked -p astersql-pd-compat-tests --lib task_100_ -- --nocapture
```

Run from the repository root, with default features. `task_100_behavior` requires
all 41 implemented classic PD matrix methods through 23 real differential
entrypoints, including explicit execution of the existing ignored oracle tests.
The independent negative controls retain error/status/default/enum differences.
See [task 100 evidence](../../docs/rust/server-api/task-100.md) for method mapping,
backend topology differences and complete validation commands.

`cargo test --locked -p astersql-pd-compat-tests -- --nocapture` additionally runs
the package regressions; the task 100 test always runs dual mode and requires the
existing Go oracle even when the surrounding package uses its default environment.
The package also needs task 99's existing Go benchmark binaries in the claimed
slot; its read-only setup is documented in [task 99](../../docs/rust/server-api/task-099.md).
The deliberately incompatible fixture service tests orchestration and negative
controls only; it is not Rust PD business compatibility evidence.

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
Per-surface deployment metadata rules remain explicit: gRPC membership checks
its default metadata, while the minimal HTTP member adapter omits the three
reviewed build/deployment fields. These omissions do not apply to errors or state.

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
