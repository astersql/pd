# Scheduling foundation integration harness

`schedule_foundation::contract(store, seed)` is compiled only for tests. It assembles
canonical RuleManager, RegionLabeler and affinity Manager over the supplied real
MetaStore. It creates no runtime or background loop and exports no service API.

The immutable task27 Go HTTP fit fixture supplies 15 store/peer/rule inputs and
ordered expected assignments. `seed=true` persists the rules and creates affinity
labels; `seed=false` only restores them, allowing the same assertions after an etcd
WAL restart. The same region snapshot is used for placement and affinity queries.
The integration region uses byte range `[01,02)`; all fixture placement rules span
the full keyspace, so this projection preserves the recorded fit inputs.

Each case checks selected and mismatched peers, orphan order, isolation/witness
scores, satisfaction, label membership and its exclusive end boundary, restored
group ranges, duplicate and overlapping creation rejection without publication.
The independent tests also stop the real etcd process to test storage failures.
The metadata dependency test checks the normal workspace dependency closure of
all eight foundation crates and prohibits reverse scheduling/server dependencies
and direct domain imports of the protocol crate. Existing endpoint protocol
adapter dependencies remain visible in the closure.
