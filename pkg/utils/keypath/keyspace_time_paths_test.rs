// Copyright 2026 AsterSQL.
use crate::*;

const GO_ORACLE: &str = concat!(
    "meta_prefix\t0\t0\t\t/pd/0/keyspaces/meta/\n",
    "group_prefix\t0\t0\t\t/pd/0/tso/keyspace_groups/membership/\n",
    "resolved\t0\t0\t\t/pd/0/raft/min_resolved_ts\n",
    "external\t0\t0\t\t/pd/0/raft/external_timestamp\n",
    "meta\t0\t0\t\t/pd/0/keyspaces/meta/00000000\n",
    "group\t0\t0\t\t/pd/0/tso/keyspace_groups/membership/00000\n",
    "timestamp\t0\t0\t\t/pd/0/timestamp\n",
    "meta\t0\t1\t\t/pd/0/keyspaces/meta/00000001\n",
    "group\t0\t1\t\t/pd/0/tso/keyspace_groups/membership/00001\n",
    "timestamp\t0\t1\t\t/ms/0/tso/00001/gta/timestamp\n",
    "meta\t0\t42\t\t/pd/0/keyspaces/meta/00000042\n",
    "group\t0\t42\t\t/pd/0/tso/keyspace_groups/membership/00042\n",
    "timestamp\t0\t42\t\t/ms/0/tso/00042/gta/timestamp\n",
    "meta\t0\t99999\t\t/pd/0/keyspaces/meta/00099999\n",
    "group\t0\t99999\t\t/pd/0/tso/keyspace_groups/membership/99999\n",
    "timestamp\t0\t99999\t\t/ms/0/tso/99999/gta/timestamp\n",
    "meta\t0\t100000\t\t/pd/0/keyspaces/meta/00100000\n",
    "group\t0\t100000\t\t/pd/0/tso/keyspace_groups/membership/100000\n",
    "timestamp\t0\t100000\t\t/ms/0/tso/100000/gta/timestamp\n",
    "meta\t0\t16777215\t\t/pd/0/keyspaces/meta/16777215\n",
    "group\t0\t16777215\t\t/pd/0/tso/keyspace_groups/membership/16777215\n",
    "timestamp\t0\t16777215\t\t/ms/0/tso/16777215/gta/timestamp\n",
    "meta\t0\t99999999\t\t/pd/0/keyspaces/meta/99999999\n",
    "group\t0\t99999999\t\t/pd/0/tso/keyspace_groups/membership/99999999\n",
    "timestamp\t0\t99999999\t\t/ms/0/tso/99999999/gta/timestamp\n",
    "meta\t0\t100000000\t\t/pd/0/keyspaces/meta/100000000\n",
    "group\t0\t100000000\t\t/pd/0/tso/keyspace_groups/membership/100000000\n",
    "timestamp\t0\t100000000\t\t/ms/0/tso/100000000/gta/timestamp\n",
    "meta\t0\t4294967295\t\t/pd/0/keyspaces/meta/4294967295\n",
    "group\t0\t4294967295\t\t/pd/0/tso/keyspace_groups/membership/4294967295\n",
    "timestamp\t0\t4294967295\t\t/ms/0/tso/4294967295/gta/timestamp\n",
    "id\t0\t0\t\t/pd/0/keyspaces/id/\n",
    "status\t0\t0\t\t/pd/0/meta_service_groups//status\n",
    "id\t0\t0\tgroup\t/pd/0/keyspaces/id/group\n",
    "status\t0\t0\tgroup\t/pd/0/meta_service_groups/group/status\n",
    "id\t0\t0\ta/b\t/pd/0/keyspaces/id/a/b\n",
    "status\t0\t0\ta/b\t/pd/0/meta_service_groups/a/b/status\n",
    "id\t0\t0\t/a//../b/\t/pd/0/keyspaces/id//a//../b/\n",
    "status\t0\t0\t/a//../b/\t/pd/0/meta_service_groups//a//../b//status\n",
    "id\t0\t0\t键空间\t/pd/0/keyspaces/id/键空间\n",
    "status\t0\t0\t键空间\t/pd/0/meta_service_groups/键空间/status\n",
    "meta_prefix\t42\t0\t\t/pd/42/keyspaces/meta/\n",
    "group_prefix\t42\t0\t\t/pd/42/tso/keyspace_groups/membership/\n",
    "resolved\t42\t0\t\t/pd/42/raft/min_resolved_ts\n",
    "external\t42\t0\t\t/pd/42/raft/external_timestamp\n",
    "meta\t42\t0\t\t/pd/42/keyspaces/meta/00000000\n",
    "group\t42\t0\t\t/pd/42/tso/keyspace_groups/membership/00000\n",
    "timestamp\t42\t0\t\t/pd/42/timestamp\n",
    "meta\t42\t1\t\t/pd/42/keyspaces/meta/00000001\n",
    "group\t42\t1\t\t/pd/42/tso/keyspace_groups/membership/00001\n",
    "timestamp\t42\t1\t\t/ms/42/tso/00001/gta/timestamp\n",
    "meta\t42\t42\t\t/pd/42/keyspaces/meta/00000042\n",
    "group\t42\t42\t\t/pd/42/tso/keyspace_groups/membership/00042\n",
    "timestamp\t42\t42\t\t/ms/42/tso/00042/gta/timestamp\n",
    "meta\t42\t99999\t\t/pd/42/keyspaces/meta/00099999\n",
    "group\t42\t99999\t\t/pd/42/tso/keyspace_groups/membership/99999\n",
    "timestamp\t42\t99999\t\t/ms/42/tso/99999/gta/timestamp\n",
    "meta\t42\t100000\t\t/pd/42/keyspaces/meta/00100000\n",
    "group\t42\t100000\t\t/pd/42/tso/keyspace_groups/membership/100000\n",
    "timestamp\t42\t100000\t\t/ms/42/tso/100000/gta/timestamp\n",
    "meta\t42\t16777215\t\t/pd/42/keyspaces/meta/16777215\n",
    "group\t42\t16777215\t\t/pd/42/tso/keyspace_groups/membership/16777215\n",
    "timestamp\t42\t16777215\t\t/ms/42/tso/16777215/gta/timestamp\n",
    "meta\t42\t99999999\t\t/pd/42/keyspaces/meta/99999999\n",
    "group\t42\t99999999\t\t/pd/42/tso/keyspace_groups/membership/99999999\n",
    "timestamp\t42\t99999999\t\t/ms/42/tso/99999999/gta/timestamp\n",
    "meta\t42\t100000000\t\t/pd/42/keyspaces/meta/100000000\n",
    "group\t42\t100000000\t\t/pd/42/tso/keyspace_groups/membership/100000000\n",
    "timestamp\t42\t100000000\t\t/ms/42/tso/100000000/gta/timestamp\n",
    "meta\t42\t4294967295\t\t/pd/42/keyspaces/meta/4294967295\n",
    "group\t42\t4294967295\t\t/pd/42/tso/keyspace_groups/membership/4294967295\n",
    "timestamp\t42\t4294967295\t\t/ms/42/tso/4294967295/gta/timestamp\n",
    "id\t42\t0\t\t/pd/42/keyspaces/id/\n",
    "status\t42\t0\t\t/pd/42/meta_service_groups//status\n",
    "id\t42\t0\tgroup\t/pd/42/keyspaces/id/group\n",
    "status\t42\t0\tgroup\t/pd/42/meta_service_groups/group/status\n",
    "id\t42\t0\ta/b\t/pd/42/keyspaces/id/a/b\n",
    "status\t42\t0\ta/b\t/pd/42/meta_service_groups/a/b/status\n",
    "id\t42\t0\t/a//../b/\t/pd/42/keyspaces/id//a//../b/\n",
    "status\t42\t0\t/a//../b/\t/pd/42/meta_service_groups//a//../b//status\n",
    "id\t42\t0\t键空间\t/pd/42/keyspaces/id/键空间\n",
    "status\t42\t0\t键空间\t/pd/42/meta_service_groups/键空间/status\n",
    "meta_prefix\t18446744073709551615\t0\t\t/pd/18446744073709551615/keyspaces/meta/\n",
    "group_prefix\t18446744073709551615\t0\t\t/pd/18446744073709551615/tso/keyspace_groups/membership/\n",
    "resolved\t18446744073709551615\t0\t\t/pd/18446744073709551615/raft/min_resolved_ts\n",
    "external\t18446744073709551615\t0\t\t/pd/18446744073709551615/raft/external_timestamp\n",
    "meta\t18446744073709551615\t0\t\t/pd/18446744073709551615/keyspaces/meta/00000000\n",
    "group\t18446744073709551615\t0\t\t/pd/18446744073709551615/tso/keyspace_groups/membership/00000\n",
    "timestamp\t18446744073709551615\t0\t\t/pd/18446744073709551615/timestamp\n",
    "meta\t18446744073709551615\t1\t\t/pd/18446744073709551615/keyspaces/meta/00000001\n",
    "group\t18446744073709551615\t1\t\t/pd/18446744073709551615/tso/keyspace_groups/membership/00001\n",
    "timestamp\t18446744073709551615\t1\t\t/ms/18446744073709551615/tso/00001/gta/timestamp\n",
    "meta\t18446744073709551615\t42\t\t/pd/18446744073709551615/keyspaces/meta/00000042\n",
    "group\t18446744073709551615\t42\t\t/pd/18446744073709551615/tso/keyspace_groups/membership/00042\n",
    "timestamp\t18446744073709551615\t42\t\t/ms/18446744073709551615/tso/00042/gta/timestamp\n",
    "meta\t18446744073709551615\t99999\t\t/pd/18446744073709551615/keyspaces/meta/00099999\n",
    "group\t18446744073709551615\t99999\t\t/pd/18446744073709551615/tso/keyspace_groups/membership/99999\n",
    "timestamp\t18446744073709551615\t99999\t\t/ms/18446744073709551615/tso/99999/gta/timestamp\n",
    "meta\t18446744073709551615\t100000\t\t/pd/18446744073709551615/keyspaces/meta/00100000\n",
    "group\t18446744073709551615\t100000\t\t/pd/18446744073709551615/tso/keyspace_groups/membership/100000\n",
    "timestamp\t18446744073709551615\t100000\t\t/ms/18446744073709551615/tso/100000/gta/timestamp\n",
    "meta\t18446744073709551615\t16777215\t\t/pd/18446744073709551615/keyspaces/meta/16777215\n",
    "group\t18446744073709551615\t16777215\t\t/pd/18446744073709551615/tso/keyspace_groups/membership/16777215\n",
    "timestamp\t18446744073709551615\t16777215\t\t/ms/18446744073709551615/tso/16777215/gta/timestamp\n",
    "meta\t18446744073709551615\t99999999\t\t/pd/18446744073709551615/keyspaces/meta/99999999\n",
    "group\t18446744073709551615\t99999999\t\t/pd/18446744073709551615/tso/keyspace_groups/membership/99999999\n",
    "timestamp\t18446744073709551615\t99999999\t\t/ms/18446744073709551615/tso/99999999/gta/timestamp\n",
    "meta\t18446744073709551615\t100000000\t\t/pd/18446744073709551615/keyspaces/meta/100000000\n",
    "group\t18446744073709551615\t100000000\t\t/pd/18446744073709551615/tso/keyspace_groups/membership/100000000\n",
    "timestamp\t18446744073709551615\t100000000\t\t/ms/18446744073709551615/tso/100000000/gta/timestamp\n",
    "meta\t18446744073709551615\t4294967295\t\t/pd/18446744073709551615/keyspaces/meta/4294967295\n",
    "group\t18446744073709551615\t4294967295\t\t/pd/18446744073709551615/tso/keyspace_groups/membership/4294967295\n",
    "timestamp\t18446744073709551615\t4294967295\t\t/ms/18446744073709551615/tso/4294967295/gta/timestamp\n",
    "id\t18446744073709551615\t0\t\t/pd/18446744073709551615/keyspaces/id/\n",
    "status\t18446744073709551615\t0\t\t/pd/18446744073709551615/meta_service_groups//status\n",
    "id\t18446744073709551615\t0\tgroup\t/pd/18446744073709551615/keyspaces/id/group\n",
    "status\t18446744073709551615\t0\tgroup\t/pd/18446744073709551615/meta_service_groups/group/status\n",
    "id\t18446744073709551615\t0\ta/b\t/pd/18446744073709551615/keyspaces/id/a/b\n",
    "status\t18446744073709551615\t0\ta/b\t/pd/18446744073709551615/meta_service_groups/a/b/status\n",
    "id\t18446744073709551615\t0\t/a//../b/\t/pd/18446744073709551615/keyspaces/id//a//../b/\n",
    "status\t18446744073709551615\t0\t/a//../b/\t/pd/18446744073709551615/meta_service_groups//a//../b//status\n",
    "id\t18446744073709551615\t0\t键空间\t/pd/18446744073709551615/keyspaces/id/键空间\n",
    "status\t18446744073709551615\t0\t键空间\t/pd/18446744073709551615/meta_service_groups/键空间/status\n",
    "match\t0\t0\t\t<none>\n",
    "match\t0\t0\t00001\t<none>\n",
    "match\t0\t0\ttso/keyspace_groups/membership/00000\t00000\n",
    "match\t0\t0\t/pd/42/tso/keyspace_groups/membership/99999\t99999\n",
    "match\t0\t0\tprefix-tso/keyspace_groups/membership/00042\t00042\n",
    "match\t0\t0\ttso/keyspace_groups/membership/0001\t<none>\n",
    "match\t0\t0\ttso/keyspace_groups/membership/100000\t<none>\n",
    "match\t0\t0\ttso/keyspace_groups/membership/００００１\t<none>\n",
    "match\t0\t0\ttso/keyspace_groups/membership/00001/\t<none>\n",
    "match\t0\t0\ttso/keyspace_groups/membership/+0001\t<none>\n",
    "match\t0\t0\ttso/keyspace_groups/membership/12a45\t<none>\n",
    "match\t0\t0\ttso/keyspace_groups/membership//00001\t<none>\n",
    "match\t0\t0\ttso/keyspace_groups/membership/4294967295\t<none>\n",
    "match\t0\t0\ttso/keyspace_groups/membership/00001/tso/keyspace_groups/membership/00002\t00002\n",
);

