# Balance diagnostics summary

`balance_plan_summary`, `Summary` and `SummaryResult` implement Go's
`BalancePlanSummary`/`Summary` contract. Inputs are borrowed `dyn Any` plan values;
only canonical `BalanceSchedulerPlan` is accepted. Other types return the existing
`ERR_DIAGNOSTIC_LOAD_PLAN` typed catalog error, discarding partial results.

Steps above 2 are clamped to target selection. Step 1 attributes region failures
to the source store. Each store retains counters only at its greatest observed
step. Status identity includes the detailed reason. Selection compares priority,
then occurrence count, then lower code. Equal code/count/priority with different
reasons is unspecified in Go (map iteration); Rust keeps the first such reason.
The output BTreeMap normalizes store order without changing status semantics.

The normal flag starts true and becomes false on any processed non-normal status;
skipped lower-step records do not affect it, but replacing counters does not reset
it. Empty input returns an empty map and true. StatusOK therefore returns false
for the normal flag, matching the Go IsNormal definition. Missing resources and
step -1 retain Go's panic boundary; steps below -1 are skipped.

No protobuf, persistence, clock, I/O, global runtime or background worker is used.
The private comparator and reason grouping are covered by
`plan_summary_test.rs`. See [batch 8 evidence](../../../docs/rust/schedule/plan/8-证据.md)
for source provenance, real Go outputs, red/green checks and dependency audit.
