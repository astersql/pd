// Copyright 2026 AsterSQL.
package main

import (
	"fmt"

	"github.com/tikv/pd/pkg/utils/keypath"
)

func keyspaceTimePaths() {
	emit := func(op string, cluster uint64, id uint32, name, value string) {
		fmt.Printf("%s\t%d\t%d\t%s\t%s\n", op, cluster, id, name, value)
	}
	for _, cluster := range []uint64{0, 42, ^uint64(0)} {
		keypath.ResetClusterID()
		keypath.SetClusterID(cluster)
		for _, entry := range []struct{ op, value string }{
			{"meta_prefix", keypath.KeyspaceMetaPrefix()},
			{"group_prefix", keypath.KeyspaceGroupIDPrefix()},
			{"resolved", keypath.MinResolvedTSPath()},
			{"external", keypath.ExternalTimestampPath()},
		} {
			emit(entry.op, cluster, 0, "", entry.value)
		}
		for _, id := range []uint32{0, 1, 42, 99999, 100000, 16777215, 99999999, 100000000, ^uint32(0)} {
			emit("meta", cluster, id, "", keypath.KeyspaceMetaPath(id))
			emit("group", cluster, id, "", keypath.KeyspaceGroupIDPath(id))
			emit("timestamp", cluster, id, "", keypath.TimestampPath(id))
		}
		for _, name := range []string{"", "group", "a/b", "/a//../b/", "键空间"} {
			emit("id", cluster, 0, name, keypath.KeyspaceIDPath(name))
			emit("status", cluster, 0, name, keypath.MetaServiceGroupStatusPath(name))
		}
	}
	pattern := keypath.GetCompiledKeyspaceGroupIDRegexp()
	for _, input := range []string{"", "00001", "tso/keyspace_groups/membership/00000", "/pd/42/tso/keyspace_groups/membership/99999", "prefix-tso/keyspace_groups/membership/00042", "tso/keyspace_groups/membership/0001", "tso/keyspace_groups/membership/100000", "tso/keyspace_groups/membership/００００１", "tso/keyspace_groups/membership/00001/", "tso/keyspace_groups/membership/+0001", "tso/keyspace_groups/membership/12a45", "tso/keyspace_groups/membership//00001", "tso/keyspace_groups/membership/4294967295", "tso/keyspace_groups/membership/00001/tso/keyspace_groups/membership/00002"} {
		capture := "<none>"
		if match := pattern.FindStringSubmatch(input); match != nil {
			capture = match[1]
		}
		emit("match", 0, 0, input, capture)
	}
	keypath.ResetClusterID()
}
