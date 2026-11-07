# pd-recover

`pd-recover` is a disaster recovery tool of PD, used to recover the PD cluster which cannot start or provide services normally.

## Build

1. [Go](https://golang.org/) Version 1.26 or later
2. In the root directory of the [PD project](https://github.com/tikv/pd), use the `make pd-recover` command to compile and generate `bin/pd-recover`.

## Usage

The details about how to use `pd-recover` can be found in [PD Recover User Guide](https://docs.pingcap.com/tidb/dev/pd-recover).

## Rust recovery tool

Build the Rust binary with `cargo build --locked -p astersql-pd-recover --bin pd-recover`.
Use it with the PD processes stopped and the recovery metadata store available.

- To restore an empty cluster, pass `-endpoints`, `-cluster-id`, and `-alloc-id`.
  Values can come from the `clusterID` and `allocIDMax` fields in a backup.
- The Rust tool also accepts `-alloc-timestamp` for the backup's `allocTimestampMax`
  in Unix nanoseconds. It preserves the default group's TSO boundary in the same transaction.
- To recover an existing member's metadata, pass `-from-old-member`. The tool increases the
  stored allocator boundary by 100,000,000 and clears that cluster's store metadata prefix.
- For TLS, pass `-cacert` and, when required, both `-cert` and `-key`.

The Rust tool returns a nonzero exit status for invalid input, identity or boundary conflicts,
and failed recovery. Restart PD after a successful recovery.

An isolated native MetaStore archive can be restored with
`pd-recover --native-restore ARCHIVE NEW_DIRECTORY NEW_CLUSTER_ID NODE_ID`.
The destination directory must not exist and its consensus cluster ID must
differ from the source. The archive is verified before the directory is
created. This operation starts one offline voter and preserves PD identity and
durable allocator and TSO records. Clients must resnapshot their watches and
reacquire leases after restore. See the [native recovery drill](../../docs/rust/production-meta/task-30.md).
