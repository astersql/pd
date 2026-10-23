// Copyright 2026 AsterSQL.
use crate::*;

const GO_ORACLE: &str = r#"ORACLE	0	revision	0		/pd/0/gc/gc_state_revision
ORACLE	0	global_prefix	0		/pd/0/gc/global_gc_barriers/
ORACLE	0	service_prefix	0		/pd/0/gc/safe_point/service/
ORACLE	0	v2_prefix	0		/pd/0/keyspaces/gc_safe_point/
ORACLE	0	global	0		/pd/0/gc/global_gc_barriers/
ORACLE	0	service	0		/pd/0/gc/safe_point/service/
ORACLE	0	global	0	gc_worker	/pd/0/gc/global_gc_barriers/gc_worker
ORACLE	0	service	0	gc_worker	/pd/0/gc/safe_point/service/gc_worker
ORACLE	0	global	0	native_br	/pd/0/gc/global_gc_barriers/native_br
ORACLE	0	service	0	native_br	/pd/0/gc/safe_point/service/native_br
ORACLE	0	global	0	nested/id	/pd/0/gc/global_gc_barriers/nested/id
ORACLE	0	service	0	nested/id	/pd/0/gc/safe_point/service/nested/id
ORACLE	0	global	0	/a//../b/	/pd/0/gc/global_gc_barriers//a//../b/
ORACLE	0	service	0	/a//../b/	/pd/0/gc/safe_point/service//a//../b/
ORACLE	0	global	0	屏障	/pd/0/gc/global_gc_barriers/屏障
ORACLE	0	service	0	屏障	/pd/0/gc/safe_point/service/屏障
ORACLE	0	gc	0		/pd/0/keyspaces/gc_safe_point/00000000
ORACLE	0	txn	0		/keyspaces/tidb/0/tidb/store/gcworker/saved_safe_point
ORACLE	0	barrier_prefix	0		/pd/0/keyspaces/service_safe_point/00000000/
ORACLE	0	min_start	0		/keyspaces/tidb/0/tidb/server/minstartts/
ORACLE	0	v2_gc	0		/pd/0/keyspaces/gc_safe_point/00000000
ORACLE	0	v2_service_prefix	0		/pd/0/keyspaces/service_safe_point/00000000/
ORACLE	0	barrier	0		/pd/0/keyspaces/service_safe_point/00000000/
ORACLE	0	v2_service	0		/pd/0/keyspaces/service_safe_point/00000000/
ORACLE	0	barrier	0	gc_worker	/pd/0/keyspaces/service_safe_point/00000000/gc_worker
ORACLE	0	v2_service	0	gc_worker	/pd/0/keyspaces/service_safe_point/00000000/gc_worker
ORACLE	0	barrier	0	native_br	/pd/0/keyspaces/service_safe_point/00000000/native_br
ORACLE	0	v2_service	0	native_br	/pd/0/keyspaces/service_safe_point/00000000/native_br
ORACLE	0	barrier	0	nested/id	/pd/0/keyspaces/service_safe_point/00000000/nested/id
ORACLE	0	v2_service	0	nested/id	/pd/0/keyspaces/service_safe_point/00000000/nested/id
ORACLE	0	barrier	0	/a//../b/	/pd/0/keyspaces/service_safe_point/00000000//a//../b/
ORACLE	0	v2_service	0	/a//../b/	/pd/0/keyspaces/service_safe_point/00000000//a//../b/
ORACLE	0	barrier	0	屏障	/pd/0/keyspaces/service_safe_point/00000000/屏障
ORACLE	0	v2_service	0	屏障	/pd/0/keyspaces/service_safe_point/00000000/屏障
ORACLE	0	gc	1		/pd/0/keyspaces/gc_safe_point/00000001
ORACLE	0	txn	1		/keyspaces/tidb/1/tidb/store/gcworker/saved_safe_point
ORACLE	0	barrier_prefix	1		/pd/0/keyspaces/service_safe_point/00000001/
ORACLE	0	min_start	1		/keyspaces/tidb/1/tidb/server/minstartts/
ORACLE	0	v2_gc	1		/pd/0/keyspaces/gc_safe_point/00000001
ORACLE	0	v2_service_prefix	1		/pd/0/keyspaces/service_safe_point/00000001/
ORACLE	0	barrier	1		/pd/0/keyspaces/service_safe_point/00000001/
ORACLE	0	v2_service	1		/pd/0/keyspaces/service_safe_point/00000001/
ORACLE	0	barrier	1	gc_worker	/pd/0/keyspaces/service_safe_point/00000001/gc_worker
ORACLE	0	v2_service	1	gc_worker	/pd/0/keyspaces/service_safe_point/00000001/gc_worker
ORACLE	0	barrier	1	native_br	/pd/0/keyspaces/service_safe_point/00000001/native_br
ORACLE	0	v2_service	1	native_br	/pd/0/keyspaces/service_safe_point/00000001/native_br
ORACLE	0	barrier	1	nested/id	/pd/0/keyspaces/service_safe_point/00000001/nested/id
ORACLE	0	v2_service	1	nested/id	/pd/0/keyspaces/service_safe_point/00000001/nested/id
ORACLE	0	barrier	1	/a//../b/	/pd/0/keyspaces/service_safe_point/00000001//a//../b/
ORACLE	0	v2_service	1	/a//../b/	/pd/0/keyspaces/service_safe_point/00000001//a//../b/
ORACLE	0	barrier	1	屏障	/pd/0/keyspaces/service_safe_point/00000001/屏障
ORACLE	0	v2_service	1	屏障	/pd/0/keyspaces/service_safe_point/00000001/屏障
ORACLE	0	gc	99999999		/pd/0/keyspaces/gc_safe_point/99999999
ORACLE	0	txn	99999999		/keyspaces/tidb/99999999/tidb/store/gcworker/saved_safe_point
ORACLE	0	barrier_prefix	99999999		/pd/0/keyspaces/service_safe_point/99999999/
ORACLE	0	min_start	99999999		/keyspaces/tidb/99999999/tidb/server/minstartts/
ORACLE	0	v2_gc	99999999		/pd/0/keyspaces/gc_safe_point/99999999
ORACLE	0	v2_service_prefix	99999999		/pd/0/keyspaces/service_safe_point/99999999/
ORACLE	0	barrier	99999999		/pd/0/keyspaces/service_safe_point/99999999/
ORACLE	0	v2_service	99999999		/pd/0/keyspaces/service_safe_point/99999999/
ORACLE	0	barrier	99999999	gc_worker	/pd/0/keyspaces/service_safe_point/99999999/gc_worker
ORACLE	0	v2_service	99999999	gc_worker	/pd/0/keyspaces/service_safe_point/99999999/gc_worker
ORACLE	0	barrier	99999999	native_br	/pd/0/keyspaces/service_safe_point/99999999/native_br
ORACLE	0	v2_service	99999999	native_br	/pd/0/keyspaces/service_safe_point/99999999/native_br
ORACLE	0	barrier	99999999	nested/id	/pd/0/keyspaces/service_safe_point/99999999/nested/id
ORACLE	0	v2_service	99999999	nested/id	/pd/0/keyspaces/service_safe_point/99999999/nested/id
ORACLE	0	barrier	99999999	/a//../b/	/pd/0/keyspaces/service_safe_point/99999999//a//../b/
ORACLE	0	v2_service	99999999	/a//../b/	/pd/0/keyspaces/service_safe_point/99999999//a//../b/
ORACLE	0	barrier	99999999	屏障	/pd/0/keyspaces/service_safe_point/99999999/屏障
ORACLE	0	v2_service	99999999	屏障	/pd/0/keyspaces/service_safe_point/99999999/屏障
ORACLE	0	gc	100000000		/pd/0/keyspaces/gc_safe_point/100000000
ORACLE	0	txn	100000000		/keyspaces/tidb/100000000/tidb/store/gcworker/saved_safe_point
ORACLE	0	barrier_prefix	100000000		/pd/0/keyspaces/service_safe_point/100000000/
ORACLE	0	min_start	100000000		/keyspaces/tidb/100000000/tidb/server/minstartts/
ORACLE	0	v2_gc	100000000		/pd/0/keyspaces/gc_safe_point/100000000
ORACLE	0	v2_service_prefix	100000000		/pd/0/keyspaces/service_safe_point/100000000/
ORACLE	0	barrier	100000000		/pd/0/keyspaces/service_safe_point/100000000/
ORACLE	0	v2_service	100000000		/pd/0/keyspaces/service_safe_point/100000000/
ORACLE	0	barrier	100000000	gc_worker	/pd/0/keyspaces/service_safe_point/100000000/gc_worker
ORACLE	0	v2_service	100000000	gc_worker	/pd/0/keyspaces/service_safe_point/100000000/gc_worker
ORACLE	0	barrier	100000000	native_br	/pd/0/keyspaces/service_safe_point/100000000/native_br
ORACLE	0	v2_service	100000000	native_br	/pd/0/keyspaces/service_safe_point/100000000/native_br
ORACLE	0	barrier	100000000	nested/id	/pd/0/keyspaces/service_safe_point/100000000/nested/id
ORACLE	0	v2_service	100000000	nested/id	/pd/0/keyspaces/service_safe_point/100000000/nested/id
ORACLE	0	barrier	100000000	/a//../b/	/pd/0/keyspaces/service_safe_point/100000000//a//../b/
ORACLE	0	v2_service	100000000	/a//../b/	/pd/0/keyspaces/service_safe_point/100000000//a//../b/
ORACLE	0	barrier	100000000	屏障	/pd/0/keyspaces/service_safe_point/100000000/屏障
ORACLE	0	v2_service	100000000	屏障	/pd/0/keyspaces/service_safe_point/100000000/屏障
ORACLE	0	gc	4294967294		/pd/0/keyspaces/gc_safe_point/4294967294
ORACLE	0	txn	4294967294		/keyspaces/tidb/4294967294/tidb/store/gcworker/saved_safe_point
ORACLE	0	barrier_prefix	4294967294		/pd/0/keyspaces/service_safe_point/4294967294/
ORACLE	0	min_start	4294967294		/keyspaces/tidb/4294967294/tidb/server/minstartts/
ORACLE	0	v2_gc	4294967294		/pd/0/keyspaces/gc_safe_point/4294967294
ORACLE	0	v2_service_prefix	4294967294		/pd/0/keyspaces/service_safe_point/4294967294/
ORACLE	0	barrier	4294967294		/pd/0/keyspaces/service_safe_point/4294967294/
ORACLE	0	v2_service	4294967294		/pd/0/keyspaces/service_safe_point/4294967294/
ORACLE	0	barrier	4294967294	gc_worker	/pd/0/keyspaces/service_safe_point/4294967294/gc_worker
ORACLE	0	v2_service	4294967294	gc_worker	/pd/0/keyspaces/service_safe_point/4294967294/gc_worker
ORACLE	0	barrier	4294967294	native_br	/pd/0/keyspaces/service_safe_point/4294967294/native_br
ORACLE	0	v2_service	4294967294	native_br	/pd/0/keyspaces/service_safe_point/4294967294/native_br
ORACLE	0	barrier	4294967294	nested/id	/pd/0/keyspaces/service_safe_point/4294967294/nested/id
ORACLE	0	v2_service	4294967294	nested/id	/pd/0/keyspaces/service_safe_point/4294967294/nested/id
ORACLE	0	barrier	4294967294	/a//../b/	/pd/0/keyspaces/service_safe_point/4294967294//a//../b/
ORACLE	0	v2_service	4294967294	/a//../b/	/pd/0/keyspaces/service_safe_point/4294967294//a//../b/
ORACLE	0	barrier	4294967294	屏障	/pd/0/keyspaces/service_safe_point/4294967294/屏障
ORACLE	0	v2_service	4294967294	屏障	/pd/0/keyspaces/service_safe_point/4294967294/屏障
ORACLE	0	gc	4294967295		/pd/0/gc/safe_point
ORACLE	0	txn	4294967295		/tidb/store/gcworker/saved_safe_point
ORACLE	0	barrier_prefix	4294967295		/pd/0/gc/safe_point/service/
ORACLE	0	min_start	4294967295		/tidb/server/minstartts/
ORACLE	0	v2_gc	4294967295		/pd/0/keyspaces/gc_safe_point/4294967295
ORACLE	0	v2_service_prefix	4294967295		/pd/0/keyspaces/service_safe_point/4294967295/
ORACLE	0	barrier	4294967295		/pd/0/gc/safe_point/service/
ORACLE	0	v2_service	4294967295		/pd/0/keyspaces/service_safe_point/4294967295/
ORACLE	0	barrier	4294967295	gc_worker	/pd/0/gc/safe_point/service/gc_worker
ORACLE	0	v2_service	4294967295	gc_worker	/pd/0/keyspaces/service_safe_point/4294967295/gc_worker
ORACLE	0	barrier	4294967295	native_br	/pd/0/gc/safe_point/service/native_br
ORACLE	0	v2_service	4294967295	native_br	/pd/0/keyspaces/service_safe_point/4294967295/native_br
ORACLE	0	barrier	4294967295	nested/id	/pd/0/gc/safe_point/service/nested/id
ORACLE	0	v2_service	4294967295	nested/id	/pd/0/keyspaces/service_safe_point/4294967295/nested/id
ORACLE	0	barrier	4294967295	/a//../b/	/pd/0/gc/safe_point/service//a//../b/
ORACLE	0	v2_service	4294967295	/a//../b/	/pd/0/keyspaces/service_safe_point/4294967295//a//../b/
ORACLE	0	barrier	4294967295	屏障	/pd/0/gc/safe_point/service/屏障
ORACLE	0	v2_service	4294967295	屏障	/pd/0/keyspaces/service_safe_point/4294967295/屏障
ORACLE	42	revision	0		/pd/42/gc/gc_state_revision
ORACLE	42	global_prefix	0		/pd/42/gc/global_gc_barriers/
ORACLE	42	service_prefix	0		/pd/42/gc/safe_point/service/
ORACLE	42	v2_prefix	0		/pd/42/keyspaces/gc_safe_point/
ORACLE	42	global	0		/pd/42/gc/global_gc_barriers/
ORACLE	42	service	0		/pd/42/gc/safe_point/service/
ORACLE	42	global	0	gc_worker	/pd/42/gc/global_gc_barriers/gc_worker
ORACLE	42	service	0	gc_worker	/pd/42/gc/safe_point/service/gc_worker
ORACLE	42	global	0	native_br	/pd/42/gc/global_gc_barriers/native_br
ORACLE	42	service	0	native_br	/pd/42/gc/safe_point/service/native_br
ORACLE	42	global	0	nested/id	/pd/42/gc/global_gc_barriers/nested/id
ORACLE	42	service	0	nested/id	/pd/42/gc/safe_point/service/nested/id
ORACLE	42	global	0	/a//../b/	/pd/42/gc/global_gc_barriers//a//../b/
ORACLE	42	service	0	/a//../b/	/pd/42/gc/safe_point/service//a//../b/
ORACLE	42	global	0	屏障	/pd/42/gc/global_gc_barriers/屏障
ORACLE	42	service	0	屏障	/pd/42/gc/safe_point/service/屏障
ORACLE	42	gc	0		/pd/42/keyspaces/gc_safe_point/00000000
ORACLE	42	txn	0		/keyspaces/tidb/0/tidb/store/gcworker/saved_safe_point
ORACLE	42	barrier_prefix	0		/pd/42/keyspaces/service_safe_point/00000000/
ORACLE	42	min_start	0		/keyspaces/tidb/0/tidb/server/minstartts/
ORACLE	42	v2_gc	0		/pd/42/keyspaces/gc_safe_point/00000000
ORACLE	42	v2_service_prefix	0		/pd/42/keyspaces/service_safe_point/00000000/
ORACLE	42	barrier	0		/pd/42/keyspaces/service_safe_point/00000000/
ORACLE	42	v2_service	0		/pd/42/keyspaces/service_safe_point/00000000/
ORACLE	42	barrier	0	gc_worker	/pd/42/keyspaces/service_safe_point/00000000/gc_worker
ORACLE	42	v2_service	0	gc_worker	/pd/42/keyspaces/service_safe_point/00000000/gc_worker
ORACLE	42	barrier	0	native_br	/pd/42/keyspaces/service_safe_point/00000000/native_br
ORACLE	42	v2_service	0	native_br	/pd/42/keyspaces/service_safe_point/00000000/native_br
ORACLE	42	barrier	0	nested/id	/pd/42/keyspaces/service_safe_point/00000000/nested/id
ORACLE	42	v2_service	0	nested/id	/pd/42/keyspaces/service_safe_point/00000000/nested/id
ORACLE	42	barrier	0	/a//../b/	/pd/42/keyspaces/service_safe_point/00000000//a//../b/
ORACLE	42	v2_service	0	/a//../b/	/pd/42/keyspaces/service_safe_point/00000000//a//../b/
ORACLE	42	barrier	0	屏障	/pd/42/keyspaces/service_safe_point/00000000/屏障
ORACLE	42	v2_service	0	屏障	/pd/42/keyspaces/service_safe_point/00000000/屏障
ORACLE	42	gc	1		/pd/42/keyspaces/gc_safe_point/00000001
ORACLE	42	txn	1		/keyspaces/tidb/1/tidb/store/gcworker/saved_safe_point
ORACLE	42	barrier_prefix	1		/pd/42/keyspaces/service_safe_point/00000001/
ORACLE	42	min_start	1		/keyspaces/tidb/1/tidb/server/minstartts/
ORACLE	42	v2_gc	1		/pd/42/keyspaces/gc_safe_point/00000001
ORACLE	42	v2_service_prefix	1		/pd/42/keyspaces/service_safe_point/00000001/
ORACLE	42	barrier	1		/pd/42/keyspaces/service_safe_point/00000001/
ORACLE	42	v2_service	1		/pd/42/keyspaces/service_safe_point/00000001/
ORACLE	42	barrier	1	gc_worker	/pd/42/keyspaces/service_safe_point/00000001/gc_worker
ORACLE	42	v2_service	1	gc_worker	/pd/42/keyspaces/service_safe_point/00000001/gc_worker
ORACLE	42	barrier	1	native_br	/pd/42/keyspaces/service_safe_point/00000001/native_br
ORACLE	42	v2_service	1	native_br	/pd/42/keyspaces/service_safe_point/00000001/native_br
ORACLE	42	barrier	1	nested/id	/pd/42/keyspaces/service_safe_point/00000001/nested/id
ORACLE	42	v2_service	1	nested/id	/pd/42/keyspaces/service_safe_point/00000001/nested/id
ORACLE	42	barrier	1	/a//../b/	/pd/42/keyspaces/service_safe_point/00000001//a//../b/
ORACLE	42	v2_service	1	/a//../b/	/pd/42/keyspaces/service_safe_point/00000001//a//../b/
ORACLE	42	barrier	1	屏障	/pd/42/keyspaces/service_safe_point/00000001/屏障
ORACLE	42	v2_service	1	屏障	/pd/42/keyspaces/service_safe_point/00000001/屏障
ORACLE	42	gc	99999999		/pd/42/keyspaces/gc_safe_point/99999999
ORACLE	42	txn	99999999		/keyspaces/tidb/99999999/tidb/store/gcworker/saved_safe_point
ORACLE	42	barrier_prefix	99999999		/pd/42/keyspaces/service_safe_point/99999999/
ORACLE	42	min_start	99999999		/keyspaces/tidb/99999999/tidb/server/minstartts/
ORACLE	42	v2_gc	99999999		/pd/42/keyspaces/gc_safe_point/99999999
ORACLE	42	v2_service_prefix	99999999		/pd/42/keyspaces/service_safe_point/99999999/
ORACLE	42	barrier	99999999		/pd/42/keyspaces/service_safe_point/99999999/
ORACLE	42	v2_service	99999999		/pd/42/keyspaces/service_safe_point/99999999/
ORACLE	42	barrier	99999999	gc_worker	/pd/42/keyspaces/service_safe_point/99999999/gc_worker
ORACLE	42	v2_service	99999999	gc_worker	/pd/42/keyspaces/service_safe_point/99999999/gc_worker
ORACLE	42	barrier	99999999	native_br	/pd/42/keyspaces/service_safe_point/99999999/native_br
ORACLE	42	v2_service	99999999	native_br	/pd/42/keyspaces/service_safe_point/99999999/native_br
ORACLE	42	barrier	99999999	nested/id	/pd/42/keyspaces/service_safe_point/99999999/nested/id
ORACLE	42	v2_service	99999999	nested/id	/pd/42/keyspaces/service_safe_point/99999999/nested/id
ORACLE	42	barrier	99999999	/a//../b/	/pd/42/keyspaces/service_safe_point/99999999//a//../b/
ORACLE	42	v2_service	99999999	/a//../b/	/pd/42/keyspaces/service_safe_point/99999999//a//../b/
ORACLE	42	barrier	99999999	屏障	/pd/42/keyspaces/service_safe_point/99999999/屏障
ORACLE	42	v2_service	99999999	屏障	/pd/42/keyspaces/service_safe_point/99999999/屏障
ORACLE	42	gc	100000000		/pd/42/keyspaces/gc_safe_point/100000000
ORACLE	42	txn	100000000		/keyspaces/tidb/100000000/tidb/store/gcworker/saved_safe_point
ORACLE	42	barrier_prefix	100000000		/pd/42/keyspaces/service_safe_point/100000000/
ORACLE	42	min_start	100000000		/keyspaces/tidb/100000000/tidb/server/minstartts/
ORACLE	42	v2_gc	100000000		/pd/42/keyspaces/gc_safe_point/100000000
ORACLE	42	v2_service_prefix	100000000		/pd/42/keyspaces/service_safe_point/100000000/
ORACLE	42	barrier	100000000		/pd/42/keyspaces/service_safe_point/100000000/
ORACLE	42	v2_service	100000000		/pd/42/keyspaces/service_safe_point/100000000/
ORACLE	42	barrier	100000000	gc_worker	/pd/42/keyspaces/service_safe_point/100000000/gc_worker
ORACLE	42	v2_service	100000000	gc_worker	/pd/42/keyspaces/service_safe_point/100000000/gc_worker
ORACLE	42	barrier	100000000	native_br	/pd/42/keyspaces/service_safe_point/100000000/native_br
ORACLE	42	v2_service	100000000	native_br	/pd/42/keyspaces/service_safe_point/100000000/native_br
ORACLE	42	barrier	100000000	nested/id	/pd/42/keyspaces/service_safe_point/100000000/nested/id
ORACLE	42	v2_service	100000000	nested/id	/pd/42/keyspaces/service_safe_point/100000000/nested/id
ORACLE	42	barrier	100000000	/a//../b/	/pd/42/keyspaces/service_safe_point/100000000//a//../b/
ORACLE	42	v2_service	100000000	/a//../b/	/pd/42/keyspaces/service_safe_point/100000000//a//../b/
ORACLE	42	barrier	100000000	屏障	/pd/42/keyspaces/service_safe_point/100000000/屏障
ORACLE	42	v2_service	100000000	屏障	/pd/42/keyspaces/service_safe_point/100000000/屏障
ORACLE	42	gc	4294967294		/pd/42/keyspaces/gc_safe_point/4294967294
ORACLE	42	txn	4294967294		/keyspaces/tidb/4294967294/tidb/store/gcworker/saved_safe_point
ORACLE	42	barrier_prefix	4294967294		/pd/42/keyspaces/service_safe_point/4294967294/
ORACLE	42	min_start	4294967294		/keyspaces/tidb/4294967294/tidb/server/minstartts/
ORACLE	42	v2_gc	4294967294		/pd/42/keyspaces/gc_safe_point/4294967294
ORACLE	42	v2_service_prefix	4294967294		/pd/42/keyspaces/service_safe_point/4294967294/
ORACLE	42	barrier	4294967294		/pd/42/keyspaces/service_safe_point/4294967294/
ORACLE	42	v2_service	4294967294		/pd/42/keyspaces/service_safe_point/4294967294/
ORACLE	42	barrier	4294967294	gc_worker	/pd/42/keyspaces/service_safe_point/4294967294/gc_worker
ORACLE	42	v2_service	4294967294	gc_worker	/pd/42/keyspaces/service_safe_point/4294967294/gc_worker
ORACLE	42	barrier	4294967294	native_br	/pd/42/keyspaces/service_safe_point/4294967294/native_br
ORACLE	42	v2_service	4294967294	native_br	/pd/42/keyspaces/service_safe_point/4294967294/native_br
ORACLE	42	barrier	4294967294	nested/id	/pd/42/keyspaces/service_safe_point/4294967294/nested/id
ORACLE	42	v2_service	4294967294	nested/id	/pd/42/keyspaces/service_safe_point/4294967294/nested/id
ORACLE	42	barrier	4294967294	/a//../b/	/pd/42/keyspaces/service_safe_point/4294967294//a//../b/
ORACLE	42	v2_service	4294967294	/a//../b/	/pd/42/keyspaces/service_safe_point/4294967294//a//../b/
ORACLE	42	barrier	4294967294	屏障	/pd/42/keyspaces/service_safe_point/4294967294/屏障
ORACLE	42	v2_service	4294967294	屏障	/pd/42/keyspaces/service_safe_point/4294967294/屏障
ORACLE	42	gc	4294967295		/pd/42/gc/safe_point
ORACLE	42	txn	4294967295		/tidb/store/gcworker/saved_safe_point
ORACLE	42	barrier_prefix	4294967295		/pd/42/gc/safe_point/service/
ORACLE	42	min_start	4294967295		/tidb/server/minstartts/
ORACLE	42	v2_gc	4294967295		/pd/42/keyspaces/gc_safe_point/4294967295
ORACLE	42	v2_service_prefix	4294967295		/pd/42/keyspaces/service_safe_point/4294967295/
ORACLE	42	barrier	4294967295		/pd/42/gc/safe_point/service/
ORACLE	42	v2_service	4294967295		/pd/42/keyspaces/service_safe_point/4294967295/
ORACLE	42	barrier	4294967295	gc_worker	/pd/42/gc/safe_point/service/gc_worker
ORACLE	42	v2_service	4294967295	gc_worker	/pd/42/keyspaces/service_safe_point/4294967295/gc_worker
ORACLE	42	barrier	4294967295	native_br	/pd/42/gc/safe_point/service/native_br
ORACLE	42	v2_service	4294967295	native_br	/pd/42/keyspaces/service_safe_point/4294967295/native_br
ORACLE	42	barrier	4294967295	nested/id	/pd/42/gc/safe_point/service/nested/id
ORACLE	42	v2_service	4294967295	nested/id	/pd/42/keyspaces/service_safe_point/4294967295/nested/id
ORACLE	42	barrier	4294967295	/a//../b/	/pd/42/gc/safe_point/service//a//../b/
ORACLE	42	v2_service	4294967295	/a//../b/	/pd/42/keyspaces/service_safe_point/4294967295//a//../b/
ORACLE	42	barrier	4294967295	屏障	/pd/42/gc/safe_point/service/屏障
ORACLE	42	v2_service	4294967295	屏障	/pd/42/keyspaces/service_safe_point/4294967295/屏障
ORACLE	18446744073709551615	revision	0		/pd/18446744073709551615/gc/gc_state_revision
ORACLE	18446744073709551615	global_prefix	0		/pd/18446744073709551615/gc/global_gc_barriers/
ORACLE	18446744073709551615	service_prefix	0		/pd/18446744073709551615/gc/safe_point/service/
ORACLE	18446744073709551615	v2_prefix	0		/pd/18446744073709551615/keyspaces/gc_safe_point/
ORACLE	18446744073709551615	global	0		/pd/18446744073709551615/gc/global_gc_barriers/
ORACLE	18446744073709551615	service	0		/pd/18446744073709551615/gc/safe_point/service/
ORACLE	18446744073709551615	global	0	gc_worker	/pd/18446744073709551615/gc/global_gc_barriers/gc_worker
ORACLE	18446744073709551615	service	0	gc_worker	/pd/18446744073709551615/gc/safe_point/service/gc_worker
ORACLE	18446744073709551615	global	0	native_br	/pd/18446744073709551615/gc/global_gc_barriers/native_br
ORACLE	18446744073709551615	service	0	native_br	/pd/18446744073709551615/gc/safe_point/service/native_br
ORACLE	18446744073709551615	global	0	nested/id	/pd/18446744073709551615/gc/global_gc_barriers/nested/id
ORACLE	18446744073709551615	service	0	nested/id	/pd/18446744073709551615/gc/safe_point/service/nested/id
ORACLE	18446744073709551615	global	0	/a//../b/	/pd/18446744073709551615/gc/global_gc_barriers//a//../b/
ORACLE	18446744073709551615	service	0	/a//../b/	/pd/18446744073709551615/gc/safe_point/service//a//../b/
ORACLE	18446744073709551615	global	0	屏障	/pd/18446744073709551615/gc/global_gc_barriers/屏障
ORACLE	18446744073709551615	service	0	屏障	/pd/18446744073709551615/gc/safe_point/service/屏障
ORACLE	18446744073709551615	gc	0		/pd/18446744073709551615/keyspaces/gc_safe_point/00000000
ORACLE	18446744073709551615	txn	0		/keyspaces/tidb/0/tidb/store/gcworker/saved_safe_point
ORACLE	18446744073709551615	barrier_prefix	0		/pd/18446744073709551615/keyspaces/service_safe_point/00000000/
ORACLE	18446744073709551615	min_start	0		/keyspaces/tidb/0/tidb/server/minstartts/
ORACLE	18446744073709551615	v2_gc	0		/pd/18446744073709551615/keyspaces/gc_safe_point/00000000
ORACLE	18446744073709551615	v2_service_prefix	0		/pd/18446744073709551615/keyspaces/service_safe_point/00000000/
ORACLE	18446744073709551615	barrier	0		/pd/18446744073709551615/keyspaces/service_safe_point/00000000/
ORACLE	18446744073709551615	v2_service	0		/pd/18446744073709551615/keyspaces/service_safe_point/00000000/
ORACLE	18446744073709551615	barrier	0	gc_worker	/pd/18446744073709551615/keyspaces/service_safe_point/00000000/gc_worker
ORACLE	18446744073709551615	v2_service	0	gc_worker	/pd/18446744073709551615/keyspaces/service_safe_point/00000000/gc_worker
ORACLE	18446744073709551615	barrier	0	native_br	/pd/18446744073709551615/keyspaces/service_safe_point/00000000/native_br
ORACLE	18446744073709551615	v2_service	0	native_br	/pd/18446744073709551615/keyspaces/service_safe_point/00000000/native_br
ORACLE	18446744073709551615	barrier	0	nested/id	/pd/18446744073709551615/keyspaces/service_safe_point/00000000/nested/id
ORACLE	18446744073709551615	v2_service	0	nested/id	/pd/18446744073709551615/keyspaces/service_safe_point/00000000/nested/id
ORACLE	18446744073709551615	barrier	0	/a//../b/	/pd/18446744073709551615/keyspaces/service_safe_point/00000000//a//../b/
ORACLE	18446744073709551615	v2_service	0	/a//../b/	/pd/18446744073709551615/keyspaces/service_safe_point/00000000//a//../b/
ORACLE	18446744073709551615	barrier	0	屏障	/pd/18446744073709551615/keyspaces/service_safe_point/00000000/屏障
ORACLE	18446744073709551615	v2_service	0	屏障	/pd/18446744073709551615/keyspaces/service_safe_point/00000000/屏障
ORACLE	18446744073709551615	gc	1		/pd/18446744073709551615/keyspaces/gc_safe_point/00000001
ORACLE	18446744073709551615	txn	1		/keyspaces/tidb/1/tidb/store/gcworker/saved_safe_point
ORACLE	18446744073709551615	barrier_prefix	1		/pd/18446744073709551615/keyspaces/service_safe_point/00000001/
ORACLE	18446744073709551615	min_start	1		/keyspaces/tidb/1/tidb/server/minstartts/
ORACLE	18446744073709551615	v2_gc	1		/pd/18446744073709551615/keyspaces/gc_safe_point/00000001
ORACLE	18446744073709551615	v2_service_prefix	1		/pd/18446744073709551615/keyspaces/service_safe_point/00000001/
ORACLE	18446744073709551615	barrier	1		/pd/18446744073709551615/keyspaces/service_safe_point/00000001/
ORACLE	18446744073709551615	v2_service	1		/pd/18446744073709551615/keyspaces/service_safe_point/00000001/
ORACLE	18446744073709551615	barrier	1	gc_worker	/pd/18446744073709551615/keyspaces/service_safe_point/00000001/gc_worker
ORACLE	18446744073709551615	v2_service	1	gc_worker	/pd/18446744073709551615/keyspaces/service_safe_point/00000001/gc_worker
ORACLE	18446744073709551615	barrier	1	native_br	/pd/18446744073709551615/keyspaces/service_safe_point/00000001/native_br
ORACLE	18446744073709551615	v2_service	1	native_br	/pd/18446744073709551615/keyspaces/service_safe_point/00000001/native_br
ORACLE	18446744073709551615	barrier	1	nested/id	/pd/18446744073709551615/keyspaces/service_safe_point/00000001/nested/id
ORACLE	18446744073709551615	v2_service	1	nested/id	/pd/18446744073709551615/keyspaces/service_safe_point/00000001/nested/id
ORACLE	18446744073709551615	barrier	1	/a//../b/	/pd/18446744073709551615/keyspaces/service_safe_point/00000001//a//../b/
ORACLE	18446744073709551615	v2_service	1	/a//../b/	/pd/18446744073709551615/keyspaces/service_safe_point/00000001//a//../b/
ORACLE	18446744073709551615	barrier	1	屏障	/pd/18446744073709551615/keyspaces/service_safe_point/00000001/屏障
ORACLE	18446744073709551615	v2_service	1	屏障	/pd/18446744073709551615/keyspaces/service_safe_point/00000001/屏障
ORACLE	18446744073709551615	gc	99999999		/pd/18446744073709551615/keyspaces/gc_safe_point/99999999
ORACLE	18446744073709551615	txn	99999999		/keyspaces/tidb/99999999/tidb/store/gcworker/saved_safe_point
ORACLE	18446744073709551615	barrier_prefix	99999999		/pd/18446744073709551615/keyspaces/service_safe_point/99999999/
ORACLE	18446744073709551615	min_start	99999999		/keyspaces/tidb/99999999/tidb/server/minstartts/
ORACLE	18446744073709551615	v2_gc	99999999		/pd/18446744073709551615/keyspaces/gc_safe_point/99999999
ORACLE	18446744073709551615	v2_service_prefix	99999999		/pd/18446744073709551615/keyspaces/service_safe_point/99999999/
ORACLE	18446744073709551615	barrier	99999999		/pd/18446744073709551615/keyspaces/service_safe_point/99999999/
ORACLE	18446744073709551615	v2_service	99999999		/pd/18446744073709551615/keyspaces/service_safe_point/99999999/
ORACLE	18446744073709551615	barrier	99999999	gc_worker	/pd/18446744073709551615/keyspaces/service_safe_point/99999999/gc_worker
ORACLE	18446744073709551615	v2_service	99999999	gc_worker	/pd/18446744073709551615/keyspaces/service_safe_point/99999999/gc_worker
ORACLE	18446744073709551615	barrier	99999999	native_br	/pd/18446744073709551615/keyspaces/service_safe_point/99999999/native_br
ORACLE	18446744073709551615	v2_service	99999999	native_br	/pd/18446744073709551615/keyspaces/service_safe_point/99999999/native_br
ORACLE	18446744073709551615	barrier	99999999	nested/id	/pd/18446744073709551615/keyspaces/service_safe_point/99999999/nested/id
ORACLE	18446744073709551615	v2_service	99999999	nested/id	/pd/18446744073709551615/keyspaces/service_safe_point/99999999/nested/id
ORACLE	18446744073709551615	barrier	99999999	/a//../b/	/pd/18446744073709551615/keyspaces/service_safe_point/99999999//a//../b/
ORACLE	18446744073709551615	v2_service	99999999	/a//../b/	/pd/18446744073709551615/keyspaces/service_safe_point/99999999//a//../b/
ORACLE	18446744073709551615	barrier	99999999	屏障	/pd/18446744073709551615/keyspaces/service_safe_point/99999999/屏障
ORACLE	18446744073709551615	v2_service	99999999	屏障	/pd/18446744073709551615/keyspaces/service_safe_point/99999999/屏障
ORACLE	18446744073709551615	gc	100000000		/pd/18446744073709551615/keyspaces/gc_safe_point/100000000
ORACLE	18446744073709551615	txn	100000000		/keyspaces/tidb/100000000/tidb/store/gcworker/saved_safe_point
ORACLE	18446744073709551615	barrier_prefix	100000000		/pd/18446744073709551615/keyspaces/service_safe_point/100000000/
ORACLE	18446744073709551615	min_start	100000000		/keyspaces/tidb/100000000/tidb/server/minstartts/
ORACLE	18446744073709551615	v2_gc	100000000		/pd/18446744073709551615/keyspaces/gc_safe_point/100000000
ORACLE	18446744073709551615	v2_service_prefix	100000000		/pd/18446744073709551615/keyspaces/service_safe_point/100000000/
ORACLE	18446744073709551615	barrier	100000000		/pd/18446744073709551615/keyspaces/service_safe_point/100000000/
ORACLE	18446744073709551615	v2_service	100000000		/pd/18446744073709551615/keyspaces/service_safe_point/100000000/
ORACLE	18446744073709551615	barrier	100000000	gc_worker	/pd/18446744073709551615/keyspaces/service_safe_point/100000000/gc_worker
ORACLE	18446744073709551615	v2_service	100000000	gc_worker	/pd/18446744073709551615/keyspaces/service_safe_point/100000000/gc_worker
ORACLE	18446744073709551615	barrier	100000000	native_br	/pd/18446744073709551615/keyspaces/service_safe_point/100000000/native_br
ORACLE	18446744073709551615	v2_service	100000000	native_br	/pd/18446744073709551615/keyspaces/service_safe_point/100000000/native_br
ORACLE	18446744073709551615	barrier	100000000	nested/id	/pd/18446744073709551615/keyspaces/service_safe_point/100000000/nested/id
ORACLE	18446744073709551615	v2_service	100000000	nested/id	/pd/18446744073709551615/keyspaces/service_safe_point/100000000/nested/id
ORACLE	18446744073709551615	barrier	100000000	/a//../b/	/pd/18446744073709551615/keyspaces/service_safe_point/100000000//a//../b/
ORACLE	18446744073709551615	v2_service	100000000	/a//../b/	/pd/18446744073709551615/keyspaces/service_safe_point/100000000//a//../b/
ORACLE	18446744073709551615	barrier	100000000	屏障	/pd/18446744073709551615/keyspaces/service_safe_point/100000000/屏障
ORACLE	18446744073709551615	v2_service	100000000	屏障	/pd/18446744073709551615/keyspaces/service_safe_point/100000000/屏障
ORACLE	18446744073709551615	gc	4294967294		/pd/18446744073709551615/keyspaces/gc_safe_point/4294967294
ORACLE	18446744073709551615	txn	4294967294		/keyspaces/tidb/4294967294/tidb/store/gcworker/saved_safe_point
ORACLE	18446744073709551615	barrier_prefix	4294967294		/pd/18446744073709551615/keyspaces/service_safe_point/4294967294/
ORACLE	18446744073709551615	min_start	4294967294		/keyspaces/tidb/4294967294/tidb/server/minstartts/
ORACLE	18446744073709551615	v2_gc	4294967294		/pd/18446744073709551615/keyspaces/gc_safe_point/4294967294
ORACLE	18446744073709551615	v2_service_prefix	4294967294		/pd/18446744073709551615/keyspaces/service_safe_point/4294967294/
ORACLE	18446744073709551615	barrier	4294967294		/pd/18446744073709551615/keyspaces/service_safe_point/4294967294/
ORACLE	18446744073709551615	v2_service	4294967294		/pd/18446744073709551615/keyspaces/service_safe_point/4294967294/
ORACLE	18446744073709551615	barrier	4294967294	gc_worker	/pd/18446744073709551615/keyspaces/service_safe_point/4294967294/gc_worker
ORACLE	18446744073709551615	v2_service	4294967294	gc_worker	/pd/18446744073709551615/keyspaces/service_safe_point/4294967294/gc_worker
ORACLE	18446744073709551615	barrier	4294967294	native_br	/pd/18446744073709551615/keyspaces/service_safe_point/4294967294/native_br
ORACLE	18446744073709551615	v2_service	4294967294	native_br	/pd/18446744073709551615/keyspaces/service_safe_point/4294967294/native_br
ORACLE	18446744073709551615	barrier	4294967294	nested/id	/pd/18446744073709551615/keyspaces/service_safe_point/4294967294/nested/id
ORACLE	18446744073709551615	v2_service	4294967294	nested/id	/pd/18446744073709551615/keyspaces/service_safe_point/4294967294/nested/id
ORACLE	18446744073709551615	barrier	4294967294	/a//../b/	/pd/18446744073709551615/keyspaces/service_safe_point/4294967294//a//../b/
ORACLE	18446744073709551615	v2_service	4294967294	/a//../b/	/pd/18446744073709551615/keyspaces/service_safe_point/4294967294//a//../b/
ORACLE	18446744073709551615	barrier	4294967294	屏障	/pd/18446744073709551615/keyspaces/service_safe_point/4294967294/屏障
ORACLE	18446744073709551615	v2_service	4294967294	屏障	/pd/18446744073709551615/keyspaces/service_safe_point/4294967294/屏障
ORACLE	18446744073709551615	gc	4294967295		/pd/18446744073709551615/gc/safe_point
ORACLE	18446744073709551615	txn	4294967295		/tidb/store/gcworker/saved_safe_point
ORACLE	18446744073709551615	barrier_prefix	4294967295		/pd/18446744073709551615/gc/safe_point/service/
ORACLE	18446744073709551615	min_start	4294967295		/tidb/server/minstartts/
ORACLE	18446744073709551615	v2_gc	4294967295		/pd/18446744073709551615/keyspaces/gc_safe_point/4294967295
ORACLE	18446744073709551615	v2_service_prefix	4294967295		/pd/18446744073709551615/keyspaces/service_safe_point/4294967295/
ORACLE	18446744073709551615	barrier	4294967295		/pd/18446744073709551615/gc/safe_point/service/
ORACLE	18446744073709551615	v2_service	4294967295		/pd/18446744073709551615/keyspaces/service_safe_point/4294967295/
ORACLE	18446744073709551615	barrier	4294967295	gc_worker	/pd/18446744073709551615/gc/safe_point/service/gc_worker
ORACLE	18446744073709551615	v2_service	4294967295	gc_worker	/pd/18446744073709551615/keyspaces/service_safe_point/4294967295/gc_worker
ORACLE	18446744073709551615	barrier	4294967295	native_br	/pd/18446744073709551615/gc/safe_point/service/native_br
ORACLE	18446744073709551615	v2_service	4294967295	native_br	/pd/18446744073709551615/keyspaces/service_safe_point/4294967295/native_br
ORACLE	18446744073709551615	barrier	4294967295	nested/id	/pd/18446744073709551615/gc/safe_point/service/nested/id
ORACLE	18446744073709551615	v2_service	4294967295	nested/id	/pd/18446744073709551615/keyspaces/service_safe_point/4294967295/nested/id
ORACLE	18446744073709551615	barrier	4294967295	/a//../b/	/pd/18446744073709551615/gc/safe_point/service//a//../b/
ORACLE	18446744073709551615	v2_service	4294967295	/a//../b/	/pd/18446744073709551615/keyspaces/service_safe_point/4294967295//a//../b/
ORACLE	18446744073709551615	barrier	4294967295	屏障	/pd/18446744073709551615/gc/safe_point/service/屏障
ORACLE	18446744073709551615	v2_service	4294967295	屏障	/pd/18446744073709551615/keyspaces/service_safe_point/4294967295/屏障
"#;