#[test]
fn keyspace_time_paths_go_bytes() {
    let mut count = 0;
    for line in GO_ORACLE.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 5);
        let paths = ClusterPaths::new(fields[1].parse().unwrap());
        let id = fields[2].parse().unwrap();
        let value = match fields[0] {
            "meta_prefix" => paths.keyspace_meta_prefix(),
            "group_prefix" => paths.keyspace_group_id_prefix(),
            "meta" => paths.keyspace_meta_path(id),
            "group" => paths.keyspace_group_id_path(id),
            "id" => paths.keyspace_id_path(fields[3]),
            "timestamp" => paths.timestamp_path(id),
            "resolved" => paths.min_resolved_ts_path(),
            "external" => paths.external_timestamp_path(),
            "status" => paths.meta_service_group_status_path(fields[3]),
            "match" => keyspace_group_id_from_path(fields[3])
                .unwrap_or("<none>")
                .to_owned(),
            other => panic!("unknown oracle operation: {other}"),
        };
        assert_eq!(value.as_bytes(), fields[4].as_bytes(), "{line}");
        println!(
            "ORACLE\t{}\t{}\t{}\t{}\t{value}",
            fields[0], fields[1], fields[2], fields[3]
        );
        count += 1;
    }
    assert_eq!(count, 137);
}

