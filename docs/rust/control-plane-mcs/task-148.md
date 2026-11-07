# Task148: finite dual-mode protocol receipt

Status: complete for the acceptance16 request set and42-row registration handoff. This receipt consumes genuine process and caller-runtime host executions; it does not mark every later global regression complete.

[Raw receipt](task-148.json.gz); [machine-readable42-method index](task-148-methods.json); [acceptance16 narrative](../server-api/acceptance/task-16.md).

The named `mcs_148_` target executes the same request functions on Go CLI, Rust CLI and genuine Rust embedded hosts. MetaStorage uses actual PD/PdNode; the four MCS use their canonical hosts. No fake PD server, alternate domain, private runtime or recursive Cargo launch is used. RM fallback true/false also uses real two-PD process/embedded followers. TLS profiles use actual business RPCs and failed client identities; common authentication precedes dispatch.

Keyspace six-method TCP evidence and admitted09/14/15 are consumed separately, with Diagnostics limited to admitted macOS amd64/CGO=0. Seven unregistered methods and LookupKeyspace retain the exact Go Unimplemented boundary.

The index gives source, unique owner, actual test/raw locator, role, stream applicability, persistent effects and TLS scope for every original stable ID. Historical commands are not counted again as new tests. Final exact command/counts and all failed fixture attempts are in the raw receipt.

