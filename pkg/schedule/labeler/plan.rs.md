# Label transaction plans and patches

`LabelPlan::{new,set_label_rule,delete_label_rule,error,commit_ops,apply}` preserves
ordered operations and sticky validation errors. Invalid plans expose no operations
and cannot publish. `LabelCommitOp::buffer` composes with other modules inside the
existing endpoint `CallbackTxn`; the host supplies the endpoint and cluster paths.
`apply` is the explicit Go-compatible publication step: callers must commit first.
`RegionLabeler::new_plan` creates a detached plan; `commit_plan` safely combines
one transaction and publication.
Repeated writes to a key in a single plan retain etcd's duplicate-key rejection;
plans do not silently coalesce those operations.

`LabelRulePatch` and `RegionLabeler::patch` validate every set before I/O, keep the
last set per ID, and skip deletes of IDs also being set. Deletes retain input order;
unique sets use ID order (Go map order is unspecified). The existing endpoint's
`MAX_BATCH_OPS=120` bounds operations per transaction. Earlier batches remain
committed on later failure; memory stays unchanged until every batch succeeds.
Callers reload after partial or unknown commit failure. There is no cross-batch
atomicity, automatic retry, private runtime, background task, or protocol type.

Independent tests in `label_txn_test.rs` run the same contract against MemoryStore
and isolated EtcdMetaStore, compare the saved Go Patch output, compose a companion
write, and inject failures only before real backend transaction dispatch.
See [batch 18 evidence](../../../docs/rust/schedule/labeler/18-证据.md).
