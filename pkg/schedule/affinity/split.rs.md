# Split permission

`SplitReason::{Admin, Size, Load, Other(i32)}` is a domain value; protocol mapping
belongs to host adapters. `Manager::allow_split` consumes the canonical region,
labeler and shared configuration view, with caller supplied Unix seconds.

Missing region, Admin, disabled affinity or zero configured merge size allow split.
For a group forbidding regular scheduling, both size and keys must be strictly
below `(configured_size + 10) * 4` MiB and that value times 10000 to deny.
The region affinity boolean is deliberately unused: peer mismatch and degraded
but unexpired groups still block. Signed wrapping arithmetic matches Go int64.
The query does not write caches or storage. Metric collection is batch 40 scope.

Behavior and verification: [batch 39 evidence](../../../docs/rust/schedule/affinity/39-证据.md).

Batch 40 records each actual automatic split rejection in the cached
`pd_affinity_status{type="split-deny-count"}` gauge when attached to a host loop.
