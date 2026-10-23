// Copyright 2026 AsterSQL.
use crate::{ClusterPaths, MsParam, prefix};

// Actual output of go run ./scripts/rust-theme3-oracle member_config_paths.
const GO_ORACLE: &str = r#"ORACLE	0	config		/pd/0/config
ORACLE	0	scheduler_prefix		/pd/0/scheduler_config/
ORACLE	0	middleware		/pd/0/service_middleware
ORACLE	0	election_nil		/pd/0/leader
ORACLE	0	scheduler		/pd/0/scheduler_config/
ORACLE	0	replication		/pd/0/replication_mode/
ORACLE	0	scheduler	balance-region	/pd/0/scheduler_config/balance-region
ORACLE	0	replication	balance-region	/pd/0/replication_mode/balance-region
ORACLE	0	scheduler	nested/name	/pd/0/scheduler_config/nested/name
ORACLE	0	replication	nested/name	/pd/0/replication_mode/nested/name
ORACLE	0	scheduler	/a//../b/	/pd/0/scheduler_config//a//../b/
ORACLE	0	replication	/a//../b/	/pd/0/replication_mode//a//../b/
ORACLE	0	scheduler	调度	/pd/0/scheduler_config/调度
ORACLE	0	replication	调度	/pd/0/replication_mode/调度
ORACLE	0	deploy	0	/pd/0/member/0/deploy_path
ORACLE	0	git_hash	0	/pd/0/member/0/git_hash
ORACLE	0	version	0	/pd/0/member/0/binary_version
ORACLE	0	priority	0	/pd/0/member/0/leader_priority
ORACLE	0	leader_weight	0	/pd/0/schedule/store_weight/00000000000000000000/leader
ORACLE	0	region_weight	0	/pd/0/schedule/store_weight/00000000000000000000/region
ORACLE	0	deploy	1	/pd/0/member/1/deploy_path
ORACLE	0	git_hash	1	/pd/0/member/1/git_hash
ORACLE	0	version	1	/pd/0/member/1/binary_version
ORACLE	0	priority	1	/pd/0/member/1/leader_priority
ORACLE	0	leader_weight	1	/pd/0/schedule/store_weight/00000000000000000001/leader
ORACLE	0	region_weight	1	/pd/0/schedule/store_weight/00000000000000000001/region
ORACLE	0	deploy	18446744073709551615	/pd/0/member/18446744073709551615/deploy_path
ORACLE	0	git_hash	18446744073709551615	/pd/0/member/18446744073709551615/git_hash
ORACLE	0	version	18446744073709551615	/pd/0/member/18446744073709551615/binary_version
ORACLE	0	priority	18446744073709551615	/pd/0/member/18446744073709551615/leader_priority
ORACLE	0	leader_weight	18446744073709551615	/pd/0/schedule/store_weight/18446744073709551615/leader
ORACLE	0	region_weight	18446744073709551615	/pd/0/schedule/store_weight/18446744073709551615/region
ORACLE	0	election	|0	/pd/0/leader
ORACLE	0	expected	|0	/ms/0//primary/expected_primary
ORACLE	0	election	|1	/pd/0/leader
ORACLE	0	expected	|1	/ms/0//primary/expected_primary
ORACLE	0	election	|99999	/pd/0/leader
ORACLE	0	expected	|99999	/ms/0//primary/expected_primary
ORACLE	0	election	|100000	/pd/0/leader
ORACLE	0	expected	|100000	/ms/0//primary/expected_primary
ORACLE	0	election	|4294967295	/pd/0/leader
ORACLE	0	expected	|4294967295	/ms/0//primary/expected_primary
ORACLE	0	election	tso|0	/ms/0/tso/00000/primary
ORACLE	0	expected	tso|0	/ms/0/tso/00000/primary/expected_primary
ORACLE	0	election	tso|1	/ms/0/tso/keyspace_groups/election/00001/primary
ORACLE	0	expected	tso|1	/ms/0/tso/keyspace_groups/election/00001/primary/expected_primary
ORACLE	0	election	tso|99999	/ms/0/tso/keyspace_groups/election/99999/primary
ORACLE	0	expected	tso|99999	/ms/0/tso/keyspace_groups/election/99999/primary/expected_primary
ORACLE	0	election	tso|100000	/ms/0/tso/keyspace_groups/election/100000/primary
ORACLE	0	expected	tso|100000	/ms/0/tso/keyspace_groups/election/100000/primary/expected_primary
ORACLE	0	election	tso|4294967295	/ms/0/tso/keyspace_groups/election/4294967295/primary
ORACLE	0	expected	tso|4294967295	/ms/0/tso/keyspace_groups/election/4294967295/primary/expected_primary
ORACLE	0	election	scheduling|0	/ms/0/scheduling/primary
ORACLE	0	expected	scheduling|0	/ms/0/scheduling/primary/expected_primary
ORACLE	0	election	scheduling|1	/ms/0/scheduling/primary
ORACLE	0	expected	scheduling|1	/ms/0/scheduling/primary/expected_primary
ORACLE	0	election	scheduling|99999	/ms/0/scheduling/primary
ORACLE	0	expected	scheduling|99999	/ms/0/scheduling/primary/expected_primary
ORACLE	0	election	scheduling|100000	/ms/0/scheduling/primary
ORACLE	0	expected	scheduling|100000	/ms/0/scheduling/primary/expected_primary
ORACLE	0	election	scheduling|4294967295	/ms/0/scheduling/primary
ORACLE	0	expected	scheduling|4294967295	/ms/0/scheduling/primary/expected_primary
ORACLE	0	election	tso/|0	/ms/0/tso//primary
ORACLE	0	expected	tso/|0	/ms/0/tso//primary/expected_primary
ORACLE	0	election	tso/|1	/ms/0/tso//primary
ORACLE	0	expected	tso/|1	/ms/0/tso//primary/expected_primary
ORACLE	0	election	tso/|99999	/ms/0/tso//primary
ORACLE	0	expected	tso/|99999	/ms/0/tso//primary/expected_primary
ORACLE	0	election	tso/|100000	/ms/0/tso//primary
ORACLE	0	expected	tso/|100000	/ms/0/tso//primary/expected_primary
ORACLE	0	election	tso/|4294967295	/ms/0/tso//primary
ORACLE	0	expected	tso/|4294967295	/ms/0/tso//primary/expected_primary
ORACLE	0	election	/x//../y/|0	/ms/0//x//../y//primary
ORACLE	0	expected	/x//../y/|0	/ms/0//x//../y//primary/expected_primary
ORACLE	0	election	/x//../y/|1	/ms/0//x//../y//primary
ORACLE	0	expected	/x//../y/|1	/ms/0//x//../y//primary/expected_primary
ORACLE	0	election	/x//../y/|99999	/ms/0//x//../y//primary
ORACLE	0	expected	/x//../y/|99999	/ms/0//x//../y//primary/expected_primary
ORACLE	0	election	/x//../y/|100000	/ms/0//x//../y//primary
ORACLE	0	expected	/x//../y/|100000	/ms/0//x//../y//primary/expected_primary
ORACLE	0	election	/x//../y/|4294967295	/ms/0//x//../y//primary
ORACLE	0	expected	/x//../y/|4294967295	/ms/0//x//../y//primary/expected_primary
ORACLE	42	config		/pd/42/config
ORACLE	42	scheduler_prefix		/pd/42/scheduler_config/
ORACLE	42	middleware		/pd/42/service_middleware
ORACLE	42	election_nil		/pd/42/leader
ORACLE	42	scheduler		/pd/42/scheduler_config/
ORACLE	42	replication		/pd/42/replication_mode/
ORACLE	42	scheduler	balance-region	/pd/42/scheduler_config/balance-region
ORACLE	42	replication	balance-region	/pd/42/replication_mode/balance-region
ORACLE	42	scheduler	nested/name	/pd/42/scheduler_config/nested/name
ORACLE	42	replication	nested/name	/pd/42/replication_mode/nested/name
ORACLE	42	scheduler	/a//../b/	/pd/42/scheduler_config//a//../b/
ORACLE	42	replication	/a//../b/	/pd/42/replication_mode//a//../b/
ORACLE	42	scheduler	调度	/pd/42/scheduler_config/调度
ORACLE	42	replication	调度	/pd/42/replication_mode/调度
ORACLE	42	deploy	0	/pd/42/member/0/deploy_path
ORACLE	42	git_hash	0	/pd/42/member/0/git_hash
ORACLE	42	version	0	/pd/42/member/0/binary_version
ORACLE	42	priority	0	/pd/42/member/0/leader_priority
ORACLE	42	leader_weight	0	/pd/42/schedule/store_weight/00000000000000000000/leader
ORACLE	42	region_weight	0	/pd/42/schedule/store_weight/00000000000000000000/region
ORACLE	42	deploy	1	/pd/42/member/1/deploy_path
ORACLE	42	git_hash	1	/pd/42/member/1/git_hash
ORACLE	42	version	1	/pd/42/member/1/binary_version
ORACLE	42	priority	1	/pd/42/member/1/leader_priority
ORACLE	42	leader_weight	1	/pd/42/schedule/store_weight/00000000000000000001/leader
ORACLE	42	region_weight	1	/pd/42/schedule/store_weight/00000000000000000001/region
ORACLE	42	deploy	18446744073709551615	/pd/42/member/18446744073709551615/deploy_path
ORACLE	42	git_hash	18446744073709551615	/pd/42/member/18446744073709551615/git_hash
ORACLE	42	version	18446744073709551615	/pd/42/member/18446744073709551615/binary_version
ORACLE	42	priority	18446744073709551615	/pd/42/member/18446744073709551615/leader_priority
ORACLE	42	leader_weight	18446744073709551615	/pd/42/schedule/store_weight/18446744073709551615/leader
ORACLE	42	region_weight	18446744073709551615	/pd/42/schedule/store_weight/18446744073709551615/region
ORACLE	42	election	|0	/pd/42/leader
ORACLE	42	expected	|0	/ms/42//primary/expected_primary
ORACLE	42	election	|1	/pd/42/leader
ORACLE	42	expected	|1	/ms/42//primary/expected_primary
ORACLE	42	election	|99999	/pd/42/leader
ORACLE	42	expected	|99999	/ms/42//primary/expected_primary
ORACLE	42	election	|100000	/pd/42/leader
ORACLE	42	expected	|100000	/ms/42//primary/expected_primary
ORACLE	42	election	|4294967295	/pd/42/leader
ORACLE	42	expected	|4294967295	/ms/42//primary/expected_primary
ORACLE	42	election	tso|0	/ms/42/tso/00000/primary
ORACLE	42	expected	tso|0	/ms/42/tso/00000/primary/expected_primary
ORACLE	42	election	tso|1	/ms/42/tso/keyspace_groups/election/00001/primary
ORACLE	42	expected	tso|1	/ms/42/tso/keyspace_groups/election/00001/primary/expected_primary
ORACLE	42	election	tso|99999	/ms/42/tso/keyspace_groups/election/99999/primary
ORACLE	42	expected	tso|99999	/ms/42/tso/keyspace_groups/election/99999/primary/expected_primary
ORACLE	42	election	tso|100000	/ms/42/tso/keyspace_groups/election/100000/primary
ORACLE	42	expected	tso|100000	/ms/42/tso/keyspace_groups/election/100000/primary/expected_primary
ORACLE	42	election	tso|4294967295	/ms/42/tso/keyspace_groups/election/4294967295/primary
ORACLE	42	expected	tso|4294967295	/ms/42/tso/keyspace_groups/election/4294967295/primary/expected_primary
ORACLE	42	election	scheduling|0	/ms/42/scheduling/primary
ORACLE	42	expected	scheduling|0	/ms/42/scheduling/primary/expected_primary
ORACLE	42	election	scheduling|1	/ms/42/scheduling/primary
ORACLE	42	expected	scheduling|1	/ms/42/scheduling/primary/expected_primary
ORACLE	42	election	scheduling|99999	/ms/42/scheduling/primary
ORACLE	42	expected	scheduling|99999	/ms/42/scheduling/primary/expected_primary
ORACLE	42	election	scheduling|100000	/ms/42/scheduling/primary
ORACLE	42	expected	scheduling|100000	/ms/42/scheduling/primary/expected_primary
ORACLE	42	election	scheduling|4294967295	/ms/42/scheduling/primary
ORACLE	42	expected	scheduling|4294967295	/ms/42/scheduling/primary/expected_primary
ORACLE	42	election	tso/|0	/ms/42/tso//primary
ORACLE	42	expected	tso/|0	/ms/42/tso//primary/expected_primary
ORACLE	42	election	tso/|1	/ms/42/tso//primary
ORACLE	42	expected	tso/|1	/ms/42/tso//primary/expected_primary
ORACLE	42	election	tso/|99999	/ms/42/tso//primary
ORACLE	42	expected	tso/|99999	/ms/42/tso//primary/expected_primary
ORACLE	42	election	tso/|100000	/ms/42/tso//primary
ORACLE	42	expected	tso/|100000	/ms/42/tso//primary/expected_primary
ORACLE	42	election	tso/|4294967295	/ms/42/tso//primary
ORACLE	42	expected	tso/|4294967295	/ms/42/tso//primary/expected_primary
ORACLE	42	election	/x//../y/|0	/ms/42//x//../y//primary
ORACLE	42	expected	/x//../y/|0	/ms/42//x//../y//primary/expected_primary
ORACLE	42	election	/x//../y/|1	/ms/42//x//../y//primary
ORACLE	42	expected	/x//../y/|1	/ms/42//x//../y//primary/expected_primary
ORACLE	42	election	/x//../y/|99999	/ms/42//x//../y//primary
ORACLE	42	expected	/x//../y/|99999	/ms/42//x//../y//primary/expected_primary
ORACLE	42	election	/x//../y/|100000	/ms/42//x//../y//primary
ORACLE	42	expected	/x//../y/|100000	/ms/42//x//../y//primary/expected_primary
ORACLE	42	election	/x//../y/|4294967295	/ms/42//x//../y//primary
ORACLE	42	expected	/x//../y/|4294967295	/ms/42//x//../y//primary/expected_primary
ORACLE	18446744073709551615	config		/pd/18446744073709551615/config
ORACLE	18446744073709551615	scheduler_prefix		/pd/18446744073709551615/scheduler_config/
ORACLE	18446744073709551615	middleware		/pd/18446744073709551615/service_middleware
ORACLE	18446744073709551615	election_nil		/pd/18446744073709551615/leader
ORACLE	18446744073709551615	scheduler		/pd/18446744073709551615/scheduler_config/
ORACLE	18446744073709551615	replication		/pd/18446744073709551615/replication_mode/
ORACLE	18446744073709551615	scheduler	balance-region	/pd/18446744073709551615/scheduler_config/balance-region
ORACLE	18446744073709551615	replication	balance-region	/pd/18446744073709551615/replication_mode/balance-region
ORACLE	18446744073709551615	scheduler	nested/name	/pd/18446744073709551615/scheduler_config/nested/name
ORACLE	18446744073709551615	replication	nested/name	/pd/18446744073709551615/replication_mode/nested/name
ORACLE	18446744073709551615	scheduler	/a//../b/	/pd/18446744073709551615/scheduler_config//a//../b/
ORACLE	18446744073709551615	replication	/a//../b/	/pd/18446744073709551615/replication_mode//a//../b/
ORACLE	18446744073709551615	scheduler	调度	/pd/18446744073709551615/scheduler_config/调度
ORACLE	18446744073709551615	replication	调度	/pd/18446744073709551615/replication_mode/调度
ORACLE	18446744073709551615	deploy	0	/pd/18446744073709551615/member/0/deploy_path
ORACLE	18446744073709551615	git_hash	0	/pd/18446744073709551615/member/0/git_hash
ORACLE	18446744073709551615	version	0	/pd/18446744073709551615/member/0/binary_version
ORACLE	18446744073709551615	priority	0	/pd/18446744073709551615/member/0/leader_priority
ORACLE	18446744073709551615	leader_weight	0	/pd/18446744073709551615/schedule/store_weight/00000000000000000000/leader
ORACLE	18446744073709551615	region_weight	0	/pd/18446744073709551615/schedule/store_weight/00000000000000000000/region
ORACLE	18446744073709551615	deploy	1	/pd/18446744073709551615/member/1/deploy_path
ORACLE	18446744073709551615	git_hash	1	/pd/18446744073709551615/member/1/git_hash
ORACLE	18446744073709551615	version	1	/pd/18446744073709551615/member/1/binary_version
ORACLE	18446744073709551615	priority	1	/pd/18446744073709551615/member/1/leader_priority
ORACLE	18446744073709551615	leader_weight	1	/pd/18446744073709551615/schedule/store_weight/00000000000000000001/leader
ORACLE	18446744073709551615	region_weight	1	/pd/18446744073709551615/schedule/store_weight/00000000000000000001/region
ORACLE	18446744073709551615	deploy	18446744073709551615	/pd/18446744073709551615/member/18446744073709551615/deploy_path
ORACLE	18446744073709551615	git_hash	18446744073709551615	/pd/18446744073709551615/member/18446744073709551615/git_hash
ORACLE	18446744073709551615	version	18446744073709551615	/pd/18446744073709551615/member/18446744073709551615/binary_version
ORACLE	18446744073709551615	priority	18446744073709551615	/pd/18446744073709551615/member/18446744073709551615/leader_priority
ORACLE	18446744073709551615	leader_weight	18446744073709551615	/pd/18446744073709551615/schedule/store_weight/18446744073709551615/leader
ORACLE	18446744073709551615	region_weight	18446744073709551615	/pd/18446744073709551615/schedule/store_weight/18446744073709551615/region
ORACLE	18446744073709551615	election	|0	/pd/18446744073709551615/leader
ORACLE	18446744073709551615	expected	|0	/ms/18446744073709551615//primary/expected_primary
ORACLE	18446744073709551615	election	|1	/pd/18446744073709551615/leader
ORACLE	18446744073709551615	expected	|1	/ms/18446744073709551615//primary/expected_primary
ORACLE	18446744073709551615	election	|99999	/pd/18446744073709551615/leader
ORACLE	18446744073709551615	expected	|99999	/ms/18446744073709551615//primary/expected_primary
ORACLE	18446744073709551615	election	|100000	/pd/18446744073709551615/leader
ORACLE	18446744073709551615	expected	|100000	/ms/18446744073709551615//primary/expected_primary
ORACLE	18446744073709551615	election	|4294967295	/pd/18446744073709551615/leader
ORACLE	18446744073709551615	expected	|4294967295	/ms/18446744073709551615//primary/expected_primary
ORACLE	18446744073709551615	election	tso|0	/ms/18446744073709551615/tso/00000/primary
ORACLE	18446744073709551615	expected	tso|0	/ms/18446744073709551615/tso/00000/primary/expected_primary
ORACLE	18446744073709551615	election	tso|1	/ms/18446744073709551615/tso/keyspace_groups/election/00001/primary
ORACLE	18446744073709551615	expected	tso|1	/ms/18446744073709551615/tso/keyspace_groups/election/00001/primary/expected_primary
ORACLE	18446744073709551615	election	tso|99999	/ms/18446744073709551615/tso/keyspace_groups/election/99999/primary
ORACLE	18446744073709551615	expected	tso|99999	/ms/18446744073709551615/tso/keyspace_groups/election/99999/primary/expected_primary
ORACLE	18446744073709551615	election	tso|100000	/ms/18446744073709551615/tso/keyspace_groups/election/100000/primary
ORACLE	18446744073709551615	expected	tso|100000	/ms/18446744073709551615/tso/keyspace_groups/election/100000/primary/expected_primary
ORACLE	18446744073709551615	election	tso|4294967295	/ms/18446744073709551615/tso/keyspace_groups/election/4294967295/primary
ORACLE	18446744073709551615	expected	tso|4294967295	/ms/18446744073709551615/tso/keyspace_groups/election/4294967295/primary/expected_primary
ORACLE	18446744073709551615	election	scheduling|0	/ms/18446744073709551615/scheduling/primary
ORACLE	18446744073709551615	expected	scheduling|0	/ms/18446744073709551615/scheduling/primary/expected_primary
ORACLE	18446744073709551615	election	scheduling|1	/ms/18446744073709551615/scheduling/primary
ORACLE	18446744073709551615	expected	scheduling|1	/ms/18446744073709551615/scheduling/primary/expected_primary
ORACLE	18446744073709551615	election	scheduling|99999	/ms/18446744073709551615/scheduling/primary
ORACLE	18446744073709551615	expected	scheduling|99999	/ms/18446744073709551615/scheduling/primary/expected_primary
ORACLE	18446744073709551615	election	scheduling|100000	/ms/18446744073709551615/scheduling/primary
ORACLE	18446744073709551615	expected	scheduling|100000	/ms/18446744073709551615/scheduling/primary/expected_primary
ORACLE	18446744073709551615	election	scheduling|4294967295	/ms/18446744073709551615/scheduling/primary
ORACLE	18446744073709551615	expected	scheduling|4294967295	/ms/18446744073709551615/scheduling/primary/expected_primary
ORACLE	18446744073709551615	election	tso/|0	/ms/18446744073709551615/tso//primary
ORACLE	18446744073709551615	expected	tso/|0	/ms/18446744073709551615/tso//primary/expected_primary
ORACLE	18446744073709551615	election	tso/|1	/ms/18446744073709551615/tso//primary
ORACLE	18446744073709551615	expected	tso/|1	/ms/18446744073709551615/tso//primary/expected_primary
ORACLE	18446744073709551615	election	tso/|99999	/ms/18446744073709551615/tso//primary
ORACLE	18446744073709551615	expected	tso/|99999	/ms/18446744073709551615/tso//primary/expected_primary
ORACLE	18446744073709551615	election	tso/|100000	/ms/18446744073709551615/tso//primary
ORACLE	18446744073709551615	expected	tso/|100000	/ms/18446744073709551615/tso//primary/expected_primary
ORACLE	18446744073709551615	election	tso/|4294967295	/ms/18446744073709551615/tso//primary
ORACLE	18446744073709551615	expected	tso/|4294967295	/ms/18446744073709551615/tso//primary/expected_primary
ORACLE	18446744073709551615	election	/x//../y/|0	/ms/18446744073709551615//x//../y//primary
ORACLE	18446744073709551615	expected	/x//../y/|0	/ms/18446744073709551615//x//../y//primary/expected_primary
ORACLE	18446744073709551615	election	/x//../y/|1	/ms/18446744073709551615//x//../y//primary
ORACLE	18446744073709551615	expected	/x//../y/|1	/ms/18446744073709551615//x//../y//primary/expected_primary
ORACLE	18446744073709551615	election	/x//../y/|99999	/ms/18446744073709551615//x//../y//primary
ORACLE	18446744073709551615	expected	/x//../y/|99999	/ms/18446744073709551615//x//../y//primary/expected_primary
ORACLE	18446744073709551615	election	/x//../y/|100000	/ms/18446744073709551615//x//../y//primary
ORACLE	18446744073709551615	expected	/x//../y/|100000	/ms/18446744073709551615//x//../y//primary/expected_primary
ORACLE	18446744073709551615	election	/x//../y/|4294967295	/ms/18446744073709551615//x//../y//primary
ORACLE	18446744073709551615	expected	/x//../y/|4294967295	/ms/18446744073709551615//x//../y//primary/expected_primary
ORACLE	prefix		.
ORACLE	prefix	a	.
ORACLE	prefix	/	/
ORACLE	prefix	//	/
ORACLE	prefix	a/	a
ORACLE	prefix	a/../b	.
ORACLE	prefix	../a/b	../a
ORACLE	prefix	../../a	../..
ORACLE	prefix	/a//b/../c	/a
ORACLE	prefix	/../../a	/
ORACLE	prefix	a/./b/	a/b
ORACLE	prefix	a/../../b/c	../b
ORACLE	prefix	a\b	.
ORACLE	prefix	调度/成员	调度
ORACLE	prefix	./	.
ORACLE	prefix	../	..
ORACLE	prefix	a/..	a
"#;