#[test]
fn gc_paths_go_byte_oracle() {
    let mut count = 0;
    for line in GO_ORACLE.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 6);
        let paths = ClusterPaths::new(fields[1].parse().unwrap());
        let id = fields[3].parse().unwrap();
        let name = fields[4];
        let actual = match fields[2] {
            "revision" => paths.gc_state_revision_path(),
            "global_prefix" => paths.global_gc_barrier_prefix(),
            "service_prefix" => paths.service_gc_safe_point_prefix(),
            "v2_prefix" => paths.gc_safe_point_v2_prefix(),
            "global" => paths.global_gc_barrier_path(name),
            "service" => paths.service_gc_safe_point_path(name),
            "gc" => paths.gc_safe_point_path(id),
            "txn" => paths.txn_safe_point_path(id),
            "barrier_prefix" => paths.gc_barrier_prefix(id),
            "min_start" => paths.compatible_tidb_min_start_ts_prefix(id),
            "v2_gc" => paths.gc_safe_point_v2_path(id),
            "v2_service_prefix" => paths.service_safe_point_v2_prefix(id),
            "barrier" => paths.gc_barrier_path(id, name),
            "v2_service" => paths.service_safe_point_v2_path(id, name),
            op => panic!("unknown operation {op}"),
        };
        assert_eq!(actual.as_bytes(), fields[5].as_bytes(), "{line}");
        println!(
            "ORACLE\t{}\t{}\t{}\t{}\t{actual}",
            fields[1], fields[2], id, name
        );
        count += 1;
    }
    assert_eq!(count, 372);
}

#[test]
fn gc_paths_context_and_aliases() {
    assert_eq!(GC_WORKER_SERVICE_SAFE_POINT_ID, "gc_worker");
    assert_eq!(NATIVE_BR_SERVICE_SAFE_POINT_ID, "native_br");
    let a = ClusterPaths::new(1);
    let b = ClusterPaths::new(2);
    assert_ne!(a.gc_safe_point_path(0), b.gc_safe_point_path(0));
    assert_ne!(
        a.gc_safe_point_path(0),
        a.gc_safe_point_path(NULL_KEYSPACE_ID)
    );
    assert_ne!(
        a.gc_safe_point_v2_path(NULL_KEYSPACE_ID),
        a.gc_safe_point_path(NULL_KEYSPACE_ID)
    );
    assert_eq!(a.txn_safe_point_path(0), b.txn_safe_point_path(0));
    assert_eq!(
        a.service_gc_safe_point_prefix(),
        a.gc_barrier_prefix(NULL_KEYSPACE_ID)
    );
    assert_eq!(
        a.service_gc_safe_point_path("nested/id"),
        a.gc_barrier_path(NULL_KEYSPACE_ID, "nested/id")
    );
}
