# Request context

`request_context::current()` returns a clone of the metadata for the currently
polled HTTP/gRPC adapter or response body. The router also inserts it into request
extensions. Values do not leak to unrelated tasks; spawned work must explicitly
carry the context/token and remain owned and joined by its host.

HTTP source IP/port follows Go `GetIPPortFromHTTPRequest`: first forwarded-for
value, then real-IP, then listener peer; a parsed host:port overrides the forwarded
port. Proxy text is diagnostic and is not validated as an authorization identity.
All forwarded-for fields remain available in wire order. HTTP caller-ID falls
back to component, then anonymous. Registered HTTP handler names supply service
labels. gRPC uses its method path and real socket peer; `pd-forwarded-host` is
recorded without changing routing/admission. Its protobuf caller ID is distinct
from HTTP caller headers and is not inferred from them.

An opaque printable X-Request-ID of at most 128 bytes is retained for correlation;
otherwise a router-local sequence with process/time prefix is generated. This is
an explicitly Rust diagnostic extension: the referenced Go RequestInfo has no
request-ID field. It is not an authentication token and is not echoed in public
responses. No body is consumed, buffered or logged by this middleware.

The cancellation token is a child of host cancellation. A drop guard spans the
handler future and every response data/trailer frame. Completion, disconnect or
future cancellation cancels it. Streaming work is polled under the same task-local
scope after response headers, and existing adapter-owned futures are dropped on
disconnect. The middleware preserves gRPC trailers and existing error semantics;
it does not install a new global logger, runtime, worker or authorization policy.

See [task-040](../../docs/rust/server-api/task-040.md) for current transport,
persistence, cancellation and read-only Go differential evidence.
