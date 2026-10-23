// Copyright 2026 AsterSQL.
use super::*;

// Captured from: go run ./scripts/rust-theme3-oracle cluster_paths
// Go keypath sources at cf27d6730; no normalization of key bytes.
const GO_ORACLE: &str = r#"ORACLE	cluster_id	/pd/cluster_id
ORACLE	0	cluster	/pd/0/raft
ORACLE	0	bootstrap	/pd/0/raft/status/raft_bootstrap_time
ORACLE	0	alloc	/pd/0/alloc_id
ORACLE	0	keyspace_alloc	/pd/0/keyspaces/alloc_id
ORACLE	0	prefix	/pd/0/raft/s/
ORACLE	0	store	0	/pd/0/raft/s/00000000000000000000
ORACLE	0	region	0	/pd/0/raft/r/00000000000000000000
ORACLE	0	store	1	/pd/0/raft/s/00000000000000000001
ORACLE	0	region	1	/pd/0/raft/r/00000000000000000001
ORACLE	0	store	42	/pd/0/raft/s/00000000000000000042
ORACLE	0	region	42	/pd/0/raft/r/00000000000000000042
ORACLE	0	store	9999999999999999999	/pd/0/raft/s/09999999999999999999
ORACLE	0	region	9999999999999999999	/pd/0/raft/r/09999999999999999999
ORACLE	0	store	10000000000000000000	/pd/0/raft/s/10000000000000000000
ORACLE	0	region	10000000000000000000	/pd/0/raft/r/10000000000000000000
ORACLE	0	store	18446744073709551615	/pd/0/raft/s/18446744073709551615
ORACLE	0	region	18446744073709551615	/pd/0/raft/r/18446744073709551615
ORACLE	42	cluster	/pd/42/raft
ORACLE	42	bootstrap	/pd/42/raft/status/raft_bootstrap_time
ORACLE	42	alloc	/pd/42/alloc_id
ORACLE	42	keyspace_alloc	/pd/42/keyspaces/alloc_id
ORACLE	42	prefix	/pd/42/raft/s/
ORACLE	42	store	0	/pd/42/raft/s/00000000000000000000
ORACLE	42	region	0	/pd/42/raft/r/00000000000000000000
ORACLE	42	store	1	/pd/42/raft/s/00000000000000000001
ORACLE	42	region	1	/pd/42/raft/r/00000000000000000001
ORACLE	42	store	42	/pd/42/raft/s/00000000000000000042
ORACLE	42	region	42	/pd/42/raft/r/00000000000000000042
ORACLE	42	store	9999999999999999999	/pd/42/raft/s/09999999999999999999
ORACLE	42	region	9999999999999999999	/pd/42/raft/r/09999999999999999999
ORACLE	42	store	10000000000000000000	/pd/42/raft/s/10000000000000000000
ORACLE	42	region	10000000000000000000	/pd/42/raft/r/10000000000000000000
ORACLE	42	store	18446744073709551615	/pd/42/raft/s/18446744073709551615
ORACLE	42	region	18446744073709551615	/pd/42/raft/r/18446744073709551615
ORACLE	18446744073709551615	cluster	/pd/18446744073709551615/raft
ORACLE	18446744073709551615	bootstrap	/pd/18446744073709551615/raft/status/raft_bootstrap_time
ORACLE	18446744073709551615	alloc	/pd/18446744073709551615/alloc_id
ORACLE	18446744073709551615	keyspace_alloc	/pd/18446744073709551615/keyspaces/alloc_id
ORACLE	18446744073709551615	prefix	/pd/18446744073709551615/raft/s/
ORACLE	18446744073709551615	store	0	/pd/18446744073709551615/raft/s/00000000000000000000
ORACLE	18446744073709551615	region	0	/pd/18446744073709551615/raft/r/00000000000000000000
ORACLE	18446744073709551615	store	1	/pd/18446744073709551615/raft/s/00000000000000000001
ORACLE	18446744073709551615	region	1	/pd/18446744073709551615/raft/r/00000000000000000001
ORACLE	18446744073709551615	store	42	/pd/18446744073709551615/raft/s/00000000000000000042
ORACLE	18446744073709551615	region	42	/pd/18446744073709551615/raft/r/00000000000000000042
ORACLE	18446744073709551615	store	9999999999999999999	/pd/18446744073709551615/raft/s/09999999999999999999
ORACLE	18446744073709551615	region	9999999999999999999	/pd/18446744073709551615/raft/r/09999999999999999999
ORACLE	18446744073709551615	store	10000000000000000000	/pd/18446744073709551615/raft/s/10000000000000000000
ORACLE	18446744073709551615	region	10000000000000000000	/pd/18446744073709551615/raft/r/10000000000000000000
ORACLE	18446744073709551615	store	18446744073709551615	/pd/18446744073709551615/raft/s/18446744073709551615
ORACLE	18446744073709551615	region	18446744073709551615	/pd/18446744073709551615/raft/r/18446744073709551615
ORACLE	parse		syntax
ORACLE	parse	0	syntax
ORACLE	parse	000	syntax
ORACLE	parse	123	123
ORACLE	parse	000123	123
ORACLE	parse	18446744073709551615	18446744073709551615
ORACLE	parse	18446744073709551616	range
ORACLE	parse	+1	syntax
ORACLE	parse	-1	syntax
ORACLE	parse	 1	syntax
ORACLE	parse	1_0	syntax
ORACLE	parse	１２	syntax
ORACLE	parse	12x	syntax
ORACLE	parse	/pd/42/raft/s/00000000000000000000	syntax
ORACLE	parse	/pd/42/raft/s/00000000000000000042	42
ORACLE	parse	/pd/43/raft/s/00000000000000000042	syntax
ORACLE	parse	/pd/42/raft/s/18446744073709551616	range
ORACLE	parse	18446744073709551616x	range
"#;

