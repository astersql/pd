# Isolation and replacement

`isolation_store_score(source, destination, stores, labels)` computes pairwise
location diversity with each source-store occurrence replaced by destination.
It uses canonical `StoreInfo::compare_location`; the first differing label at
index i contributes `100^(labels.len()-i-1)`. Empty labels and fewer than two
stores score zero. Missing labels use core comparison semantics. No snapshots
are mutated and no health, witness or isolation-level veto is added here.

`RegionFit::replace` rejects a missing destination or unassigned source first.
Destination membership in the same rule, and membership in a different rule with
the same role, allow an exchange before checking constraints or diversity.
Other destinations must match the source rule's labels (including exclusive
labels) and preserve or improve its total pairwise score; equality is allowed.
This matches Go `RegionFit.Replace`, including leader transfer across voter rules.

`get_rule_fit` searches selected peer IDs, excluding orphans. `extra_count`
subtracts desired rule counts from known region-store occurrences and preserves
negative shortages. `region_stores` and `rules` expose immutable ordered slices.
The Rust `fit_region` retains these inputs itself, combining Go's internal
`fitRegion` result with the metadata attached by `RuleManager.FitRegion`.
Region stores follow peer order, exclude unknown stores and unrelated input
stores, and preserve repeated occurrences. All snapshots use canonical core and
Rule types; no protocol types, persistence, background tasks or I/O are involved.

Go's `IsolationLevel` is not consumed in fit.go; scheduler callers enforce that
separate constraint. Isolation, witness, role and health ordering remain in the
existing fit module. See [task 28 evidence](../../../docs/rust/schedule/placement/28-证据.md).
