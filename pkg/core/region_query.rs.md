# Region query and heartbeat counters

`region_query.rs` owns QueryStats merge/reset and RegionInfo load projections.
`report_interval()` returns the exact start/end second counters set by
`set_report_interval`; consumers retain Go unsigned wrapping subtraction.
Task 23 exercises round trips and compact heartbeat capture through the real
core RegionInfo. No protocol or runtime dependency is introduced.
