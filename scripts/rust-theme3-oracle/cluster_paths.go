// Copyright 2026 AsterSQL.
package main

import (
	"fmt"
	"math"
	"strconv"

	"github.com/tikv/pd/pkg/utils/keypath"
)

func clusterPaths() {
	defer keypath.ResetClusterID()
	fmt.Printf("ORACLE\tcluster_id\t%s\n", keypath.ClusterIDPath)
	for _, cluster := range []uint64{0, 42, math.MaxUint64} {
		keypath.SetClusterID(cluster)
		fmt.Printf("ORACLE\t%d\tcluster\t%s\n", cluster, keypath.ClusterPath())
		fmt.Printf("ORACLE\t%d\tbootstrap\t%s\n", cluster, keypath.ClusterBootstrapTimePath())
		fmt.Printf("ORACLE\t%d\talloc\t%s\n", cluster, keypath.AllocIDPath())
		fmt.Printf("ORACLE\t%d\tkeyspace_alloc\t%s\n", cluster, keypath.KeyspaceAllocIDPath())
		fmt.Printf("ORACLE\t%d\tprefix\t%s\n", cluster, keypath.StorePathPrefix())
		for _, id := range []uint64{0, 1, 42, 9999999999999999999, 10000000000000000000, math.MaxUint64} {
			fmt.Printf("ORACLE\t%d\tstore\t%d\t%s\n", cluster, id, keypath.StorePath(id))
			fmt.Printf("ORACLE\t%d\tregion\t%d\t%s\n", cluster, id, keypath.RegionPath(id))
		}
	}
	keypath.SetClusterID(42)
	inputs := []string{"", "0", "000", "123", "000123", "18446744073709551615", "18446744073709551616", "+1", "-1", " 1", "1_0", "１２", "12x", keypath.StorePath(0), keypath.StorePath(42), "/pd/43/raft/s/00000000000000000042", "/pd/42/raft/s/18446744073709551616", "18446744073709551616x"}
	for _, input := range inputs {
		id, err := keypath.ExtractStoreIDFromPath(input)
		result := strconv.FormatUint(id, 10)
		if err != nil {
			result = "syntax"
			if err.(*strconv.NumError).Err == strconv.ErrRange {
				result = "range"
			}
		}
		fmt.Printf("ORACLE\tparse\t%s\t%s\n", input, result)
	}
}
