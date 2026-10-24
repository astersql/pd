# Group range updates

`Manager::update_affinity_group_key_ranges` accepts separate additions and removals,
matching the Go transaction order. Nonempty duplicate group operations and mixed
add/remove operations fail before reads. Empty operations still resolve the group
and publish a label update. Cache misses read the actual label rule; an existing
group without a label starts empty. Missing groups retain the untyped missing-label
error via `TransactionError::MissingLabel`.

Additions validate against all cached ranges before removals. Consequently a range
cannot migrate between groups in one request. Exact removal uses the canonical
range helper; label validation runs before storage. Label operations commit in
120-operation batches using StorageEndpoint. Only after all batches succeed are
labeler, range cache, counts and affinity versions published. Removal invalidates
region counts, including when some ranges remain. No group metadata is rewritten.
The future region ownership cache is outside this batch's existing Manager model.

The caller supplies matching storage, paths and labeler and serializes access with
exclusive mutable borrows. Time is explicit Unix seconds. There is no private
runtime or background task. See [batch 35 evidence](../../../docs/rust/schedule/affinity/35-证据.md).
