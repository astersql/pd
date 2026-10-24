# Expiration cleanup and host metrics

`RegionLabeler::clear_expired_labels(now)` uses the authoritative expiry/index model.
Equal timestamps remain live. Changed rules are persisted individually; empty rules
are deleted. I/O errors are returned and do not stop later rules. Memory cleanup
precedes persistence, matching Go; failed deletes leave empty rules until restore.
Restore validates TTL again, so even failed persistence cannot resurrect old labels.
No retry or stronger transactional guarantee is claimed.

`get_rule_and_key_range_counts(now)` counts effective rules and original key ranges,
including duplicate ranges. It is the input for the host's rule_count/range_count
metric sampling (Go scheduler controller), independent of the GC interval.

`LabelerGc::start` requires a current Tokio runtime, a host-owned shared owner,
a nonzero interval and a host clock returning Unix nanoseconds. First tick occurs
after the interval. `shutdown` signals cancellation and joins, including when
waiting for the owner lock; an in-flight transaction finishes before joining.
Errors from the latest sweep are returned, keeping memory bounded by rule count.
Drop signals cancellation; hosts must use shutdown to observe task completion.

`LabelerMetrics::register` registers in the supplied Registry with Go metrics.go
names, help, counter labels and 13 exponential histogram buckets. No global state,
implicit registration or extra gauge series are introduced. Hosts cache counter
children for fixed checkers/schedulers deny events, observe construction duration,
and own collector lifecycle. Duplicate registration returns an error; partial
registration rolls back the event collector. This API does not instrument GC
with invented event labels, and does not introduce a scheduler dependency.

Independent tests are explicitly registered as `label_gc_test` in lib.rs. They
cover strict expiry, permanent labels, partial/full cleanup, counts, real MemoryStore
and EtcdMetaStore, WAL restart, transaction failure/continuation, paused Tokio time,
cancel while waiting for the lock, registry descriptors, and a Go runtime fixture.
