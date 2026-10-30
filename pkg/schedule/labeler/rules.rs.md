# Region label rule model

Canonical crate: `astersql-pd-schedule-labeler`, exported by [lib.rs](lib.rs).
Go oracle: [rules.go](rules.go), [rule_test.go](rule_test.go), and
[TestAdjustRule/TestAdjustRule2](labeler_test.go). Runtime evidence and source
inventory: [task 14](../../../docs/rust/schedule/labeler/14-证据.md).

Public API:

- `KEY_RANGE`: supported rule type `key-range`.
- `Timestamp`: signed Unix nanoseconds (`i128`), explicit deterministic clock input.
- `RegionLabel::{expires_at,expire_before,check_and_adjust_expire}`; public JSON
  key/value/ttl/start_at fields, private derived expiry; Display matches Go.
- `LabelRule::{from_json_without_check,from_json,check_and_adjust,key_ranges,
  min_expire,expire_before,check_and_remove_expire_labels}`; public JSON fields,
  private derived ranges and minimum expiry.
- `KeyRangeRule`: decoded byte endpoints plus original case-preserving hex strings;
  byte endpoints are excluded from JSON.
- `RuleError::{Json,Domain}` and `RuleError::code`: JSON errors have no PD domain
  code; validation uses the existing region-content or hex-decoding catalog.

Validation order follows Go: id, labels, key/value/TTL, removal of strictly expired
labels, supported rule type, data shape, start/end hex, increasing byte boundaries.
An empty end key is unbounded. Range and label order are preserved; no sorting or
merging. After validation, data is replaced with the canonical KeyRangeRule JSON
(start_key/end_key only), discarding unknown range fields as Go does while
preserving original hex spelling. Task070 adds a real HTTP differential and
independent regression for this persistence boundary. Unknown JSON fields are ignored; object keys match without ASCII case,
null scalars preserve defaults/prior values, duplicate fields are consumed in order.
`from_json_without_check` leaves all derived state absent. Empty label collections
use a Rust Vec; unchecked serialization emits an empty array rather than Go's nil
slice null. Checked rules always contain labels; cleanup produces an empty array.

TTL reuses canonical typeutil signed Go Duration. Auto start retains input nanoseconds
for expiry and formats UTC UnixDate at second precision, so reloading JSON can lose
less than one second, as Go does. Hosts supply UTC timestamps rather than installing
process-local timezone state. Explicit UnixDate accepts Go's named and fabricated
zones, fractional seconds, and Gregorian date validation. Generic unknown zone names
have offset zero; GMT hour offsets shift the instant. Weekday names are validated
but are not cross-checked against the date, matching Go time.Parse. Error categories
are stable; duration/date diagnostic wording remains Rust-specific.

Expiry comparisons are strict `<`; no TTL means immortal even with a malformed
start_at. Cleanup preserves label order and recalculates minimum expiry. No storage,
protocol, global clock, runtime, background job or metrics is introduced here. Subsequent
index, storage, transaction, resolution and GC work consumes this single domain model.
Chrono has only its std feature for calendar operations, with no ambient clock.
Tests live separately in [label_rule_test.rs](label_rule_test.rs), explicitly registered
by lib.rs. No Go files or modules are changed.
