# Canonical cluster handles

BasicCluster composes the canonical store registry and region indexes. Cloned
values share both collections; they do not freeze or copy a heartbeat snapshot.
Each mutable region handle serializes index operations through RegionsInfo.
Runtime pipelines retain admission control for multi-step heartbeat effects,
while Coordinator and policy readers observe the same accepted payloads.

See [shared index ownership](regions_shared.rs.md) and
[task 017 evidence](../../docs/rust/server-api/task-017.md).
