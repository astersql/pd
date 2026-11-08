# Classic PD process assembly

The classic CLI supplies its existing external etcd connector. The assembly
validates the advertised addresses, binds the listener, opens the supplied store,
bootstraps the PD identity and starts the canonical runtime. The standalone
signal helper stays with the process assembly.

The binary's Unix test target compiles the same assembly and supplies its own
isolated SQLite/OpenRaft store. That injection is confined to the test harness.
The release CLI selects the external etcd connector.
