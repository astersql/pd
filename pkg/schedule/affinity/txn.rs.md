# Group creation and deletion

`Manager::create_affinity_groups`, `create_affinity_groups_if_not_exists`, and
`delete_affinity_groups` compose canonical labeler plans with StorageEndpoint
operations. Callers supply the matching storage, cluster paths and mutable labeler;
exclusive Rust borrows serialize publication. Time is explicit Unix seconds.

Creation validates every ID before filtering existing groups, checks new and
cached ranges together, validates all labels, and orders label writes before group
writes. Every group starts degraded, version 1; attaching even an absent label
increments its version. IfNotExists preserves existing groups and ranges.

Deletion rejects empty requests, deduplicates IDs in input order, checks all ranges
before checking missing groups, and lets force remove unknown groups' orphan labels.
Memory groups, ranges and label rules change only after all commits succeed.

Transactions retain endpoint's 120-operation batches: a later failure leaves prior
batches durable and memory unpublished. Reload reconciles that state. TransactionError
preserves ID, PD content, label validation, callback and backend error variants.
Canonical MetaStore rejects repeated writes to a key within one transaction like
etcd; Go's legacy memory backend accepts them. No affinity-level deduplication
changes this endpoint boundary. Duplicate deletes are always deduplicated.

Tests use the real MemoryStore and EtcdMetaStore, and a forwarding MetaStore gate
that only rejects a selected backend transaction call. Matching, range validation,
label plans and transaction results are not mocked. The crate shares one etcd
fixture module so concurrent tests use a single binary initializer and directory
counter. See [batch 34 evidence](../../../docs/rust/schedule/affinity/34-证据.md).
