# Single-rule fit

`RuleFit` owns an immutable `Arc<Rule>`, canonical core peers, selected store
snapshots, peers requiring role conversion, isolation and witness-load scores.
`is_satisfied` checks exact count and no role mismatches. Health does not affect
satisfaction. `contains_store` searches selected peers. `compare` orders count,
fewer role mismatches, higher isolation, then lower witness load.

`fit_rule(stores, region, rule, support_witness)` selects one rule independently.
It rejects missing stores and label/exclusive-label mismatches, and excludes the
leader from witness rules when witness support is enabled. Candidates sort by
healthy, pending, down, then peer ID. Down takes priority over pending. Role
mismatches remain candidates because scheduling may convert them. Voter includes
joint and unknown non-learner roles, matching Go `!core.IsLearner`; leader uses
peer ID alone. Witness mismatch requires conversion even when support is disabled.

Fixed-size combinations use colex order, preserving Go numeric-mask tie order
without shifts that overflow at machine-word peer counts. Selection remains
combinatorial, as in Go. Isolation uses canonical store location comparison;
witness load sums store witness counts only for enabled witness rules.
No persistence, I/O, background tasks, time inputs or protocol types are involved.
Multi-rule assignment, RegionFit, replacement and cache belong to later tasks.
Independent tests are explicitly registered in lib.rs. See
[task 26 evidence](../../../docs/rust/schedule/placement/26-证据.md).
