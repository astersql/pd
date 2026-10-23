// Copyright 2026 AsterSQL.
use crate::*;

const GO_ORACLE: &str = concat!(
    "rule\t0\t0\t\t/pd/0/rules/\t\t\n",
    "group\t0\t0\t\t/pd/0/rule_group/\t\t\n",
    "label\t0\t0\t\t/pd/0/region_label/\t\t\n",
    "affinity\t0\t0\t\t/pd/0/affinity_groups/\t\t\n",
    "rule\t0\t0\tgroup\t/pd/0/rules/group\t\t\n",
    "group\t0\t0\tgroup\t/pd/0/rule_group/group\t\t\n",
    "label\t0\t0\tgroup\t/pd/0/region_label/group\t\t\n",
    "affinity\t0\t0\tgroup\t/pd/0/affinity_groups/group\t\t\n",
    "rule\t0\t0\ta/b\t/pd/0/rules/a/b\t\t\n",
    "group\t0\t0\ta/b\t/pd/0/rule_group/a/b\t\t\n",
    "label\t0\t0\ta/b\t/pd/0/region_label/a/b\t\t\n",
    "affinity\t0\t0\ta/b\t/pd/0/affinity_groups/a/b\t\t\n",
    "rule\t0\t0\t/a//../b/\t/pd/0/rules//a//../b/\t\t\n",
    "group\t0\t0\t/a//../b/\t/pd/0/rule_group//a//../b/\t\t\n",
    "label\t0\t0\t/a//../b/\t/pd/0/region_label//a//../b/\t\t\n",
    "affinity\t0\t0\t/a//../b/\t/pd/0/affinity_groups//a//../b/\t\t\n",
    "rule\t0\t0\t规则\t/pd/0/rules/规则\t\t\n",
    "group\t0\t0\t规则\t/pd/0/rule_group/规则\t\t\n",
    "label\t0\t0\t规则\t/pd/0/region_label/规则\t\t\n",
    "affinity\t0\t0\t规则\t/pd/0/affinity_groups/规则\t\t\n",
    "rules_prefix\t0\t0\t\t/pd/0/rules/\t\t\n",
    "common\t0\t0\t\t/pd/0/rule\t\t\n",
    "groups_prefix\t0\t0\t\t/pd/0/rule_group/\t\t\n",
    "labels_prefix\t0\t0\t\t/pd/0/region_label/\t\t\n",
    "affinity_prefix\t0\t0\t\t/pd/0/affinity_groups/\t\t\n",
    "rule\t42\t0\t\t/pd/42/rules/\t\t\n",
    "group\t42\t0\t\t/pd/42/rule_group/\t\t\n",
    "label\t42\t0\t\t/pd/42/region_label/\t\t\n",
    "affinity\t42\t0\t\t/pd/42/affinity_groups/\t\t\n",
    "rule\t42\t0\tgroup\t/pd/42/rules/group\t\t\n",
    "group\t42\t0\tgroup\t/pd/42/rule_group/group\t\t\n",
    "label\t42\t0\tgroup\t/pd/42/region_label/group\t\t\n",
    "affinity\t42\t0\tgroup\t/pd/42/affinity_groups/group\t\t\n",
    "rule\t42\t0\ta/b\t/pd/42/rules/a/b\t\t\n",
    "group\t42\t0\ta/b\t/pd/42/rule_group/a/b\t\t\n",
    "label\t42\t0\ta/b\t/pd/42/region_label/a/b\t\t\n",
    "affinity\t42\t0\ta/b\t/pd/42/affinity_groups/a/b\t\t\n",
    "rule\t42\t0\t/a//../b/\t/pd/42/rules//a//../b/\t\t\n",
    "group\t42\t0\t/a//../b/\t/pd/42/rule_group//a//../b/\t\t\n",
    "label\t42\t0\t/a//../b/\t/pd/42/region_label//a//../b/\t\t\n",
    "affinity\t42\t0\t/a//../b/\t/pd/42/affinity_groups//a//../b/\t\t\n",
    "rule\t42\t0\t规则\t/pd/42/rules/规则\t\t\n",
    "group\t42\t0\t规则\t/pd/42/rule_group/规则\t\t\n",
    "label\t42\t0\t规则\t/pd/42/region_label/规则\t\t\n",
    "affinity\t42\t0\t规则\t/pd/42/affinity_groups/规则\t\t\n",
    "rules_prefix\t42\t0\t\t/pd/42/rules/\t\t\n",
    "common\t42\t0\t\t/pd/42/rule\t\t\n",
    "groups_prefix\t42\t0\t\t/pd/42/rule_group/\t\t\n",
    "labels_prefix\t42\t0\t\t/pd/42/region_label/\t\t\n",
    "affinity_prefix\t42\t0\t\t/pd/42/affinity_groups/\t\t\n",
    "rule\t18446744073709551615\t0\t\t/pd/18446744073709551615/rules/\t\t\n",
    "group\t18446744073709551615\t0\t\t/pd/18446744073709551615/rule_group/\t\t\n",
    "label\t18446744073709551615\t0\t\t/pd/18446744073709551615/region_label/\t\t\n",
    "affinity\t18446744073709551615\t0\t\t/pd/18446744073709551615/affinity_groups/\t\t\n",
    "rule\t18446744073709551615\t0\tgroup\t/pd/18446744073709551615/rules/group\t\t\n",
    "group\t18446744073709551615\t0\tgroup\t/pd/18446744073709551615/rule_group/group\t\t\n",
    "label\t18446744073709551615\t0\tgroup\t/pd/18446744073709551615/region_label/group\t\t\n",
    "affinity\t18446744073709551615\t0\tgroup\t/pd/18446744073709551615/affinity_groups/group\t\t\n",
    "rule\t18446744073709551615\t0\ta/b\t/pd/18446744073709551615/rules/a/b\t\t\n",
    "group\t18446744073709551615\t0\ta/b\t/pd/18446744073709551615/rule_group/a/b\t\t\n",
    "label\t18446744073709551615\t0\ta/b\t/pd/18446744073709551615/region_label/a/b\t\t\n",
    "affinity\t18446744073709551615\t0\ta/b\t/pd/18446744073709551615/affinity_groups/a/b\t\t\n",
    "rule\t18446744073709551615\t0\t/a//../b/\t/pd/18446744073709551615/rules//a//../b/\t\t\n",
    "group\t18446744073709551615\t0\t/a//../b/\t/pd/18446744073709551615/rule_group//a//../b/\t\t\n",
    "label\t18446744073709551615\t0\t/a//../b/\t/pd/18446744073709551615/region_label//a//../b/\t\t\n",
    "affinity\t18446744073709551615\t0\t/a//../b/\t/pd/18446744073709551615/affinity_groups//a//../b/\t\t\n",
    "rule\t18446744073709551615\t0\t规则\t/pd/18446744073709551615/rules/规则\t\t\n",
    "group\t18446744073709551615\t0\t规则\t/pd/18446744073709551615/rule_group/规则\t\t\n",
    "label\t18446744073709551615\t0\t规则\t/pd/18446744073709551615/region_label/规则\t\t\n",
    "affinity\t18446744073709551615\t0\t规则\t/pd/18446744073709551615/affinity_groups/规则\t\t\n",
    "rules_prefix\t18446744073709551615\t0\t\t/pd/18446744073709551615/rules/\t\t\n",
    "common\t18446744073709551615\t0\t\t/pd/18446744073709551615/rule\t\t\n",
    "groups_prefix\t18446744073709551615\t0\t\t/pd/18446744073709551615/rule_group/\t\t\n",
    "labels_prefix\t18446744073709551615\t0\t\t/pd/18446744073709551615/region_label/\t\t\n",
    "affinity_prefix\t18446744073709551615\t0\t\t/pd/18446744073709551615/affinity_groups/\t\t\n",
    "settings\t0\t0\t\tresource_group/keyspace/settings/0/\t\t\n",
    "states\t0\t0\t\tresource_group/keyspace/states/0/\t\t\n",
    "settings\t0\t0\tgroup\tresource_group/keyspace/settings/0/group\t\t\n",
    "states\t0\t0\tgroup\tresource_group/keyspace/states/0/group\t\t\n",
    "settings\t0\t0\ta/b\tresource_group/keyspace/settings/0/a/b\t\t\n",
    "states\t0\t0\ta/b\tresource_group/keyspace/states/0/a/b\t\t\n",
    "settings\t0\t0\t/a//../b/\tresource_group/keyspace/settings/0//a//../b/\t\t\n",
    "states\t0\t0\t/a//../b/\tresource_group/keyspace/states/0//a//../b/\t\t\n",
    "settings\t0\t0\t资源\tresource_group/keyspace/settings/0/资源\t\t\n",
    "states\t0\t0\t资源\tresource_group/keyspace/states/0/资源\t\t\n",
    "limit\t0\t0\t\tresource_group/keyspace/service_limits/0\t\t\n",
    "settings\t0\t1\t\tresource_group/keyspace/settings/1/\t\t\n",
    "states\t0\t1\t\tresource_group/keyspace/states/1/\t\t\n",
    "settings\t0\t1\tgroup\tresource_group/keyspace/settings/1/group\t\t\n",
    "states\t0\t1\tgroup\tresource_group/keyspace/states/1/group\t\t\n",
    "settings\t0\t1\ta/b\tresource_group/keyspace/settings/1/a/b\t\t\n",
    "states\t0\t1\ta/b\tresource_group/keyspace/states/1/a/b\t\t\n",
    "settings\t0\t1\t/a//../b/\tresource_group/keyspace/settings/1//a//../b/\t\t\n",
    "states\t0\t1\t/a//../b/\tresource_group/keyspace/states/1//a//../b/\t\t\n",
    "settings\t0\t1\t资源\tresource_group/keyspace/settings/1/资源\t\t\n",
    "states\t0\t1\t资源\tresource_group/keyspace/states/1/资源\t\t\n",
    "limit\t0\t1\t\tresource_group/keyspace/service_limits/1\t\t\n",
    "settings\t0\t42\t\tresource_group/keyspace/settings/42/\t\t\n",
    "states\t0\t42\t\tresource_group/keyspace/states/42/\t\t\n",
    "settings\t0\t42\tgroup\tresource_group/keyspace/settings/42/group\t\t\n",
    "states\t0\t42\tgroup\tresource_group/keyspace/states/42/group\t\t\n",
    "settings\t0\t42\ta/b\tresource_group/keyspace/settings/42/a/b\t\t\n",
    "states\t0\t42\ta/b\tresource_group/keyspace/states/42/a/b\t\t\n",
    "settings\t0\t42\t/a//../b/\tresource_group/keyspace/settings/42//a//../b/\t\t\n",
    "states\t0\t42\t/a//../b/\tresource_group/keyspace/states/42//a//../b/\t\t\n",
    "settings\t0\t42\t资源\tresource_group/keyspace/settings/42/资源\t\t\n",
    "states\t0\t42\t资源\tresource_group/keyspace/states/42/资源\t\t\n",
    "limit\t0\t42\t\tresource_group/keyspace/service_limits/42\t\t\n",
    "settings\t0\t4294967294\t\tresource_group/keyspace/settings/4294967294/\t\t\n",
    "states\t0\t4294967294\t\tresource_group/keyspace/states/4294967294/\t\t\n",
    "settings\t0\t4294967294\tgroup\tresource_group/keyspace/settings/4294967294/group\t\t\n",
    "states\t0\t4294967294\tgroup\tresource_group/keyspace/states/4294967294/group\t\t\n",
    "settings\t0\t4294967294\ta/b\tresource_group/keyspace/settings/4294967294/a/b\t\t\n",
    "states\t0\t4294967294\ta/b\tresource_group/keyspace/states/4294967294/a/b\t\t\n",
    "settings\t0\t4294967294\t/a//../b/\tresource_group/keyspace/settings/4294967294//a//../b/\t\t\n",
    "states\t0\t4294967294\t/a//../b/\tresource_group/keyspace/states/4294967294//a//../b/\t\t\n",
    "settings\t0\t4294967294\t资源\tresource_group/keyspace/settings/4294967294/资源\t\t\n",
    "states\t0\t4294967294\t资源\tresource_group/keyspace/states/4294967294/资源\t\t\n",
    "limit\t0\t4294967294\t\tresource_group/keyspace/service_limits/4294967294\t\t\n",
    "settings\t0\t4294967295\t\tresource_group/settings/\t\t\n",
    "states\t0\t4294967295\t\tresource_group/states/\t\t\n",
    "settings\t0\t4294967295\tgroup\tresource_group/settings/group\t\t\n",
    "states\t0\t4294967295\tgroup\tresource_group/states/group\t\t\n",
    "settings\t0\t4294967295\ta/b\tresource_group/settings/a/b\t\t\n",
    "states\t0\t4294967295\ta/b\tresource_group/states/a/b\t\t\n",
    "settings\t0\t4294967295\t/a//../b/\tresource_group/settings//a//../b/\t\t\n",
    "states\t0\t4294967295\t/a//../b/\tresource_group/states//a//../b/\t\t\n",
    "settings\t0\t4294967295\t资源\tresource_group/settings/资源\t\t\n",
    "states\t0\t4294967295\t资源\tresource_group/states/资源\t\t\n",
    "limit\t0\t4294967295\t\tresource_group/keyspace/service_limits/4294967295\t\t\n",
    "controller\t0\t0\t\tresource_group/controller\t\t\n",
    "resource_prefix\t0\t0\t\tresource_group/\t\t\n",
    "settings_prefix\t0\t0\t\tresource_group/settings/\t\t\n",
    "states_prefix\t0\t0\t\tresource_group/states/\t\t\n",
    "ks_settings_prefix\t0\t0\t\tresource_group/keyspace/settings/\t\t\n",
    "ks_states_prefix\t0\t0\t\tresource_group/keyspace/states/\t\t\n",
    "limit_prefix\t0\t0\t\tresource_group/keyspace/service_limits/\t\t\n",
    "parse\t0\t0\t123/group1\t123\tgroup1\t\n",
    "parse\t0\t0\t456/group1/subgroup\t456\tgroup1/subgroup\t\n",
    "parse\t0\t0\t0/group1\t0\tgroup1\t\n",
    "parse\t0\t0\t4294967295/group1\t4294967295\tgroup1\t\n",
    "parse\t0\t0\t\t0\t\tinvalid keyspace resource group setting path: \n",
    "parse\t0\t0\t123\t0\t\tinvalid keyspace resource group setting path: 123\n",
    "parse\t0\t0\tabc/group1\t0\t\tinvalid keyspace ID str: abc\n",
    "parse\t0\t0\t4294967296/group1\t0\t\tinvalid keyspace ID str: 4294967296\n",
    "parse\t0\t0\t123/\t123\t\t\n",
    "parse\t0\t0\t123//x\t123\t/x\t\n",
    "parse\t0\t0\t00042/资源\t42\t资源\t\n",
    "parse\t0\t0\t+1/x\t0\t\tinvalid keyspace ID str: +1\n",
    "parse\t0\t0\t-1/x\t0\t\tinvalid keyspace ID str: -1\n",
    "parse\t0\t0\t 1/x\t0\t\tinvalid keyspace ID str:  1\n",
    "parse\t0\t0\t１/x\t0\t\tinvalid keyspace ID str: １\n",
    "parse\t0\t0\t/x\t0\t\tinvalid keyspace ID str: \n",
    "parse\t0\t0\t1_0/x\t0\t\tinvalid keyspace ID str: 1_0\n",
    "parse\t0\t0\t0x1/x\t0\t\tinvalid keyspace ID str: 0x1\n",
    "parse\t0\t0\t1/a/../b\t1\ta/../b\t\n",
    "parse\t0\t0\tresource_group/keyspace/settings/1/x\t0\t\tinvalid keyspace ID str: resource_group\n",
);

