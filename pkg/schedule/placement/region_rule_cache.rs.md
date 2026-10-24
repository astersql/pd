# Placement fit cache

`RegionRuleFitCacheManager::{new,check_and_get_cache,set_cache,invalid,remove_store_cache}`
implements Go's two-stage candidate/promotion contract. First insertion has zero
hits and no result; the tenth subsequent set publishes an immutable `Arc<RegionFit>`.
The check boolean describes candidate freshness independently of result promotion.
Rule identity uses group/id/version/create timestamp; store identity uses ID,
legacy state and order-independent labels; region identity uses epoch and leader
store ID. Peer topology changes follow Go's epoch contract (conf_ver must advance).
The cache does not invent a different raw-peer comparison.

`validate_region`, `validate_stores`, `validate_fit` preserve Go eligibility.
An explicit Unix-nanosecond time argument feeds core's disconnected predicate;
no internal clock, runtime or background worker exists. Pending peers alone do
not reject caching. Snapshot health and freshly supplied current-store liveness
are separate checks. Current stores must come from the host's current cluster
state; stale fit stores cannot establish/promote an entry after removal.

`RuleManager::{fit_region,fit_region_without_cache,is_region_fit_cached,
set_region_fit_cache,invalid_cache,remove_store_cache}` combines its canonical
rule list, config provider, core snapshots and real fit algorithm. First insertion
is explicitly requested, as in Go; unchanged candidates advance during fits.
Disabled caching bypasses lookup and promotion. Rule mutations change version/time
snapshots and therefore prevent reuse. Host mutation serialization is expressed
by `&mut self`; the host must serialize current-store snapshot acquisition and
store removal notifications with these calls. No independent cache mutex can
make an externally stale cluster snapshot fresh.

Unlike Go's shared store-cache/reverse-index optimization, entries retain immutable
canonical store snapshots and remove-store scans entries. This avoids a second
mutable store identity table while guaranteeing the same eviction, including
candidate and promoted fits. Removal is O(cached regions); checks remain local to
one region. Shared store pointer identity is not a Rust public contract.
No protocol types, I/O, persistence keys, global state or additional dependencies
are introduced. Tests and Go run provenance: [task 29 evidence](../../../docs/rust/schedule/placement/29-证据.md).