| Stable ID | Method | Actual checks |
| --- | --- | --- |
| `wire:e17b42dd6f886f0372095932` | `deadlock.Deadlock/GetWaitForEntries` | `sa_016_descriptor_only_services_remain_unregistered_on_all_listener_roles` |
| `wire:6fb6a00aa58b90dd21a15ba8` | `deadlock.Deadlock/Detect` | `sa_016_descriptor_only_services_remain_unregistered_on_all_listener_roles` |
| `wire:ef8b5be6c5113d55d2215d9b` | `resource_manager.ResourceManager/ListResourceGroups` | `mcs_148_classic_rm_fallback / mcs_148_resource_manager_modes` |
| `wire:3b03fab280d7325da6448677` | `resource_manager.ResourceManager/GetResourceGroup` | `mcs_148_classic_rm_fallback / mcs_148_resource_manager_modes` |
| `wire:939c1d3fb566aa23bde5c591` | `resource_manager.ResourceManager/AddResourceGroup` | `mcs_148_classic_rm_fallback / mcs_148_resource_manager_modes` |
| `wire:e5d9dfcc1a114a11264e9005` | `resource_manager.ResourceManager/ModifyResourceGroup` | `mcs_148_classic_rm_fallback / mcs_148_resource_manager_modes` |
| `wire:315f48e34233558345694e1e` | `resource_manager.ResourceManager/DeleteResourceGroup` | `mcs_148_classic_rm_fallback / mcs_148_resource_manager_modes` |
| `wire:41fd52349110370dc146081a` | `resource_manager.ResourceManager/AcquireTokenBuckets` | `mcs_148_classic_rm_fallback / mcs_148_resource_manager_modes` |
| `wire:6760157a800da9106e2ada99` | `tracepb.TraceRecordPubSub/Subscribe` | `sa_016_descriptor_only_services_remain_unregistered_on_all_listener_roles` |
| `wire:f35d1f2e444550e4b90b62e1` | `keyspacepb.Namespace/CreateNamespace` | `sa_016_descriptor_only_services_remain_unregistered_on_all_listener_roles` |
| `wire:6a0cea2b1388b7b75a8de838` | `keyspacepb.Namespace/LoadNamespace` | `sa_016_descriptor_only_services_remain_unregistered_on_all_listener_roles` |
| `wire:3a168d8e446608339598c85c` | `keyspacepb.Namespace/GetAllNamespaces` | `sa_016_descriptor_only_services_remain_unregistered_on_all_listener_roles` |
| `wire:26898258c7377b580446c567` | `keyspacepb.Namespace/UpdateNamespaceState` | `sa_016_descriptor_only_services_remain_unregistered_on_all_listener_roles` |
| `wire:1396be1dde54de95c636a380` | `keyspacepb.Keyspace/LoadKeyspace` | `sa_016_keyspace_watch_cancel_reopen_and_live_backend_compaction` |
| `wire:e4aa723a4c35186d20e39c06` | `keyspacepb.Keyspace/LookupKeyspace` | `sa_016_keyspace_watch_cancel_reopen_and_live_backend_compaction` |
| `wire:bab45497756ebf705ecbbe3d` | `keyspacepb.Keyspace/LoadKeyspaceByID` | `sa_016_keyspace_watch_cancel_reopen_and_live_backend_compaction` |
| `wire:c9c01aef0ae87c0da31bf16d` | `keyspacepb.Keyspace/WatchKeyspaces` | `sa_016_keyspace_watch_cancel_reopen_and_live_backend_compaction` |
| `wire:ddd38cf64f9369c1c5e32c65` | `keyspacepb.Keyspace/UpdateKeyspaceState` | `sa_016_keyspace_watch_cancel_reopen_and_live_backend_compaction` |
| `wire:da597dfb2c06f6829f427ed3` | `keyspacepb.Keyspace/GetAllKeyspaces` | `sa_016_keyspace_watch_cancel_reopen_and_live_backend_compaction` |
| `wire:a5a03ba0ba4c676c25e3177d` | `diagnosticspb.Diagnostics/search_log` | `sa_014_log_stream / sa_015_server_info` |
| `wire:652c16ee3bcb0a4e873e87ce` | `diagnosticspb.Diagnostics/server_info` | `sa_014_log_stream / sa_015_server_info` |
| `wire:6a6b90a86873c0331a84283e` | `tsopb.TSO/Tso` | `mcs_148_tso_modes` |
| `wire:ce8a3004621529e648a92abc` | `tsopb.TSO/FindGroupByKeyspaceID` | `mcs_148_tso_modes` |
| `wire:e3d43f40ba5eaf5e8a0f9825` | `tsopb.TSO/GetMinTS` | `mcs_148_tso_modes` |
| `wire:699593e8cf2e419ac96a432c` | `schedulingpb.Scheduling/StoreHeartbeat` | `mcs_148_scheduling_split_modes / mcs_148_scheduling_scatter_modes` |
| `wire:d13584abfe3daadf8819e464` | `schedulingpb.Scheduling/RegionHeartbeat` | `mcs_148_scheduling_split_modes / mcs_148_scheduling_scatter_modes` |
| `wire:b8afc494f38f3276acbc8ef0` | `schedulingpb.Scheduling/SplitRegions` | `mcs_148_scheduling_split_modes / mcs_148_scheduling_scatter_modes` |
| `wire:eb4079b3088f75f87c50eb7b` | `schedulingpb.Scheduling/ScatterRegions` | `mcs_148_scheduling_split_modes / mcs_148_scheduling_scatter_modes` |
| `wire:06c11b199fc8271de7383e69` | `schedulingpb.Scheduling/GetOperator` | `mcs_148_scheduling_split_modes / mcs_148_scheduling_scatter_modes` |
| `wire:66a98a89c2c80ac5573cba5b` | `schedulingpb.Scheduling/AskBatchSplit` | `mcs_148_scheduling_split_modes / mcs_148_scheduling_scatter_modes` |
| `wire:24be74d67ce37728d2eaca9f` | `schedulingpb.Scheduling/RegionBuckets` | `mcs_148_scheduling_split_modes / mcs_148_scheduling_scatter_modes` |
| `wire:0883781e75d62818a67168f5` | `meta_storagepb.MetaStorage/Watch` | `mcs_148_metastorage_modes` |
| `wire:b211befe98fd66909e97212f` | `meta_storagepb.MetaStorage/Get` | `mcs_148_metastorage_modes` |
| `wire:0cea8d5001f0baf5449a6d48` | `meta_storagepb.MetaStorage/Put` | `mcs_148_metastorage_modes` |
| `wire:453882faaa41bd44f75abe16` | `meta_storagepb.MetaStorage/Delete` | `mcs_148_metastorage_modes` |
| `wire:97cfa63b5dc6f8c106cb0e92` | `routerpb.Router/GetRegion` | `mcs_148_router_modes` |
| `wire:05048320c392d22119efc567` | `routerpb.Router/GetPrevRegion` | `mcs_148_router_modes` |
| `wire:d524cc7c25febe7c5c67c433` | `routerpb.Router/GetRegionByID` | `mcs_148_router_modes` |
| `wire:3cec09d7dd369a9064f4d33a` | `routerpb.Router/QueryRegion` | `mcs_148_router_modes` |
| `wire:be76ac8ee8c178f2158a3546` | `routerpb.Router/BatchScanRegions` | `mcs_148_router_modes` |
| `wire:66bd9cd4443003641b407cf2` | `routerpb.Router/GetStore` | `mcs_148_router_modes` |
| `wire:737f1ca1a6e7d55493cfe47b` | `routerpb.Router/GetAllStores` | `mcs_148_router_modes` |

Streaming applicability: Tso/QueryRegion/heartbeat/buckets and tokens half-close or cancel their own request; fresh streams use the same canonical owner. MetaStorage and Keyspace Watch are infinite server streams, so no artificial finite EOF is required. Metadata compaction recovery is checked at actual revision-bearing watch consumers; token/heartbeat/query requests have no resume revision to compact. Backend source recovery and frontend stream lifecycle are distinguished in the index.

Persistence: MetaStorage SDK rows and65-68 WAL; RM metadata role/follower/settings revisions and actual grants/balances plus prior138 periodic190000/handoff; TSO timestamp rows and11/13 transitions/WAL; Scheduling full all-key namespace unchanged and real PD allocation/command/ACK; Router observes the genuine PD cache without a second index.

Deferred owners17-24 are listed in the machine-readable index. They include native Rust production durability, complete platform/configuration/topology/catalogue checks, classic process terminal differences and whole-workspace regressions. They are not substitutes for the real domain/stream/state evidence required here.