#[test]
fn keyspace_time_paths_isolation_and_order() {
    let paths = ClusterPaths::new(42);
    let other = ClusterPaths::new(43);
    let ids = [0, 1, 42, 99999];
    let mut previous = String::new();
    for id in ids {
        let meta = paths.keyspace_meta_path(id);
        assert!(meta.starts_with(&paths.keyspace_meta_prefix()));
        assert!(meta > previous);
        assert_ne!(meta, other.keyspace_meta_path(id));
        previous = meta;
        let group = paths.keyspace_group_id_path(id);
        assert!(group.starts_with(&paths.keyspace_group_id_prefix()));
        assert_eq!(
            keyspace_group_id_from_path(&group).unwrap().parse::<u32>(),
            Ok(id)
        );
        assert_ne!(group, other.keyspace_group_id_path(id));
        assert_ne!(paths.timestamp_path(id), other.timestamp_path(id));
    }
    assert_ne!(paths.keyspace_meta_path(1), paths.keyspace_meta_path(2));
    assert_ne!(paths.timestamp_path(0), paths.timestamp_path(1));
    assert_ne!(paths.timestamp_path(1), paths.timestamp_path(2));
    assert!(keyspace_group_id_from_path(&paths.keyspace_group_id_path(100000)).is_none());
    assert_eq!(
        paths.keyspace_id_path("a/../b"),
        "/pd/42/keyspaces/id/a/../b"
    );
}