#[test]
fn member_config_paths_go_oracle() {
    let mut count = 0;
    for line in GO_ORACLE.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let actual = if fields[1] == "prefix" {
            prefix(fields[2])
        } else {
            let paths = ClusterPaths::new(fields[1].parse().unwrap());
            let input = fields[3];
            match fields[2] {
                "config" => paths.config_path(),
                "scheduler_prefix" => paths.scheduler_config_path_prefix(),
                "middleware" => paths.service_middleware_path(),
                "scheduler" => paths.scheduler_config_path(input),
                "replication" => paths.replication_mode_path(input),
                "deploy" => paths.member_binary_deploy_path(input.parse().unwrap()),
                "git_hash" => paths.member_git_hash_path(input.parse().unwrap()),
                "version" => paths.member_binary_version_path(input.parse().unwrap()),
                "priority" => paths.member_leader_priority_path(input.parse().unwrap()),
                "leader_weight" => paths.store_leader_weight_path(input.parse().unwrap()),
                "region_weight" => paths.store_region_weight_path(input.parse().unwrap()),
                "election_nil" => paths.election_path(None),
                "election" | "expected" => {
                    let (service, group) = input.rsplit_once('|').unwrap();
                    let param = MsParam {
                        service_name: service.into(),
                        group_id: group.parse().unwrap(),
                    };
                    if fields[2] == "election" {
                        paths.election_path(Some(&param))
                    } else {
                        paths.expected_primary_path(&param)
                    }
                }
                op => panic!("unknown operation {op}"),
            }
        };
        assert_eq!(
            actual.as_bytes(),
            fields.last().unwrap().as_bytes(),
            "{line}"
        );
        println!("{}\t{actual}", fields[..fields.len() - 1].join("\t"));
        count += 1;
    }
    assert_eq!(count, 263);
}

#[test]
fn member_config_paths_independent_contexts() {
    let a = ClusterPaths::new(1);
    let b = ClusterPaths::new(2);
    assert_eq!(a.config_path(), "/pd/1/config");
    assert_eq!(b.config_path(), "/pd/2/config");
    assert_eq!(a.election_path(None), "/pd/1/leader");
    assert_eq!(
        MsParam::default(),
        MsParam {
            service_name: String::new(),
            group_id: 0
        }
    );
}
