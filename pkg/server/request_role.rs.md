# Request role admission

`validate` shares the leader-required request ordering from Go
`validateRoleInRequest`: stopped scope, current host/lease health, then cluster ID.
A missing header has cluster ID zero. The adapter returns tonic transport errors;
bootstrap absence is instead represented by `ResponseHeader.Error`.

The runtime supplies canonical bootstrap state by reading existing cluster metadata
from MetaStore. It retains a weak store reference so a stopped node does not keep
storage alive. Unary metadata reads check admission before bootstrap; StoreHeartbeat
also requires stats before bootstrap. RegionHeartbeat and ReportBuckets deliberately
check bootstrap before role/header admission, as their Go methods do.

TSO retains its distinct identity-before-allocator path. This module owns no domain
state, workers, runtime or forwarding policy. See
[task 018 evidence](../../docs/rust/server-api/task-018.md).
