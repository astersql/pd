# Region heartbeat admission

`prepare_region_heartbeat` validates the report against BasicCluster's real root,
inherits missing size/keys and optionally report buckets, then runs the canonical
core region guide. The owned preparation permits a host to release a read lock
before acquiring exclusive access for `PreparedRegionHeartbeat::commit`.

Commit rechecks current epoch/term and overlaps whenever the guide requests a
cache write, commits the root and refreshes store-role subtrees from that root.
A duplicate skips the root write and refreshes its subtree, repairing root-only
entries. Incoming term zero remains unknown, and a higher term alone does not
force a cache write. Unsafe-recovery exceptions come from canonical core checks.
Equal-epoch concurrent statistics can overwrite one another in commit order;
there is no stronger ordering guarantee. A prepared duplicate can succeed after
another report evicts it, but subtree refresh cannot restore its missing ID.

`handle_region_heartbeat` is the sequential convenience entry point. Results
contain the inherited report, guide and actual evictions for later host wiring.
Persistence, hot statistics, classification, sync queues and scheduling are not
performed here. No protobuf, runtime, storage backend or global state is added.
Tests are explicitly registered in lib.rs and use real BasicCluster indexes.
