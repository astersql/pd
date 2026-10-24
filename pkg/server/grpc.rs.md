# gRPC adapter

Transport boundary for the minimal PD role. Existing GetMembers, AllocID and TSO
behavior remains unchanged. StoreHeartbeat optionally calls the host's explicit
StoreHeartbeatService with losslessly converted domain statistics. The route checks
cluster ID and health, preserves transport versus response-header errors and maps
NodeState values without importing protobuf into the cluster library.
See store_heartbeat_adapter.rs.md for field presence, unsupported controller inputs,
state ownership and lifecycle boundaries. No worker or runtime is created here.