#[test]
fn rule_resource_paths_go_byte_oracle() {
    let mut count = 0;
    for line in GO_ORACLE.lines() {
        let f: Vec<_> = line.split('\t').collect();
        assert_eq!(f.len(), 7);
        let p = ClusterPaths::new(f[1].parse().unwrap());
        let id = f[2].parse().unwrap();
        let name = f[3];
        let actual = match f[0] {
            "rule" => p.rule_key_path(name),
            "group" => p.rule_group_id_path(name),
            "label" => p.region_label_key_path(name),
            "affinity" => p.affinity_group_path(name),
            "rules_prefix" => p.rules_path_prefix(),
            "common" => p.rule_common_path_prefix(),
            "groups_prefix" => p.rule_group_path_prefix(),
            "labels_prefix" => p.region_label_path_prefix(),
            "affinity_prefix" => p.affinity_groups_prefix(),
            "settings" => keyspace_resource_group_setting_path(id, name),
            "states" => keyspace_resource_group_state_path(id, name),
            "limit" => keyspace_service_limit_path(id),
            "controller" => controller_config_path().to_owned(),
            "resource_prefix" => resource_group_prefix().to_owned(),
            "settings_prefix" => resource_group_setting_prefix().to_owned(),
            "states_prefix" => resource_group_state_prefix().to_owned(),
            "ks_settings_prefix" => keyspace_resource_group_setting_prefix().to_owned(),
            "ks_states_prefix" => keyspace_resource_group_state_prefix().to_owned(),
            "limit_prefix" => keyspace_service_limit_prefix().to_owned(),
            "parse" => {
                let (value, parsed, error) = match parse_keyspace_resource_group_path(name) {
                    Ok((id, name)) => (id.to_string(), name.to_owned(), String::new()),
                    Err(e) => ("0".to_owned(), String::new(), e.to_string()),
                };
                assert_eq!(parsed.as_bytes(), f[5].as_bytes(), "{line}");
                assert_eq!(error.as_bytes(), f[6].as_bytes(), "{line}");
                value
            }
            op => panic!("unknown operation {op}"),
        };
        assert_eq!(actual.as_bytes(), f[4].as_bytes(), "{line}");
        println!("ORACLE\t{line}");
        count += 1;
    }
    assert_eq!(count, 157);
}

