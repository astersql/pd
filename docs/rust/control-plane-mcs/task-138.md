# Task138: classic ResourceManager conditional assembly

Status: complete for acceptance16's local RM/proxy assembly and host-ownership boundary.

[Raw receipt](task-138.json.gz); [current dual-mode receipt](task-148.md); [method index](task-148-methods.json).

The same PD listener registers one ResourceManager. Fallback true binds the canonical term-owned RM; false binds the shared proxy and initializes the PD metadata-only manager. List/Get/token requests select RM primary; Add/Modify/Delete validate and select PD leader. Standalone RM rejects metadata mutation instead of persisting PD-owned settings.

Current real two-PD fixtures execute add/modify/delete through an actual follower, valid leader forwarded-host and forged follower target, for both fallback values. Each request set runs on Go processes, Rust processes and caller-runtime Rust PdNodeBuilder hosts. Original settings bytes/create/modrevision/version/lease are retained; rejected forwarding preserves them. PD metadata-only roles do not create token state. Closing embedded nodes leaves the caller runtime/token alive.

The standalone RM TLS request set also runs on Go/Rust processes and the genuine ResourceManagerHost supplied by the caller. It reads the actual group, rejects all three metadata writes, grants5 tokens, returns the actual99995 initial-pool balance, half-closes with EOF and rejects missing/denied/rogue client identities. The bounded TLS case does not pretend that graceful exit forces a new state save before Go's60-second persistence tick.

The current targeted `mcs_138_resource_groups_are_term_owned_and_periodically_durable` run passed1/0/0. It asserted200000 initial tokens,10000 consumed,190000 periodically persisted and restored after leader handoff; it also checked a retained token stream closes and the parent runtime survives. The receipt retains the immutable older run separately. Source comparison found assembly/state changes after that older receipt, so this one relevant persistence check was rerun rather than treating an old result as current. Two-instance/startup failure/caller ownership assertions from the canonical embedded isolation tests remain scoped to their actual runs.

All callers use the canonical managers, proxy connection pool, shared listener and supplied executor/backend. No second token algorithm, fake PD participant, protocol bridge or private runtime is introduced. Native Rust production durability and full global shutdown/configuration/platform regressions remain with acceptance17-24.
