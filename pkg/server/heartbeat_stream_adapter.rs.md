# Heartbeat command transport

`CommandStream` adapts domain commands and header-only keepalive to a bounded
channel owned by the actual tonic RegionHeartbeat response stream. A send waits
at most five seconds and also observes stream cancellation. Conversion through
`schedule_wire::to_pdpb` stays in the server layer.

The gRPC adapter polls incoming reports and outgoing commands together. It binds
only a valid, healthy store report through the host's routing capability. Input
EOF cancels outgoing delivery; dropping the response closes its receiver. New
streams replace bindings in the canonical serial router. Errors keep their
existing wire headers and cluster-ID validation precedes binding.

The runtime additionally binds sends to their leader term so a retired leader
cannot continue publishing on a listener that remains open. Tests in runtime
task 017 use real TCP for command delivery, stream replacement, stale reports and
shutdown; the controlled-clock test exercises the same domain routing contract.
