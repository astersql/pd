# Placement fit

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
Replacement and fit metadata queries are documented in [fit_isolation.rs.md](fit_isolation.rs.md). Cache remains a later task.
Independent tests are explicitly registered in lib.rs. See
[task 26 evidence](../../../docs/rust/schedule/placement/26-证据.md).

`fit_region(stores, region, rules, support_witness)` preserves the caller's rule
order and returns `RegionFit` with rule fits, orphan peers and retained region-store/rule snapshots. Its `is_satisfied`
requires a nonempty rule list, all rule fits satisfied, and no orphans.
The worker sorts all peers by health then ID, filters each rule's unselected
candidates, and explores fixed-count combinations in Go numeric-mask order.
Single-rule assembly and comparison are shared with `fit_rule`.

Better rule fits reset the best suffix; equal fits recurse into the next rule
and replace the current assignment only if that suffix improves. Earlier rules
therefore dominate later rules lexicographically. Peers are marked for recursion
and unmarked on return. Orphans are captured from the winning complete selection,
including unknown-store peers. Exact ties retain the first combination. With no
location labels, Go's satisfied-fit early exit stops combination traversal too;
location labels keep exhaustive search enabled for later isolation improvements.
The iterator avoids machine-word mask overflow; complexity remains combinatorial.
No rules produces sorted orphan peers and an unsatisfied fit. Rules are expected
to be validated by the existing rule manager (including positive counts).
See [task 27 evidence](../../../docs/rust/schedule/placement/27-证据.md) and
explicitly registered `placement_region_fit_test.rs`.

RegionFit retains whether the orphan list has ever been populated during combination
search (`orphan_peers_is_nil`). This preserves the Go distinction between nil and an
allocated list subsequently cleared, without adding transport or JSON types to the
fitting domain. The HTTP detail adapter maps empty RuleFit peer lists to null and
uses this provenance for orphan-peers; see task-069 HTTP/Go differential evidence.
