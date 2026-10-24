# Bucket stream oracle probe

Test/evidence example for a disposable existing Go PD binary. The task49 oracle
Python driver starts that binary with isolated data, executes this tonic probe,
then terminates/waits the server and removes data. Run under the shared Cargo-slot
wrapper; no Go source, module or injected code is created or changed.

Bootstrap and region heartbeat establish region1/peer2/store1. ReportBuckets sends
missing region99, v3, stale v2, duplicate v3, EOF v4 and wrong-cluster v5. The probe
asserts no per-report response, Unknown/EOF at client close, FailedPrecondition for
cluster mismatch, and NotBootstrapped before bootstrap. HTTP snapshots expose hot
bucket boundaries and rates once the handler's hot-degree threshold is reached.
GetRegionByID is printed with an explicit limitation: default store bucket feature
disables its buckets field, so it cannot establish report version in this probe.
Existing task45 runtime evidence establishes version/CAS/Removed admission.
