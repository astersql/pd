# Store heartbeat Go oracle probe

Standalone Rust tonic client, run with PD_ORACLE_URL pointing at a disposable Go PD
server. It sends missing-statistics, bootstrap, active, unknown and removed-store
requests. It mutates only the disposable oracle server's data. docs/rust/state/task47/oracle.py
owns server startup, readiness, terminate/wait and directory cleanup. The probe
is not a production server and does not install runtime or global state in libraries.
