# rule_manager.rs

`RuleManager` owns canonical `RuleConfig` and `RuleList`, an `Arc<StorageEndpoint>` and
caller-owned `ClusterPaths`. Mutations take `&mut self`; hosts serialize access rather
than holding an internal lock across asynchronous I/O. No private runtime, global
state, system clock, signals or background tasks are installed.

Public API: `new`, `set_key_type`, `is_initialized`, `config`, `rule_list`, `get_rule`,
`adjust_rule`, `initialize`, `set_rule`, `delete_rule`; errors are `RuleManagerError`.
`SharedConfigProvider` supplies witness policy and maximum replica count at each
operation. Optional current `StoreInfo` slices enforce real label matching. Key mode
is explicitly set, matching Go SetKeyType; decoded bytes use the canonical codec.
Rule/configuration public values never contain protobuf types.

Initialize is idempotent. skipLoadRules marks initialized without storage I/O and
leaves an empty list (`None`). Normal loading validates rules, retains the first
logical key in storage byte order, migrates historical suffixes to StoreKey, deletes
invalid/duplicate entries, then loads valid groups. Repairs preserve arbitrary raw
key bytes and use the Go limit of 120 operations per transaction, saves before deletes.
Invalid groups are ignored and retained in storage. Empty rule storage migrates the
replication count, labels and isolation policy to pd/default and optional pd/witness.
Like Go, initialization can retain partially loaded state on failure; retry does not
provide a new atomic snapshot. Default rules enter memory before transaction commit.

Single changes validate coverage before persistence, trim identical JSON mutations,
and publish only after storage success. The caller supplies Unix seconds. Go saves
incoming version/create_timestamp before incrementing the live version, so restart
restores the saved incoming fields; this observable behavior is intentionally retained.
After commit the immutable rule list is rebound to the committed rule snapshots.
Deleting the last covering voter fails validation; deleting a missing rule is a no-op.
Errors distinguish invalid content, hex, rule-list validation, endpoint and transaction
failures. No storage retries hide indeterminate backend outcomes.

The storage endpoint transitively references protocol adapters, but this module only
uses its generic JSON RuleStorage operations and backend-neutral MetaStore transaction.
See [task 24 evidence](../../../docs/rust/schedule/placement/24-证据.md) and the explicitly
registered independent `placement_restore_test.rs` tests.

Bulk HTTP uses `prepare_patch` to validate/trim an owned canonical patch and freeze
persistence values separately from committed runtime versions. `PreparedRulePatch::persist`
uses one caller-supplied fenced StorageEndpoint transaction, then `publish_patch` replaces
the configuration and list without IO. Backend capacity errors reject the entire update.
The existing domain batch APIs retain their Go-compatible chunking behavior.
