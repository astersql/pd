# Server public surface

PdServices supplies identity, health, ID and TSO operations. The default
store_heartbeat_service accessor returns None, retaining opt-in host ownership for
the state service. Hosts may implement the domain-only StoreHeartbeatService and
use apply_store_heartbeat with their existing canonical cluster and statistics.
router composes HTTP and gRPC under one host cancellation scope. Production and
separate test modules, including store_heartbeat_adapter_test, are explicitly registered.
