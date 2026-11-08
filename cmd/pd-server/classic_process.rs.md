# Classic PD process assembly

The assembly validates addresses, binds the listener, opens the supplied
MetaStore, bootstraps PD identity and starts the canonical runtime. The
standalone process owns signal handling.

The default connector uses external etcd. The paired local native flags
select an isolated, loopback-only SQLite/OpenRaft store with one voter.
The standalone adapter closes that store after the shared assembly stops.
The embedded test supplies the same storage adapter to the caller-owned
runtime.
[Local topology and invocation](../../docs/rust/server-api/acceptance/native-topology.md).
