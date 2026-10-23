// Copyright 2026 AsterSQL.
package main

import (
	"fmt"
	"math"

	"github.com/tikv/pd/pkg/utils/keypath"
)

func memberConfigPaths() {
	defer keypath.ResetClusterID()
	for _, cluster := range []uint64{0, 42, math.MaxUint64} {
		keypath.SetClusterID(cluster)
		emit := func(op, input, value string) { fmt.Printf("ORACLE\t%d\t%s\t%s\t%s\n", cluster, op, input, value) }
		emit("config", "", keypath.ConfigPath())
		emit("scheduler_prefix", "", keypath.SchedulerConfigPathPrefix())
		emit("middleware", "", keypath.ServiceMiddlewarePath())
		emit("election_nil", "", keypath.ElectionPath(nil))
		for _, name := range []string{"", "balance-region", "nested/name", "/a//../b/", "调度"} {
			emit("scheduler", name, keypath.SchedulerConfigPath(name))
			emit("replication", name, keypath.ReplicationModePath(name))
		}
		for _, id := range []uint64{0, 1, math.MaxUint64} {
			input := fmt.Sprint(id)
			emit("deploy", input, keypath.MemberBinaryDeployPath(id))
			emit("git_hash", input, keypath.MemberGitHashPath(id))
			emit("version", input, keypath.MemberBinaryVersionPath(id))
			emit("priority", input, keypath.MemberLeaderPriorityPath(id))
			emit("leader_weight", input, keypath.StoreLeaderWeightPath(id))
			emit("region_weight", input, keypath.StoreRegionWeightPath(id))
		}
		for _, service := range []string{"", "tso", "scheduling", "tso/", "/x//../y/"} {
			for _, group := range []uint32{0, 1, 99999, 100000, math.MaxUint32} {
				p := &keypath.MsParam{ServiceName: service, GroupID: group}
				input := fmt.Sprintf("%s|%d", service, group)
				emit("election", input, keypath.ElectionPath(p))
				emit("expected", input, keypath.ExpectedPrimaryPath(p))
			}
		}
	}
	for _, input := range []string{"", "a", "/", "//", "a/", "a/../b", "../a/b", "../../a", "/a//b/../c", "/../../a", "a/./b/", "a/../../b/c", "a\\b", "调度/成员", "./", "../", "a/.."} {
		fmt.Printf("ORACLE\tprefix\t%s\t%s\n", input, keypath.Prefix(input))
	}
}
