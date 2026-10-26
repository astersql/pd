# Label leader scheduler

`LabelConfig` keeps the Go `ranges` schema through canonical hex-key serialization;
`from_args` reuses query-unescape range decoding (including whole-keyspace CLI default
and ignored trailing unmatched argument). Absent/null JSON ranges remain null on encode.
`LabelScheduler::{new,config,encode_config,is_schedule_allowed,schedule}` is the domain
entry point. Host admission checks leader operator count strictly below the limit.

`label.go` selects stores with any matching reject-leader property, samples one leader
region per source, excludes down/pending followers, applies MEDIUM StoreStateFilter,
and randomly picks one remaining follower. The real PeerOperatorFactory performs
placement and role checks. Successful output has LEADER kind, MEDIUM priority,
`label-reject-leader` description and one TransferLeader with no alternative targets.
A builder failure terminates the call; its typed error is returned for host diagnostics.
No-target/no-region advances to the next source. Source transfer-out pause is deliberately
not checked, matching Go. At most one operator is emitted; this scheduler owns no cache.

Go map order is unspecified: source and candidate IDs are sorted to make caller-owned
random draws reproducible. This constrains unspecified order, not the feasible target
set. Clock and random sampling are explicit borrowed inputs through the existing
EvictLeaderContext; there is no private runtime. Registry, metrics publication, HTTP,
operator-controller admission and delivery remain host/facade responsibilities.
No new storage operation or reload behavior is added (Go explicitly leaves reload TODO).

Evidence is **read-only Go source-derived**, under the user's explicit authorization;
no Go runtime oracle or cross-language runtime differential is claimed. Sources include
label.go, init.go, scheduler_test.go `TestRejectLeader`, core GetFollowers and builder
role/placement checks. Witness followers are selectable but cannot lead: choosing one
fails the builder and ends this attempt. Learners are absent from the follower set.
[Task evidence](../../../docs/rust/schedule-execution/task-097.md) records 40 table-driven
behavior cases, real heartbeat/influence/completion assertions, malformed configuration,
leader-in depletion, placement error, mutation red evidence and full regression checks.
