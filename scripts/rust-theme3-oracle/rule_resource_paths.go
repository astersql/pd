// Copyright 2026 AsterSQL.
package main

import (
	"fmt"

	"github.com/tikv/pd/pkg/utils/keypath"
)

func ruleResourcePaths() {
	emit := func(op string, cluster uint64, id uint32, name, value, parsed, err string) {
		fmt.Printf("%s\t%d\t%d\t%s\t%s\t%s\t%s\n", op, cluster, id, name, value, parsed, err)
	}
	for _, cluster := range []uint64{0, 42, ^uint64(0)} {
		keypath.ResetClusterID()
		keypath.SetClusterID(cluster)
		for _, name := range []string{"", "group", "a/b", "/a//../b/", "规则"} {
			for _, entry := range []struct{ op, value string }{
				{"rule", keypath.RuleKeyPath(name)}, {"group", keypath.RuleGroupIDPath(name)},
				{"label", keypath.RegionLabelKeyPath(name)}, {"affinity", keypath.AffinityGroupPath(name)},
			} {
				emit(entry.op, cluster, 0, name, entry.value, "", "")
			}
		}
		for _, entry := range []struct{ op, value string }{
			{"rules_prefix", keypath.RulesPathPrefix()}, {"common", keypath.RuleCommonPathPrefix()},
			{"groups_prefix", keypath.RuleGroupPathPrefix()}, {"labels_prefix", keypath.RegionLabelPathPrefix()},
			{"affinity_prefix", keypath.AffinityGroupsPrefix()},
		} {
			emit(entry.op, cluster, 0, "", entry.value, "", "")
		}
	}
	for _, id := range []uint32{0, 1, 42, ^uint32(0) - 1, ^uint32(0)} {
		for _, name := range []string{"", "group", "a/b", "/a//../b/", "资源"} {
			emit("settings", 0, id, name, keypath.KeyspaceResourceGroupSettingPath(id, name), "", "")
			emit("states", 0, id, name, keypath.KeyspaceResourceGroupStatePath(id, name), "", "")
		}
		emit("limit", 0, id, "", keypath.KeyspaceServiceLimitPath(id), "", "")
	}
	for _, entry := range []struct{ op, value string }{
		{"controller", keypath.ControllerConfigPath()}, {"resource_prefix", keypath.ResourceGroupPrefix()},
		{"settings_prefix", keypath.ResourceGroupSettingPrefix()}, {"states_prefix", keypath.ResourceGroupStatePrefix()},
		{"ks_settings_prefix", keypath.KeyspaceResourceGroupSettingPrefix()}, {"ks_states_prefix", keypath.KeyspaceResourceGroupStatePrefix()},
		{"limit_prefix", keypath.KeyspaceServiceLimitPrefix()},
	} {
		emit(entry.op, 0, 0, "", entry.value, "", "")
	}
	for _, path := range []string{"123/group1", "456/group1/subgroup", "0/group1", "4294967295/group1", "", "123", "abc/group1", "4294967296/group1", "123/", "123//x", "00042/资源", "+1/x", "-1/x", " 1/x", "１/x", "/x", "1_0/x", "0x1/x", "1/a/../b", "resource_group/keyspace/settings/1/x"} {
		id, name, err := keypath.ParseKeyspaceResourceGroupPath(path)
		message := ""
		if err != nil {
			message = err.Error()
		}
		emit("parse", 0, 0, path, fmt.Sprint(id), name, message)
	}
	keypath.ResetClusterID()
}
