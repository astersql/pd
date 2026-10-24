# Heartbeat state pipeline

`StatePipeline` borrows canonical `HeartbeatEffectsState`, `HotStat`, runners,
StorageEndpoint and Downstreams. The host owns serialization and lifecycle.
Store and bucket inputs use their existing admission handlers; region inputs use
`submit_region_heartbeat`, then admit canonical hot-flow expiration/read/write
work only for accepted reports. Duplicate reports remain valid flow samples;
stale reports never reach the hot queues or side effects. Queue submission is
best effort, with the same policy as existing store/bucket handling.

No scheduling decisions, protobuf/tonic, private runtime, new storage/history
algorithm or automatic retries are introduced. The serialized host path makes
post-admission hot submission equivalent for accepted reports to Go's submission
before its guide/second root check. It does not emulate Go's concurrently rejected
second-check hot side effects; callers must serialize pipeline admissions.

Wait for effect receipts and quiesce admissions to obtain a cross-cache checkpoint.
`snapshot` uses HotStat FIFO barriers and copies stores, region-role counters,
classification bitmaps and label counters. Region entries retain immutable Arc
snapshots; returned raw store statistics and collections do not share mutable
holders. A closed worker returns an error, never a partial StatisticsSnapshot.
The view is not an atomic transaction across concurrent admissions.

Use `RegionSyncer::for_heartbeat_state` at the server boundary, passing its
`downstreams()` to this pipeline. This shares one root, one history and the same
snapshot/bind gate. Do not also call publish for those reports. The sync effect
locks canonical state while appending and broadcasting; it holds no lock over I/O.
Standalone `RegionSyncer::new/publish` remains available for other hosts.

Sync followers retain Go's consumed subset: metadata, leader, four raw flow
counters and bucket boundaries. Size/keys/down/pending are not replicated by this
protocol. Bucket-only reports update local state/worker; a later admitted region
change carries effective buckets downstream. Split/merge use canonical overlap
cleanup and storage deletion; history indices never mean MetaStore revisions.

Independent tests in state_pipeline_test.rs replay accepted/duplicate/stale,
bucket, epoch, split and merge through real MemoryStore/EtcdMetaStore, compare
per-step follower/cache/KV/role/classification snapshots, query hot caches and
verify detached store copies and closed-worker errors. The server transport test
uses real RegionSyncer HTTP/2 incremental and full subscriptions against this same
state. Hosts stop/join runners, hot workers, streams and listeners explicitly.
