# Availability loop

`AvailabilityLoop::initialize` awaits the canonical `Manager::initialize` future
before registration/spawn. `start` accepts an already initialized manager, host
Tokio Handle, parent CancellationToken, Registry, interval, and live snapshot
closure returning stores/config/Unix nanos/seconds. Default interval is 10 seconds.
No private runtime, global registry, protocol types, or storage implementation.

`manager` exposes the shared Mutex for host updates. Cancellation takes priority
both at the tick and while waiting for this lock. `shutdown` cancels and joins,
is repeatable and reports join errors; Drop cancels, with task-owned registration
released when the task exits. Hosts must await shutdown for synchronous cleanup.
Initialization and registration failures spawn no task. Tokio's host clock is
controllable in tests; missed ticks are skipped.

`AffinityMetrics` caches all four `pd_affinity_status{type=...}` gauge children,
retaining Go's help, names and gauge type. Periodic collection skips empty
managers exactly like Go. Group count, cache count and current-version affinity
count reflect real Manager data. Split rejection increments the cached gauge.
Registration is per host registry and is removed on task exit, including panic
unwinding, so subsequent starts on the same registry succeed.

Tests: independent `affinity_lifecycle_test.rs`, explicitly registered in lib.rs.
