# Bucket heartbeat adapter

`BucketHeartbeatService` is an optional host-owned domain boundary. Its report and
error types come from the canonical statistics-buckets and cluster crates; the
trait does not expose generated messages. The host calls `handle_bucket_heartbeat`
under its cluster lock and supplies the optional local `BucketWorkerClient`.
That function alone decides admission and queue submission. Missing regions and
Removed leaders never enqueue; equal/stale bucket versions may be applied and
update hot loads while retaining the newer core report snapshot.

`bucket_from_proto` moves region ID, version, ordered binary keys, six independent
counter vectors and the millisecond period. It does not round, rescale, fill absent
arrays, filter old versions or interpret `region_epoch`. Worker input requirements
are preserved: arrays must cover each adjacent key pair; a single key has zero
pairs, and missing stats convert to empty vectors. Zero period normalization to
10000ms belongs to the worker.

`Grpc::report_buckets` consumes the real client-streaming RPC until EOF. Bootstrap
errors return a response header, cluster mismatch returns FailedPrecondition, and
host health failures return Unavailable. Nil buckets and empty keys are ignored.
Domain missing-region errors and applied=false do not end the stream. Reports
produce no success acknowledgement; at EOF tonic clients observe Unknown/EOF as
with the existing Go server. The region epoch has no local admission role.

The RPC future owns reception and in-flight host calls. Host cancellation drops
both, returning Cancelled; client disconnect drops the handler future. There is no
detached worker, private runtime, persistence, scheduling decision, metrics registry
or forwarding topology in this adapter. The host shuts down and joins its worker.
Tests use the real canonical cluster and bucket worker over tonic HTTP/2 loopback,
including wire encode/decode, versions, same-stream continuation and lifecycle.
