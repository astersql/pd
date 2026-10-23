// Copyright 2026 AsterSQL.
package main

import (
	"fmt"

	"github.com/tikv/pd/pkg/utils/keypath"
)

func registryMarkerPaths() {
	emit := func(op string, cluster uint64, name, addr, value string) {
		fmt.Printf("%s\t%d\t%s\t%s\t%s\n", op, cluster, name, addr, value)
	}
	for _, cluster := range []uint64{0, 42, ^uint64(0)} {
		keypath.ResetClusterID()
		keypath.SetClusterID(cluster)
		emit("recovering", cluster, "", "", keypath.RecoveringMarkPath())
		emit("pitr", cluster, "", "", keypath.PitrRestoreModeMarkPath())
		emit("maintenance_prefix", cluster, "", "", keypath.MaintenanceTaskPathPrefix())
		for _, name := range []string{"", "tso", "scheduling", "a/b", "/a//../b/", ".", "..", "维护"} {
			emit("service", cluster, name, "", keypath.ServicePath(name))
			emit("maintenance", cluster, name, "", keypath.MaintenanceTaskPath(name))
			for _, addr := range []string{"", "127.0.0.1:2379", "http://[::1]:2380/a/../", "/a//../b/", "服务"} {
				emit("registry", cluster, name, addr, keypath.RegistryPath(name, addr))
			}
		}
	}
	for _, input := range []string{"", "/", "tso", "a/b", "a/../b", "/a//../b/", "../a/", "/ms/42/tso/registry/", "/ms/42/tso/registry/http://[::1]:2380/a/../"} {
		emit("prefix", 0, input, "", keypath.Prefix(input))
	}
	keypath.ResetClusterID()
}