#[test]
fn rule_resource_paths_prefixes_and_roundtrips() {
    for cluster in [0, 42, u64::MAX] {
        let p = ClusterPaths::new(cluster);
        for key in [p.rule_key_path("x"), p.rule_group_id_path("x")] {
            assert!(key.starts_with(&p.rule_common_path_prefix()));
            assert!(!key.starts_with(
                &ClusterPaths::new(cluster.wrapping_add(1)).rule_common_path_prefix()
            ));
        }
    }
    for id in [0, 1, 42, u32::MAX - 1] {
        for name in ["", "a/b", "/a//../b/", "资源"] {
            for (path, prefix) in [
                (
                    keyspace_resource_group_setting_path(id, name),
                    keyspace_resource_group_setting_prefix(),
                ),
                (
                    keyspace_resource_group_state_path(id, name),
                    keyspace_resource_group_state_prefix(),
                ),
            ] {
                assert_eq!(
                    parse_keyspace_resource_group_path(path.strip_prefix(prefix).unwrap()),
                    Ok((id, name))
                );
                assert!(!path.starts_with(&format!("{prefix}{}/", id + 1)));
            }
        }
    }
    assert!(matches!(
        parse_keyspace_resource_group_path("123"),
        Err(ResourceGroupPathError::InvalidPath(_))
    ));
    assert!(matches!(
        parse_keyspace_resource_group_path("+1/x"),
        Err(ResourceGroupPathError::InvalidKeyspaceId(_))
    ));
}
