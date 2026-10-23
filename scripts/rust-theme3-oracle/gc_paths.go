// Copyright 2026 AsterSQL.
package main

import (
	"fmt"
	"math"

	"github.com/tikv/pd/pkg/utils/keypath"
)

func gcPaths() {
	defer keypath.ResetClusterID()
	for _, cluster := range []uint64{0, 42, math.MaxUint64} {
		keypath.SetClusterID(cluster)
		emit := func(op string, id uint32, name, value string) {
			fmt.Printf("ORACLE\t%d\t%s\t%d\t%s\t%s\n", cluster, op, id, name, value)
		}
		emit("revision", 0, "", keypath.GCStateRevisionPath())
		emit("global_prefix", 0, "", keypath.GlobalGCBarrierPrefix())
		emit("service_prefix", 0, "", keypath.ServiceGCSafePointPrefix())
		emit("v2_prefix", 0, "", keypath.GCSafePointV2Prefix())
		for _, name := range []string{"", keypath.GCWorkerServiceSafePointID, keypath.NativeBRServiceSafePointID, "nested/id", "/a//../b/", "屏障"} {
			emit("global", 0, name, keypath.GlobalGCBarrierPath(name))
			emit("service", 0, name, keypath.ServiceGCSafePointPath(name))
		}
		for _, id := range []uint32{0, 1, 99999999, 100000000, math.MaxUint32 - 1, math.MaxUint32} {
			emit("gc", id, "", keypath.GCSafePointPath(id))
			emit("txn", id, "", keypath.TxnSafePointPath(id))
			emit("barrier_prefix", id, "", keypath.GCBarrierPrefix(id))
			emit("min_start", id, "", keypath.CompatibleTiDBMinStartTSPrefix(id))
			emit("v2_gc", id, "", keypath.GCSafePointV2Path(id))
			emit("v2_service_prefix", id, "", keypath.ServiceSafePointV2Prefix(id))
			for _, name := range []string{"", keypath.GCWorkerServiceSafePointID, keypath.NativeBRServiceSafePointID, "nested/id", "/a//../b/", "屏障"} {
				emit("barrier", id, name, keypath.GCBarrierPath(id, name))
				emit("v2_service", id, name, keypath.ServiceSafePointV2Path(id, name))
			}
		}
	}
}
