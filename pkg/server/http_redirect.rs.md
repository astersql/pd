# HTTP leader forwarding

`RedirectState` borrows the host's `PdServices` and cancellation scope. Middleware
reuses health/leadership validation and canonical MetaStore leader metadata;
protobuf decoding stays inside the server adapter. Local v1 paths and explicit
follower handling bypass the proxy. A nonempty PD-Redirector on another follower
fails before dialing, preventing forwarding cycles.

`wait_for_leader` uses Tokio timers: v1 waits for at most three seconds in the
absence of a leader; v2 returns immediately. A request's outer cancellation scope
covers metadata reads, timer waits, dialing, upload, and waiting for response headers.
The body stream then owns both its cancellation future and Hyper connection driver,
so no detached connection worker survives a dropped request or response.

Paths/query strings and request bodies remain streaming transport data. gzip
responses use a streaming decoder; response status and application headers are
retained, while content encoding and length are recomputed by the downstream
transport. Existing listeners are HTTP; TLS configuration belongs to the separate
TLS integration tasks. The adapter never downgrades an HTTPS endpoint to HTTP.

The independent `http_redirect_test.rs` module validates real two-node runtime
forwarding, shared-etcd transport observations, errors, cancellation and controlled
leader waiting. The read-only Go differential test is opt-in through
`scripts/http-redirect-oracle.py`. See
[task evidence](../../docs/rust/server-api/task-041.md) for scope and limitations.