#[test]
fn cluster_paths_go_bytes() {
    for line in GO_ORACLE
        .lines()
        .filter(|line| !line.starts_with("ORACLE\tparse\t"))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let actual = if fields[1] == "cluster_id" {
            CLUSTER_ID_PATH.to_owned()
        } else {
            let paths = ClusterPaths::new(fields[1].parse().unwrap());
            assert_eq!(paths.cluster_id().to_string(), fields[1]);
            match fields[2] {
                "cluster" => paths.cluster_path(),
                "bootstrap" => paths.cluster_bootstrap_time_path(),
                "alloc" => paths.alloc_id_path(),
                "keyspace_alloc" => paths.keyspace_alloc_id_path(),
                "prefix" => paths.store_path_prefix(),
                "store" => paths.store_path(fields[3].parse().unwrap()),
                "region" => paths.region_path(fields[3].parse().unwrap()),
                other => panic!("unexpected oracle field {other}"),
            }
        };
        assert_eq!(
            actual.as_bytes(),
            fields.last().unwrap().as_bytes(),
            "{line}"
        );
        println!("{line}");
    }
}

#[test]
fn cluster_paths_go_store_parsing() {
    let paths = ClusterPaths::new(42);
    for line in GO_ORACLE
        .lines()
        .filter(|line| line.starts_with("ORACLE\tparse\t"))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let expected = match fields[3] {
            "syntax" => Err(StoreIdError::InvalidSyntax),
            "range" => Err(StoreIdError::OutOfRange),
            value => Ok(value.parse().unwrap()),
        };
        assert_eq!(
            paths.extract_store_id_from_path(fields[2]),
            expected,
            "{line}"
        );
        println!("{line}");
    }
}

#[test]
fn cluster_paths_independent_contexts() {
    let first = ClusterPaths::new(1);
    let second = ClusterPaths::new(10);
    assert_eq!(first.cluster_path(), "/pd/1/raft");
    assert_eq!(second.cluster_path(), "/pd/10/raft");
    assert_eq!(first.store_path(42), "/pd/1/raft/s/00000000000000000042");
    assert!(
        second
            .extract_store_id_from_path(&first.store_path(42))
            .is_err()
    );
    assert_eq!(first.cluster_id(), 1);
}
