# 核心领域公开能力覆盖审计

审计日期：2026-10-23；批次 68，直接依赖批次 67。规划 Go 基准
`0820f0b67b0e49b289b84fdf66b7b8fb178afe82`，审计起点
`82a98ed64eaff8ea6a7a38169bc1b5604a39cde8`。
该范围 Go 生产源码、已有测试及根模块配置相对基准无差异；没有修改或运行注入工具。
[core 契约](core-contract.md)、[storelimit 契约](storelimit-contract.md)、
[response 契约](response-contract.md) 是历史责任清单；本文更新其“尚无 Rust”快照，
不把历史缺失状态或已删除编号任务当作当前实现状态。

## 证据含义

- **源码推导 + Rust 测试**：当前只读 Go 分支/已有断言提供期望，Rust 真行为测试验证。
  这不等于运行 Go/Rust 差分；本文不声称新运行过 Go oracle。
- **历史 oracle**：任务 2–7 的提交曾含 Go oracle，提交 `014342310` 已删除它们。
  当前 fixture 和 Rust 测试仍在，历史执行结果不能由当前文件存在性重新证明。
- **红绿记录**：本次补齐的补齐行为均先实际失败再通过（缺方法 E0599 或角色计数断言失败）；
  任务 64–67 的相邻 `.rs.md` 保留前序红绿记录。其他任务的原始会话命令执行项和完成记录也已只读核对，见文末 64 项运行追溯；
  源码、fixture、当前测试、原始红绿记录分别作为证据，不互相替代。
- 表内 Go 测试为契约中的静态引用，可能仅覆盖同类行为；无直接断言时照实保留。
  不将 token 命中作为接收者解析后的完整调用图。特别是 WitnessScore 的实际方法调用在
  schedulers/balance_witness.go 和 schedulers/utils.go，placement/fit.go 的同名字段不是调用。
- 类型、getter、选项不要求 Go 同形 API；Rust 自有字段、枚举或明确组合表达式可以承载同一行为。
  以下索引同时给出精确符号和模块；没有宣称 Go informer/Runner trait 或 protobuf 已进入 core。

## 本次修复及失败/通过证据

| 原责任 | Go 行为 | 当前 Rust 符号及测试 | 新验证 |
| --- | --- | --- | --- |
| 23 | store.go:308 WitnessScore，整数加 delta 后转浮点 | store_health.rs::witness_score；store_health_test.rs::core_task_68_witness_score_delta_and_wrapping | 缺方法退出 101 → 1 通过；含负值与 i64 溢出 |
| 30 | region.go:703/717，witness 非零 Store ID 的大小/keys 为 0 | region_flow.rs::store_peer_approximate_size/store_peer_approximate_keys；core_task_68_store_peer_statistics | 缺方法退出 101 → 通过；普通、未知、零 ID、clone 隔离 |
| 29 | region_option.go，ID、角色、witness、learner、down/pending、epoch 修改 | RegionOption::{RegionId,PeerIds,Witnesses,Learners,Role,DownPeers,PendingPeers,ConfVer,Version}；core_task_68_clone_option_coverage | 缺选项退出 101 → 通过；顺序、长度不符、epoch 缺省和排序 |
| 30 | region_option.go 的原始统计 setter 不经过 heartbeat 归一化 | RegionInfo::stats_mut；core_task_68_raw_stat_options_preserve_signed_values_and_source | 缺方法退出 101 → 通过；负大小、u64 边界、来源保持和快照隔离 |
| 11/16 | store_option.go 的元数据修改、SetStoreMeta 保留 ID/心跳及未赋值字段 | StoreInfo::edit_meta/set_store_meta；core_task_68_store_metadata_options_detach_and_preserve_identity | 缺方法退出 101 → 通过；浅 clone 分离、字段更新、身份及心跳保留 |
| 46/47 | region.go:1128–1180，组合写入同时更新 root 与 subtree | RegionsInfo::put_region/atomic_check_and_put_region/check_and_put_region；core_task_68_combined_put_updates_roles_but_root_only_does_not | 缺 put_region 后实际断言 0 != 1（101）→ 通过；root-only 仍不更新角色树 |

| 14/16 | store_option.go:SetNewStoreStats 创建不带 HMA 的独立统计容器 | StoreInfo::set_new_store_stats；core_task_68_new_store_stats_detaches_smoothing | 缺方法（101）→ 通过；旧 clone 隔离、连续报告保持 raw available |

| 64/65 | response/region.go:224 单 Region 在转换前检查取消 | marshal_region_info_json_with_cancellation；core_task_68_single_region_cancellation | 缺入口（101）→ 通过；取消/超时、None 输入、无取消 JSON 兼容 |
| 27 | store_option.go:SetNetworkSlowTriggers 原样更新 u64 计数 | StoreInfo::set_network_slow_triggers；core_task_68_store_metadata_options_detach_and_preserve_identity | 缺 setter（101）→ 通过；u64 上界、旧 clone 隔离和新 clone 保留 |

这些是逐个补齐的依赖修复，未引入 Go 测试、运行时、协议依赖或新的 Cargo target。
八项测试使用只读 Go 源码明确推导的期望，不伪装成新 Go oracle 输出。

## 编号任务、当前模块、符号、测试及提交

任务 1/8/62 是契约文档，68 是本审计，69 负责最终主题回归与关闭来源史诗。
下面所有路径、测试注册和生产符号均从当前工作区复核；同一模块可能被后续任务扩展。
测试列为当前以任务编号命名的测试（具体断言仍须阅读测试文件），不是每项 Go API 的独立测试数。

| 任务 | 当前 Rust 源码及公开符号 | 测试文件与有效声明数 | 最近实现提交 |
| --- | --- | --- | --- |
| 2 | [limit_types.rs](../../pkg/core/storelimit/limit_types.rs)：`Type`, `ADD_PEER`, `REMOVE_PEER`, `SEND_SNAPSHOT`, `TRANSFER_LEADER_IN`, `fn`, `fn`, `STORE_LIMIT_TYPE_LEN`, `VERSION_V1`, `VERSION_V2`, `SMALL_REGION_THRESHOLD`, `UNLIMITED`, `REGION_INFLUENCE`, `SMALL_REGION_INFLUENCE`, `TYPE_NAME_VALUE`, `type_from_name` | [limit_types_test.rs](../../pkg/core/storelimit/limit_types_test.rs) (3) | `25ce7bf3c` |
| 3 | [rate_limit.rs](../../pkg/core/storelimit/rate_limit.rs)：`StoreRateLimit`, `new`, `available`, `take`, `rate`, `version`, `ack`, `feedback` | [rate_limit_test.rs](../../pkg/core/storelimit/rate_limit_test.rs) (4) | `7893989e5` |
| 4 | [rate_reset.rs](../../pkg/core/storelimit/rate_reset.rs)：`reset` | [rate_reset_test.rs](../../pkg/core/storelimit/rate_reset_test.rs) (8) | `7893989e5` |
| 5 | [window.rs](../../pkg/core/storelimit/window.rs)：`Window`, `new`, `reset`, `capacity`, `used`, `count`, `available`, `take`, `ack` | [window_test.rs](../../pkg/core/storelimit/window_test.rs) (3) | `85f1142cc` |
| 6 | [sliding_windows.rs](../../pkg/core/storelimit/sliding_windows.rs)：`SlidingWindows`, `new`, `version`, `capacity`, `used`, `available`, `take`, `ack` | [sliding_windows_test.rs](../../pkg/core/storelimit/sliding_windows_test.rs) (1) | `d594a0f13` |
| 7 | [window_feedback.rs](../../pkg/core/storelimit/window_feedback.rs)：`feedback` | [window_feedback_test.rs](../../pkg/core/storelimit/window_feedback_test.rs) (2) | `d594a0f13` |
| 9 | [peer.rs](../../pkg/core/peer.rs)：`PeerRole`, `Peer`, `is_voter`, `is_learner`, `is_witness`, `is_voter_or_incoming_voter`, `is_learner_or_demoting_voter`, `is_in_joint_state`, `count_in_joint_state` | [peer_test.rs](../../pkg/core/peer_test.rs) (4) | `78a8481f6` |
| 10 | [region_meta.rs](../../pkg/core/region_meta.rs)：`RegionId`, `RegionEpoch`, `KeyRange`, `contains`, `RegionEncryptionMeta`, `RegionMeta` | [region_meta_test.rs](../../pkg/core/region_meta_test.rs) (3) | `d28695fde` |
| 11 | [store_meta.rs](../../pkg/core/store_meta.rs)：`StoreMeta`, `StoreInfo`, `new`, `edit_meta`, `set_store_meta`, `meta`, `id`, `address`, `status_address`, `version`, `deploy_path`, `start_time_unix_seconds`, `leader_weight`, `region_weight`, `TxnProtocolVersionRange` | [store_meta_test.rs](../../pkg/core/store_meta_test.rs) (2) | `11238f195` |
| 12 | [store_state.rs](../../pkg/core/store_state.rs)：`StoreState`, `NodeState`, `state`, `node_state`, `is_up`, `is_preparing`, `is_serving`, `is_removing`, `is_removed`, `is_physically_destroyed`, `set_store_state` | [store_state_test.rs](../../pkg/core/store_state_test.rs) (4) | `f7558c366` |
| 13 | [store_labels.rs](../../pkg/core/store_labels.rs)：`ENGINE_KEY`, `ENGINE_ROLE_KEY`, `ENGINE_TIKV`, `ENGINE_TIFLASH`, `ENGINE_TIFLASH_COMPUTE`, `StoreLabel`, `merge_labels`, `is_store_contain_label`, `InvalidStoreEngine`, `validate_store_engine_key`, `labels`, `set_labels`, `label_value`, `is_tikv`, `is_tiflash_write`, `is_tiflash_compute`, `engine` | [store_labels_test.rs](../../pkg/core/store_labels_test.rs) (5) | `a304a4569` |
| 14 | [store_stats.rs](../../pkg/core/store_stats.rs)：`DFSStats`, `StoreStats`, `store_stats`, `update_raw_stats`, `capacity`, `available`, `used_size`, `is_busy`, `sending_snap_count`, `receiving_snap_count`, `bytes_written`, `bytes_read`, `keys_written`, `keys_read`, `TimeInterval`, `RecordPair`, `PeerStat`, `CpuStats`, `DfsStatScope`, `DfsStatItem`, `SlowTrend`, `SnapshotStat`, `KeyspaceIdentity` | [store_stats_test.rs](../../pkg/core/store_stats_test.rs) (2) | `11238f195` |
| 15 | [store_stats_average.rs](../../pkg/core/store_stats_average.rs)：`climp0`, `avg_available` | [store_stats_average_test.rs](../../pkg/core/store_stats_average_test.rs) (4) | `f7558c366` |
| 16 | [store_clone.rs](../../pkg/core/store_clone.rs)：`shallow_clone`, `limiter` | [store_clone_test.rs](../../pkg/core/store_clone_test.rs) (5) | `33cc8bd96` |
| 17 | [store_time.rs](../../pkg/core/store_time.rs)：`last_heartbeat_unix_nanos`, `set_last_heartbeat_unix_nanos`, `set_last_persist_unix_nanos`, `set_last_awaken_unix_nanos`, `down_time_nanos`, `need_persist`, `is_disconnected`, `is_unhealthy`, `uptime_nanos`, `need_awaken_store` | [store_time_test.rs](../../pkg/core/store_time_test.rs) (2) | `906fa9cc5` |
| 18 | [store_control.rs](../../pkg/core/store_control.rs)：`pause_leader_transfer`, `resume_leader_transfer`, `allow_leader_transfer_in`, `allow_leader_transfer_out`, `slow_store_evicted`, `slow_store_recovered`, `evicted_as_slow_store`, `stopping_store_evicted`, `stopping_store_recovered`, `evicted_as_stopping_store`, `slow_trend_evicted`, `slow_trend_recovered`, `is_evicted_as_slow_trend` | [store_control_test.rs](../../pkg/core/store_control_test.rs) (3) | `c059ae898` |
| 19 | [store_limit.rs](../../pkg/core/store_limit.rs)：`StoreLimiter`, `version`, `reset`, `rate`, `feedback`, `available`, `take`, `ack`, `set_store_limit`, `limit_version`, `reset_store_limit`, `feedback`, `is_available` | [store_limit_test.rs](../../pkg/core/store_limit_test.rs) (3) | `d00d91bf2` |
| 20 | [store_resource.rs](../../pkg/core/store_resource.rs)：`leader_count`, `region_count`, `learner_count`, `witness_count`, `pending_peer_count`, `leader_size`, `region_size`, `set_leader_count`, `set_region_count`, `set_learner_count`, `set_witness_count`, `set_pending_peer_count`, `set_leader_size`, `set_region_size`, `set_leader_weight`, `set_region_weight`, `resource_count`, `resource_size`, `resource_weight`, `leader_score` | [store_resource_test.rs](../../pkg/core/store_resource_test.rs) (3) | `433307c8a` |
| 21 | [store_score_v1.rs](../../pkg/core/store_score_v1.rs)：`region_score_v1` | [store_score_v1_test.rs](../../pkg/core/store_score_v1_test.rs) (2) | `79f0143db` |
| 22 | [store_score_v2.rs](../../pkg/core/store_score_v2.rs)：`region_score`, `region_score_v2` | [store_score_v2_test.rs](../../pkg/core/store_score_v2_test.rs) (3) | `130ee3cdb` |
| 23 | [store_health.rs](../../pkg/core/store_health.rs)：`witness_score`, `storage_size`, `available_ratio`, `is_low_space`, `slow_score`, `network_slow_scores`, `is_slow`, `min_resolved_ts`, `set_min_resolved_ts`, `is_available_for_min_resolved_ts` | [store_health_test.rs](../../pkg/core/store_health_test.rs) (3) | `9cc33e4cd` |
| 24 | [store_location.rs](../../pkg/core/store_location.rs)：`compare_location`, `distinct_score` | [store_location_test.rs](../../pkg/core/store_location_test.rs) (3) | `a304a4569` |
| 25 | [store_topology.rs](../../pkg/core/store_topology.rs)：`get_store_topo_weight` | [store_topology_test.rs](../../pkg/core/store_topology_test.rs) (1) | `d7d9f6257` |
| 26 | [stores.rs](../../pkg/core/stores.rs)：`StoresInfo`, `new`, `get_store`, `get_stores`, `get_meta_stores`, `get_store_ids`, `store_count`, `put_store`, `patch_store`, `delete_store`, `reset_stores` | [stores_test.rs](../../pkg/core/stores_test.rs) (3) | `33cc8bd96` |
| 27 | [stores_control.rs](../../pkg/core/stores_control.rs)：`StoreControlError`, `network_slow_triggers`, `pause_leader_transfer`, `resume_leader_transfer`, `slow_store_evicted`, `slow_store_recovered`, `stopping_store_evicted`, `stopping_store_recovered`, `slow_trend_evicted`, `slow_trend_recovered`, `reset_store_limit`, `trigger_network_slow_evict`, `reset_trigger_network_slow_evict`, `avg_network_slow_score` | [stores_control_test.rs](../../pkg/core/stores_control_test.rs) (3) | `33cc8bd96` |
| 28 | [region.rs](../../pkg/core/region.rs)：`DownPeer`, `RegionInfo`, `new`, `meta`, `leader`, `voters`, `learners`, `witnesses`, `down_peers`, `pending_peers`, `get_peer`, `get_store_peer`, `get_store_voter`, `get_store_learner`, `get_store_witness`, `get_down_peer`, `get_down_voter`, `get_down_learner`, `get_pending_peer`, `get_pending_voter`, `get_pending_learner`, `store_ids`, `followers`, `non_witness_voters`, `diff_followers` | [region_test.rs](../../pkg/core/region_test.rs) (3) | `593c15364` |
| 29 | [region_clone.rs](../../pkg/core/region_clone.rs)：`RegionOption`, `clone_with` | [region_clone_test.rs](../../pkg/core/region_clone_test.rs) (3) | `593c15364` |
| 30 | [region_flow.rs](../../pkg/core/region_flow.rs)：`RegionHeartbeatStats`, `RegionStats`, `normalize`, `flow_round_divisor_by_digit`, `round_bytes_read`, `round_bytes_written`, `store_peer_approximate_size`, `store_peer_approximate_keys`, `set_heartbeat_stats`, `stats_mut`, `stats` | [region_flow_test.rs](../../pkg/core/region_flow_test.rs) (3) | `c7cc1abbb` |
| 31 | [region_query.rs](../../pkg/core/region_query.rs)：`QueryStats`, `merge`, `read_query_num`, `write_query_num`, `reset_read`, `reset_write`, `set_query_stats`, `add_query_stats`, `read_query_num`, `write_query_num`, `set_report_interval`, `set_read_cpu_usage`, `write_rate`, `loads`, `write_loads` | [region_query_test.rs](../../pkg/core/region_query_test.rs) (2) | `e905b1f6e` |
| 32 | [region_merge_policy.rs](../../pkg/core/region_merge_policy.rs)：`is_empty_region`, `need_merge`, `is_oversized` | [region_merge_policy_test.rs](../../pkg/core/region_merge_policy_test.rs) (1) | `f902c561d` |
| 33 | [region_inherit.rs](../../pkg/core/region_inherit.rs)：`RegionSource`, `RegionBuckets`, `RegionBucketMeta`, `source`, `set_source`, `loaded_from_storage`, `loaded_from_sync`, `report_buckets`, `set_bucket_meta`, `buckets`, `inherit` | [region_inherit_test.rs](../../pkg/core/region_inherit_test.rs) (3) | `d28695fde` |
| 34 | [region_buckets.rs](../../pkg/core/region_buckets.rs)：`update_buckets`, `compare_and_set_report_buckets` | [region_buckets_test.rs](../../pkg/core/region_buckets_test.rs) (5) | `e498bd682` |
| 35 | [region_guide.rs](../../pkg/core/region_guide.rs)：`RegionGuide`, `set_term`, `term`, `replication_status`, `set_replication_status`, `region_guide`, `has_replication_status`, `set_response_cpu_usage`, `response_cpu_usage` | [region_guide_test.rs](../../pkg/core/region_guide_test.rs) (2) | `593c15364` |
| 36 | [region_format.rs](../../pkg/core/region_format.rs)：`Rule`, `new`, `marshal_json`, `unmarshal_json`, `hex_region_key`, `hex_region_key_str`, `diff_region_key_info`, `diff_region_peers_info`, `HexRegionMeta`, `region_to_hex_meta`, `HexRegionsMeta`, `regions_to_hex_meta` | [region_format_test.rs](../../pkg/core/region_format_test.rs) (4) | `fe5c56441` |
| 37 | [region_split.rs](../../pkg/core/region_split.rs)：`split_regions` | [region_split_test.rs](../../pkg/core/region_split_test.rs) (3) | `788cf7d0a` |
| 38 | [region_merge.rs](../../pkg/core/region_merge.rs)：`merge_regions` | [region_merge_test.rs](../../pkg/core/region_merge_test.rs) (4) | `fd1cec3a2` |
| 39 | [region_tree.rs](../../pkg/core/region_tree.rs)：`RegionItem`, `new`, `region`, `less`, `RegionTree`, `new`, `from_regions`, `search`, `search_prev`, `search_by_keys`, `search_by_prev_keys`, `get_adjacent_regions` | [region_tree_test.rs](../../pkg/core/region_tree_test.rs) (3) | `94d85cf0c` |
| 40 | [region_tree_update.rs](../../pkg/core/region_tree_update.rs)：`overlaps`, `update`, `remove` | [region_tree_update_test.rs](../../pkg/core/region_tree_update_test.rs) (2) | `94d85cf0c` |
| 41 | [region_tree_stats.rs](../../pkg/core/region_tree_stats.rs)：`inc_ref`, `dec_ref`, `get_ref`, `with_count_ref`, `update_ref`, `update_stat`, `total_size`, `total_write_rate`, `not_from_storage_regions_count` | [region_tree_stats_test.rs](../../pkg/core/region_tree_stats_test.rs) (4) | `94d85cf0c` |
| 42 | [region_tree_scan.rs](../../pkg/core/region_tree_scan.rs)：`scan_range`, `scan_ranges`, `get_count_by_range` | [region_tree_scan_test.rs](../../pkg/core/region_tree_scan_test.rs) (4) | `bbf3591a9` |
| 43 | [region_tree_random.rs](../../pkg/core/region_tree_random.rs)：`random_region`, `random_regions` | [region_tree_random_test.rs](../../pkg/core/region_tree_random_test.rs) (5) | `b9a2a038a` |
| 44 | [regions.rs](../../pkg/core/regions.rs)：`RegionsInfo`, `new`, `get_region`, `get_regions`, `tree_len`, `set_region` | [regions_test.rs](../../pkg/core/regions_test.rs) (2) | `d258f91ad` |
| 45 | [region_stale.rs](../../pkg/core/region_stale.rs)：`StaleRegionError`, `RegionValidationError`, `is_region_recreated`, `check_region`, `RegionPreCheck`, `pre_check_put_region`, `valid_region` | [region_stale_test.rs](../../pkg/core/region_stale_test.rs) (3) | `2ef936c5a` |
| 46 | [regions_atomic.rs](../../pkg/core/regions_atomic.rs)：`put_region`, `check_and_put_root_tree`, `atomic_check_and_put_region`, `check_and_put_region` | [regions_atomic_test.rs](../../pkg/core/regions_atomic_test.rs) (3) | `d27e3ba52` |
| 47 | [regions_subtree.rs](../../pkg/core/regions_subtree.rs)：`RegionRole`, `role_tree`, `update_sub_tree` | [regions_subtree_test.rs](../../pkg/core/regions_subtree_test.rs) (4) | `d258f91ad` |
| 48 | [regions_subtree_order.rs](../../pkg/core/regions_subtree_order.rs)：`check_and_put_sub_tree`, `update_sub_tree_order_insensitive` | [regions_subtree_order_test.rs](../../pkg/core/regions_subtree_order_test.rs) (3) | `e23ed661a` |
| 49 | [regions_reset.rs](../../pkg/core/regions_reset.rs)：`remove_region`, `remove_region_from_sub_tree`, `remove_region_if_exist`, `reset_region_cache` | [regions_reset_test.rs](../../pkg/core/regions_reset_test.rs) (3) | `d258f91ad` |
| 50 | [regions_query.rs](../../pkg/core/regions_query.rs)：`RegionQueryResponse`, `RegionQueryResult`, `query_regions` | [regions_query_test.rs](../../pkg/core/regions_query_test.rs) (4) | `88d32cf74` |
| 51 | [regions_scan.rs](../../pkg/core/regions_scan.rs)：`scan_regions`, `scan_region_with_iterator` | [regions_scan_test.rs](../../pkg/core/regions_scan_test.rs) (3) | `02bcb5cd1` |
| 52 | [regions_batch_scan.rs](../../pkg/core/regions_batch_scan.rs)：`BatchScanRegionsOptions`, `RegionNotAdjacent`, `code`, `batch_scan_regions` | [regions_batch_scan_test.rs](../../pkg/core/regions_batch_scan_test.rs) (2) | `dde2f5b66` |
| 53 | [regions_stats.rs](../../pkg/core/regions_stats.rs)：`StoreRegionStats`, `store_role_count`, `store_role_size`, `store_role_write_rate`, `store_region_count`, `store_region_size`, `store_write_rate`, `store_stats`, `total_region_count`, `average_region_size`, `not_from_storage_regions_count`, `not_from_storage_regions_count_by_store`, `is_store_prepared` | [regions_stats_test.rs](../../pkg/core/regions_stats_test.rs) (2) | `447804069` |
| 54 | [regions_range_stats.rs](../../pkg/core/regions_range_stats.rs)：`get_region_size_by_range`, `get_region_count_by_range`, `store_role_count_by_range`, `store_peer_count_by_range`, `get_adjacent_regions`, `get_range_holes` | [regions_range_stats_test.rs](../../pkg/core/regions_range_stats_test.rs) (4) | `dd514448d` |
| 55 | [regions_random.rs](../../pkg/core/regions_random.rs)：`need_transfer_witness_leader`, `rand_leader_regions`, `rand_follower_regions`, `rand_learner_regions`, `rand_witness_regions`, `rand_pending_regions` | [regions_random_test.rs](../../pkg/core/regions_random_test.rs) (4) | `294d66cfc` |
| 56 | [stores_region.rs](../../pkg/core/stores_region.rs)：`get_follower_stores`, `get_region_stores`, `get_leader_store`, `get_non_witness_voter_stores` | [stores_region_test.rs](../../pkg/core/stores_region_test.rs) (2) | `7c7e7cb5a` |
| 57 | [basic_cluster.rs](../../pkg/core/basic_cluster.rs)：`BasicCluster`, `new`, `update_store_status`, `update_all_store_status`, `get_leader_store_by_region_id` | [basic_cluster_test.rs](../../pkg/core/basic_cluster_test.rs) (3) | `245b474b5` |
| 58 | [basic_cluster_rates.rs](../../pkg/core/basic_cluster_rates.rs)：`get_stores_leader_write_rate`, `get_stores_write_rate` | [basic_cluster_rates_test.rs](../../pkg/core/basic_cluster_rates_test.rs) (2) | `83cdfc95d` |
| 59 | [core_trace.rs](../../pkg/server/core_trace.rs)：`Stage`, `ALL`, `label`, `HeartbeatMetrics`, `register`, `duration`, `count`, `HeartbeatTracer`, `new`, `begin`, `reset_checkpoint`, `on_save_cache_begin`, `on_save_cache_finished`, `finish` | [core_trace_test.rs](../../pkg/server/core_trace_test.rs) (3) | `7eca96bbd` |
| 60 | [core_store_adapter.rs](../../pkg/server/core_store_adapter.rs)：`store_from_proto`, `store_to_proto`, `store_stats_from_proto`, `store_stats_to_proto` | [core_store_adapter_test.rs](../../pkg/server/core_store_adapter_test.rs) (5) | `11238f195` |
| 61 | [core_region_adapter.rs](../../pkg/server/core_region_adapter.rs)：`peer_from_proto`, `peer_to_proto`, `region_from_proto`, `region_to_proto`, `region_from_heartbeat` | [core_region_adapter_test.rs](../../pkg/server/core_region_adapter_test.rs) (4) | `593c15364` |
| 63 | [peer.rs](../../pkg/response/peer.rs)：`MetaPeer`, `from_peer`, `PDPeerStats` | [peer_test.rs](../../pkg/response/peer_test.rs) (4) | `9830616e9` |
| 64 | [region.rs](../../pkg/response/region.rs)：`APIRegionEpoch`, `ReplicationStatus`, `RegionInfo`, `new_api_region_info`, `init_region`, `marshal_region_info_json` | [region_test.rs](../../pkg/response/region_test.rs) (4) | `593c15364` |
| 65 | [regions.rs](../../pkg/response/regions.rs)：`RegionListCancellation`, `RegionListError`, `marshal_regions_info_json` | [regions_test.rs](../../pkg/response/regions_test.rs) (3) | `c412da1b0` |
| 66 | [store.rs](../../pkg/response/store.rs)：`StoreResponseConfig`, `MetaStore`, `StoreTimestamp`, `SlowTrend`, `StoreStatus`, `StoreInfo`, `StoresInfo`, `build_store_info` | [store_test.rs](../../pkg/response/store_test.rs) (5) | `d5bf73867` |
| 67 | [store_state.rs](../../pkg/response/store_state.rs)：`build_store_info_at` | [store_state_test.rs](../../pkg/response/store_state_test.rs) (2) | `82a98ed64` |

## 非同形 API、内部机制与范围边界

| Go 清单项 | 当前表达或边界 | 责任 |
| --- | --- | --- |
| StoreCreateOption、SetStoreAddress/StartTime/Version/DeployPath/NodeState | StoreInfo::edit_meta 修改自有 StoreMeta；状态转换仍用 set_store_state；set_store_meta 只更新 Go 明列字段 | 11–16，本次补元数据变更入口 |
| RegionCreateOption、WithWitness/WithWitnesses/WithLearners/WithNewPeerIDs 等 | RegionOption；WithWitness 为 Witnesses(vec![id])；Learners 以传入身份替换、清 witness；PeerIds 长度不符不修改 | 29，本次补选项 |
| Set{Read,Written}{Bytes,Keys}、SetApproximate* | stats_mut 的自有 RegionStats 字段直接赋值，不使用有损 normalize 替代 | 30，本次补入口 |
| GetStat | stats() 的四个 read/written bytes/keys；pdpb.RegionStat 生成消息装配在 server，不泄漏至 core | 30/61 |
| GetCPUUsage/SetCPUUsage | response_cpu_usage/set_response_cpu_usage；unified read CPU 在 set_read_cpu_usage，两个计数不混用 | 31/61/64 |
| GetRegionByKey/GetPrevRegionByKey | query_regions 的 key_ids/prev_key_ids，再 get_region 取得完整快照；底层 RegionTree::search/search_prev | 39/50 |
| GetRelevantRegions | pre_check_put_region 的前两项 origin/overlaps；第三项校验结果不改变快照 | 44/45 |
| GetStoreRegions/ByTypeInSubTree | role_tree(role,store).scan_ranges()；all 拼接 Leader/Follower/Learner 三树，witness 已包含其中；未知字符串应由适配层校验，core 使用 RegionRole | 47/53 |
| GetStore*Count/Size/WriteRate、角色范围查询 | store_role_count/size/write_rate、store_role_count_by_range，角色为显式 RegionRole；跨角色合计使用 store_region_count/size/write_rate | 53/54 |
| GetMetaRegions | get_regions() 每项 meta().clone()；只产生领域 RegionMeta，协议消息由 server 转换 | 44/61 |
| CloneStoreStats | store_stats() 返回自有副本；Clone 与 ShallowClone 的 stats/limiter 共享另由 16 核验 | 14/16 |
| SetBuckets | set_bucket_meta：调用者从版本 > 0 的 buckets 提取 RegionBucketMeta，否则 None；report buckets 更新与 CAS 使用独立 API | 33/34/61 |
| IsFlashbackChanged、SortedPeersEqual/SortedPeersStatsEqual | region_guide 比较 flashback；排序相等的内部比较采用 peer id/store id 投影，不能把包含 role/witness 的派生 Eq 当成 Go identity 比较 | 28/35 |
| GetStoreIDs、GetStoreStats、基础构造器 | StoresInfo::get_store_ids、RegionInfo::store_ids、StoreInfo::store_stats、各类型 ::new；元数据和类型通过自有字段表达 | 9–14/26/28 |
| WithLimit/WithOutputMustContainAllKeyRange | BatchScanRegionsOptions 自有字段；错误为领域错误，未承诺网络状态码映射 | 52 |
| ToUpperASCIIInplace/EncodeToString/HexRegionKey/HexMeta | region_format 的 hex_region_key/hex_region_key_str 及 Display；ASCII 内部辅助不要求相同分配/原地改写函数形状 | 36 |
| RandomKindReadQuery/RandomKindWriteQuery、SetReadQuery/SetWrittenQuery、NewTestRegionInfo | Go 测试/模拟器工具；Rust 测试直接构造 QueryStats/RegionInfo 并调用 reset/merge。未提供全局随机 Go 测试工具同形 API；不属于生产领域行为交付 | 28/31 测试装配，完整 simulator 范围外 |
| Lock/Unlock/RLock/RUnlock、临时 slice Len/Swap/Less | Rust 借用及宿主同步，Vec 排序；不移植 Go 锁接口；不能因此声称任意线程交错自动安全 | 39–49 |
| factory.go 的公开工厂变量 | 值构造、clone 与 server 转换覆盖 Region/Peer/Store 等可观察值语义；对象池机械翻译范围外；CPUStats 等其他主题另验 | 16/29/60/61 |
| MetaProcessContext/ContextTODO、Runner | server/core_trace 的宿主时钟/Registry；Go Runner trait、全局装配和完整心跳服务不迁移 | 59；宿主后续主题 |
| metrics.go 全局 MustRegister、CollectWaitLockMetrics | 阶段指标在 59；树/锁/bucket 全套指标、全局 Registry 装配属于后续统计/运行时主题，不宣称兼容 | 59 及后续主题 |
| response 生成编码器/解码器、Adjust | serde DTO 与公开序列化入口负责可观察 JSON；不移植 easyjson 生成器或测试指针别名修复 | 63–65 |
| response 配置、时间、错误到 HTTP/gRPC | StoreResponseConfig 与显式 now/maxdown；路由、鉴权、HTTP 状态映射在未来宿主接入，本计划没有 Region/Store 网络 API | 64–67；后续网络主题 |

## 三份清单逐项索引

以下保留原清单每行的 Go 符号、用途、代表调用方、已有测试和编号责任；当前 Rust
映射须结合上方任务表和非同形 API 表阅读，不能继续把原清单历史“缺失”解释为现状。
内部方法按其拥有行为测试，不要求迁移对象池/锁/生成器的实现细节。

### core-contract.md

| 依赖源码 | 已存在的 Rust API / 测试源码 | 集成缺口与责任 |
| --- | --- | --- |
| [constant](../../pkg/core/constant/kind.rs) | PriorityLevel、ResourceKind、SchedulePolicy、KeyType、Direction、ScheduleKind；kind_test.rs | 仅枚举能力，9/18/20 使用，非 Peer/Store |
| [rangetree](../../pkg/core/rangetree/lib.rs) | RangeItem、RangeTree::with_factory/update/find/get_overlaps/remove/scan_range/get_adjacent_item/get_at/get_with_index；range_tree_test.rs | Go region_tree.go 使用 btree 而非该通用树；39–43 必须另验引用计数、统计、随机候选与边界 |
| [btree](../../pkg/btree) | 已有 canonical crate 与排序索引实现 | 39–43 不得用通用树存在替代 RegionTree 统计/随机行为证据 |
| [movingaverage](../../pkg/movingaverage/hma.rs) | Hma::with_size/add/get/reset/set/get_instantaneous；hma_test.rs | Go NewHMA(60) 与 Rust 默认构造不同，15 显式选 60 并验证截断/负值；16 核对共享 |
| [ratelimit](../../pkg/ratelimit/rate_limiter.rs) | RateLimiter 的 async 查询与消费；rate_limiter_test.rs | 19 不能假定同步接口；StoreRateLimit 封装与共享仍需装配 |
| [Runner](../../pkg/ratelimit/runner.rs) | ConcurrentRunner/SyncRunner、取消、start/run_task/stop；runner_test.rs | 不等于 Go Runner trait，可用闭包和异步完成语义须在宿主核对；59 不承诺完整移植 |
| [storelimit](../../pkg/core/storelimit/lib.rs) | StoreRateLimit、Window、SlidingWindows、Type，任务 2–7 独立测试 | 尚无统一 Go StoreLimit 同形动态接口；19 负责 v1/v2 统一装配，16/27 保留共享/控制 |
| [proto](../../pkg/proto) | 固定 kvproto 来源、生成类型与 wire fixtures | 仅 60/61 依赖；core 禁止 proto、transport、具体 storage |
| 编号 | 行为与用途 |
| --- | --- |
| 9 | 定义自有 Peer 与 PeerRole，迁移 voter、learner、incoming、demoting、witness 分类和 joint 计数，禁止引用 protobuf。 |
| 10 | 自有 RegionMeta、RegionEpoch、ID 和半开 KeyRange，空 end 表示无穷，保留 u64 边界。 |
| 11 | 自有 StoreMeta、地址、版本、部署路径、启动时间及 StoreInfo 默认权重和零值。 |
| 12 | Up、Preparing、Serving、Removing、Removed 与物理销毁标记的组合和转换。 |
| 13 | 标签合并、重复键、大小写与空值，以及 TiKV、TiFlash write、compute 和引擎校验错误。 |
| 14 | 自有 StoreStats 与 DFSStats 的容量、可用量、忙状态、快照计数和更新。 |
| 15 | GetAvgAvailable、climp0 与连续统计样本的平滑。 |
| 16 | Clone 与 ShallowClone 的元数据隔离、统计及 limiter 共享边界。 |
| 17 | DownTime、NeedPersist、IsDisconnected、IsUnhealthy、uptime 和 NeedAwakenStore 的阈值，显式输入当前时间。 |
| 18 | Pause 与 Resume 的入出方向、slow、stopping、slow-trend 驱逐及恢复。 |
| 19 | StoreInfo 的 v1-v2 limiter 装配、ResetStoreLimit、Feedback 与优先级可用性，使用真实 storelimit 实现。 |
| 20 | leader、region、learner、witness、pending 的计数与 size、weight setter 和 ResourceKind 投影。 |
| 21 | 高空间、过渡、低空间三个区间和权重、delta 边界，数值与 Go oracle 对齐。 |
| 22 | v2 公式与版本路由、零容量、低空间阈值和增减 delta。 |
| 23 | StorageSize、AvailableRatio、IsLowSpace、slow score、network slow score 和 min resolved TS 可用性。 |
| 24 | CompareLocation 与 DistinctScore 的首个差异层、缺失 label 和同位置得分。 |
| 25 | 真实拓扑权重，覆盖部分有效 label、空值、大小写、无合格 Store 及超过 16 层标签，不移植 Go 对象池。 |
| 26 | 新增、覆盖、删除、重置和列表快照，删除 Store 不应用旧 patch。 |
| 27 | 按 Store ID 暂停、恢复、驱逐、limit 重置及缺失 Store 错误。 |
| 28 | 自有 RegionInfo 构造、leader 与 follower、down 与 pending、learner 与 witness 的查询和排序。 |
| 29 | Clone 后键、peer、leader、epoch、flashback 修改不污染原快照。 |
| 30 | 领域 heartbeat 统计归一化、字节到 MiB、IAKV 与 columnar size 和流量舍入。 |
| 31 | 读写 query 合并与重置、GetLoads 与写入速率，零和反向 interval 按 Go 行为归一化。 |
| 32 | IsEmptyRegion、NeedMerge、IsOversized 的 size 和 keys 阈值，含空 Region 的特殊值。 |
| 33 | 来自 Storage、Sync、Heartbeat 的来源语义、流量与 bucket 继承条件，保留真实已观测行为。 |
| 34 | UpdateBuckets、CompareAndSetReportBuckets 与 bucketMeta 新旧版本接受规则，测试竞争更新只能留下有效版本。 |
| 35 | GenerateRegionGuideFunc 的保存元信息、保存缓存、同步和统计变更判定，覆盖逻辑存储量、leader、peer、term、flashback 与 buckets。 |
| 36 | Rule 序列化、键十六进制大写、DiffRegionPeersInfo 与 DiffRegionKeyInfo 和 HexMeta 的可观察字符串。 |
| 37 | SplitRegions 的键边界、epoch 和新 ID 生成行为，按 Go 输入输出核验，生产路径不硬编码测试 fixture。 |
| 38 | MergeRegions 的相邻键区间、peer、epoch 和边界结果，独立于 merge 资格判断。 |
| 39 | RegionItem 排序、半开区间 search、searchPrev、批量 key 查询及相邻项。 |
| 40 | 重叠替换、相同 ID 改范围、split 与 merge 和 remove 的区间不变量。 |
| 41 | updateRef、updateStat、size 与 write rate、来源计数在替换和删除后不重复累计。 |
| 42 | scanRange 提前停止、范围计数、空边界与有洞索引的扫描。 |
| 43 | 连续和不连续范围中的随机候选始终有效，空候选与重试边界明确。 |
| 44 | RegionsInfo 的 ID 表、root tree 和 SetRegion 替换结果一致，GetRegion 与 GetRegions 反映同一快照。 |
| 45 | PreCheckPutRegion、check 与 ValidRegion 对 epoch、term、重建 Region 及 overlap 的接受拒绝和错误语义。 |
| 46 | CheckAndPutRootTree、CheckAndPutRegion 与 AtomicCheckAndPutRegion 在校验和提交间不接受旧版本，返回正确 overlaps。 |
| 47 | leader、follower、learner、witness、pending 子树随 peer 和范围变化移除旧成员并加入新成员。 |
| 48 | CheckAndPutSubTree 和 UpdateSubTreeOrderInsensitive 重放 root 新旧事件仍收敛。 |
| 49 | RemoveRegion、RemoveRegionIfExist、ResetRegionCache 清理 root、ID、subtree 与逻辑 ref。 |
| 50 | QueryRegions 按 keys、prevKeys 和 IDs 返回对应结果，重复与缺失输入、128 分批边界正确。 |
| 51 | ScanRegions、ScanRegionWithIterator 的半开范围与 limit 为零、负数和跨洞行为。 |
| 52 | BatchScanRegions 的 WithLimit 与 OutputMustContainAllKeyRange，范围缺洞报错和多范围总限制。 |
| 53 | 各角色 count、size、write rate、平均 size、非 Storage 来源计数和 IsStorePrepared 的 0.9 门槛由同一组索引推导。 |
| 54 | GetRegionSizeByRange、角色范围 count、GetRangeHoles 与相邻 Region，含无界范围和部分相交。 |
| 55 | RandLeader、Follower、Learner、Witness、Pending 及 NeedTransferWitnessLeader 只返回合格 Region，角色变化后没有旧候选。 |
| 56 | GetFollowerStores、GetRegionStores、GetLeaderStore、GetNonWitnessVoterStores 对未知 Store 和 witness 返回准确结果。 |
| 57 | BasicCluster 组合已验证 StoresInfo 与 RegionsInfo，UpdateStoreStatus 和 UpdateAllStoreStatus 的计数、size 与 leader store 查找一致。 |
| 58 | GetStoresLeaderWriteRate 与 GetStoresWriteRate 的 Store ID、bytes、keys 三数组一一对应，空集和缺失 Store 正确。 |
| 59 | 在适配层以宿主 Registry 和显式时钟记录阶段耗时，每阶段独立且时钟回退不计负值。 |
| 60 | 在 server 转换自有 Store、StoreStats 与 kvproto，覆盖缺省字段、未知枚举和完整字段映射。 |
| 61 | 在 server 转换 Region、Peer、epoch 与 heartbeat stats，保留角色、bucket、replication、query 和 size 单位。 |

**basic_cluster.go**

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `BasicCluster` (23) | 公开；类型/接口 | `server/server.go:187` | `pkg/mcs/scheduling/server/server_test.go:TestGetClusterBeforeStoredDoesNotPanic` | 26/27/50–58 |
| `NewBasicCluster` (29) | 公开；函数/方法 | `server/server.go:531` | `pkg/mcs/router/server/meta/watcher_test.go:TestNewWatcherWaitLoadFailed` | 26/27/50–58 |
| `UpdateStoreStatus` (37, `BasicCluster`) | 公开；函数/方法 | `pkg/core/store.go:1072` | `pkg/schedule/schedulers/scatter_range_test.go:TestScatterRangeBalance` | 26/27/50–58 |
| `GetLeaderStoreByRegionID` (45, `BasicCluster`) | 公开；函数/方法 | `server/grpc_service.go:1187` | `tests/integrations/mcs/scheduling/server_test.go:TestMultipleServerTestSuite` | 26/27/50–58 |
| `getWriteRate` (54, `BasicCluster`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 58 |
| `GetStoresLeaderWriteRate` (75, `BasicCluster`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 58 |
| `GetStoresWriteRate` (80, `BasicCluster`) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:942` | 无直接用例，负责任务补 oracle | 58 |
| `UpdateAllStoreStatus` (85, `BasicCluster`) | 公开；函数/方法 | `tests/testutil.go:282` | `server/cluster/cluster_test.go:TestRemovingProcess` | 26/27/50–58 |
| `RegionSetInformer` (97) | 公开；类型/接口 | `pkg/schedule/core/cluster_informer.go:77` | 无直接用例，负责任务补 oracle | 26/27/50–58 |
| `batchScanRegionsOptions` (117) | 内部；类型/接口 | `pkg/core/region.go:2160` | 无直接用例，负责任务补 oracle | 52 |
| `BatchScanRegionsOptionFunc` (123) | 公开；类型/接口 | `pkg/core/region.go:2155` | 无直接用例，负责任务补 oracle | 52 |
| `WithLimit` (126) | 公开；函数/方法 | `client/opt/option.go:348` | `pkg/election/leadership_test.go:TestExitWatch` | 52 |
| `WithOutputMustContainAllKeyRange` (133) | 公开；函数/方法 | `client/opt/option.go:322` | `tests/integrations/client/client_test.go:TestClientWatchWithRevision` | 52 |
| `StoreSetInformer` (140) | 公开；类型/接口 | `pkg/statistics/store.go:138` | 无直接用例，负责任务补 oracle | 26/27/50–58 |
| `StoreSetController` (152) | 公开；类型/接口 | `pkg/schedule/core/cluster_informer.go:76` | 无直接用例，负责任务补 oracle | 26/27/50–58 |

**context.go**

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `MetaProcessContext` (24) | 公开；类型/接口 | `pkg/core/region.go:882` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `ContextTODO` (35) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestNeedSync` | 59；宿主/后续主题 |

**factory.go**

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |

**metrics.go**

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `init` (129) | 内部；函数/方法 | `pkg/core/store.go:584` | `pkg/mcs/resourcemanager/redirector/metadata_handler_test.go:TestRMMetadataFallbackHandlerReturnsInitError` | 59；宿主/后续主题 |
| `RegionHeartbeatProcessTracer` (146) | 公开；类型/接口 | `pkg/core/context.go:26` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `noopHeartbeatProcessTracer` (175) | 内部；类型/接口 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `NewNoopHeartbeatProcessTracer` (178) | 公开；函数/方法 | `pkg/core/context.go:38` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `Begin` (183, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/keyspace/metrics.go:81` | `tests/integrations/realcluster/ts_test.go:TestTS` | 59；宿主/后续主题 |
| `OnPreCheckFinished` (186, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1348` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnAsyncHotStatsFinished` (189, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1358` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnRegionGuideFinished` (192, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1363` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnSaveCacheBegin` (195, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1396` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnSaveCacheFinished` (198, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1407` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnCheckOverlapsFinished` (201, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/core/region.go:1156` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnValidateRegionFinished` (204, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/core/region.go:1160` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnSetRegionFinished` (207, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/core/region.go:1166` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnUpdateSubTreeFinished` (210, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/core/region.go:1168` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnCollectRegionStatsFinished` (213, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1449` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnAllStageFinished` (216, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster_worker.go:65` | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 59；宿主/后续主题 |
| `Release` (219, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `client/inner_client.go:125` | `pkg/ratelimit/concurrency_limiter_test.go:TestConcurrencyLimiter2` | 59；宿主/后续主题 |
| `regionHeartbeatProcessTracer` (225) | 内部；类型/接口 | 本文件内部/未见外部引用 | `pkg/core/metrics_test.go:TestRegionHeartbeatTracerToleratesNonMonotonicClock` | 59；宿主/后续主题 |
| `NewHeartbeatProcessTracer` (230) | 公开；函数/方法 | `server/cluster/cluster_worker.go:43` | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 59；宿主/后续主题 |
| `Begin` (235, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/keyspace/metrics.go:81` | `tests/integrations/realcluster/ts_test.go:TestTS` | 59；宿主/后续主题 |
| `measure` (251, `regionHeartbeatProcessTracer`) | 内部；函数/方法 | `pkg/election/metrics.go:55` | `pkg/core/metrics_test.go:TestRegionHeartbeatTracerRecordsEachStageIndependently` | 59；宿主/后续主题 |
| `resetCheckpoint` (264, `regionHeartbeatProcessTracer`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnPreCheckFinished` (269, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1348` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnAsyncHotStatsFinished` (274, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1358` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnRegionGuideFinished` (279, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1363` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnSaveCacheBegin` (284, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1396` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnSaveCacheFinished` (289, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1407` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnCheckOverlapsFinished` (294, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/core/region.go:1156` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnValidateRegionFinished` (299, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/core/region.go:1160` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnSetRegionFinished` (304, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/core/region.go:1166` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnUpdateSubTreeFinished` (309, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/core/region.go:1168` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnCollectRegionStatsFinished` (314, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1449` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnAllStageFinished` (319, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster_worker.go:65` | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 59；宿主/后续主题 |
| `Release` (325, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `client/inner_client.go:125` | `pkg/ratelimit/concurrency_limiter_test.go:TestConcurrencyLimiter2` | 59；宿主/后续主题 |

**peer.go**

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `IsLearner` (22) | 公开；函数/方法 | `pkg/core/region.go:161` | `pkg/response/region_test.go:TestEasyJSONCompatibility` | 9 |
| `IsWitness` (27) | 公开；函数/方法 | `pkg/core/region.go:166` | `pkg/response/region_test.go:TestEasyJSONCompatibility` | 9 |
| `IsVoter` (32) | 公开；函数/方法 | `pkg/schedule/checker/rule_checker.go:162` | 无直接用例，负责任务补 oracle | 9 |
| `IsVoterOrIncomingVoter` (38) | 公开；函数/方法 | `pkg/schedule/operator/step.go:906` | 无直接用例，负责任务补 oracle | 9 |
| `IsLearnerOrDemotingVoter` (48) | 公开；函数/方法 | `pkg/schedule/operator/step.go:914` | 无直接用例，负责任务补 oracle | 9 |
| `IsInJointState` (57) | 公开；函数/方法 | `pkg/schedule/operator/step.go:1066` | 无直接用例，负责任务补 oracle | 9 |
| `CountInJointState` (69) | 公开；函数/方法 | `pkg/schedule/operator/step.go:980` | 无直接用例，负责任务补 oracle | 9 |

**region.go**

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `errRegionIsStale` (60) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 45 |
| `RegionInfo` (67) | 公开；类型/接口 | `pkg/core/store.go:820` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestMain` | 28/49/53 |
| `RegionSource` (105) | 公开；类型/接口 | `pkg/core/region_option.go:424` | 无直接用例，负责任务补 oracle | 28/49/53 |
| `LoadedFromStorage` (117, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_tree.go:180` | `tests/server/cluster/cluster_test.go:TestRegionStatistics` | 28/49/53 |
| `LoadedFromSync` (123, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `tests/server/cluster/cluster_test.go:TestRegionStatistics` | 28/49/53 |
| `IncRef` (128, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_tree.go:168` | 无直接用例，负责任务补 oracle | 28/49/53 |
| `DecRef` (133, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_tree.go:167` | 无直接用例，负责任务补 oracle | 28/49/53 |
| `GetRef` (138, `RegionInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1381` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 28/49/53 |
| `NewRegionInfo` (143) | 公开；函数/方法 | `tests/testutil.go:304` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRecordsSplitScatterInSchedulingService` | 28 |
| `classifyVoterAndLearner` (156) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 28 |
| `peersEqualTo` (177, `RegionInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestShouldRemoveFromSubTree` | 29/35 |
| `rangeEqualsTo` (186, `RegionInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestCntRefAfterFlowOnlyUpdate` | 29/35 |
| `IsEmptyRegion` (209) | 公开；函数/方法 | `pkg/statistics/region.go:103` | `pkg/core/region_test.go:TestIsEmptyRegion` | 28/61 |
| `RegionHeartbeatResponse` (214) | 公开；类型/接口 | `pkg/core/factory.go:29` | `pkg/mcs/scheduling/server/server_test.go:TestClusterResourceGettersHandleNilReceiver` | 28/61 |
| `RegionHeartbeatRequest` (220) | 公开；类型/接口 | `tools/utils/meta.go:129` | `pkg/mcs/scheduling/server/server_test.go:TestRegionHeartbeatReturnsNotBootstrappedWhenHeartbeatStreamsMissing` | 28/61 |
| `RegionFromHeartbeat` (238) | 公开；函数/方法 | `server/grpc_service.go:1393` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 30/61 |
| `Inherit` (295, `RegionInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1353` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 33 |
| `Clone` (323, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store.go:121` | `pkg/mcs/resourcemanager/metadataapi/config_service_test.go:TestConfigServiceMetadataWriteDisabledReturns403` | 29 |
| `NeedMerge` (365, `RegionInfo`) | 公开；函数/方法 | `pkg/statistics/region_collection.go:181` | `pkg/core/region_test.go:TestNeedMerge` | 32 |
| `IsOversized` (370, `RegionInfo`) | 公开；函数/方法 | `pkg/statistics/region_collection.go:162` | 无直接用例，负责任务补 oracle | 32 |
| `GetTerm` (375, `RegionInfo`) | 公开；函数/方法 | `server/grpc_service.go:1488` | `tests/integrations/mcs/scheduling/server_test.go:TestServerTestSuite` | 28 |
| `GetLearners` (380, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/filter/healthy.go:57` | `pkg/core/region_test.go:TestShouldRemoveFromSubTree` | 28 |
| `GetVoters` (385, `RegionInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:317` | `pkg/core/region_test.go:TestSortedEqual` | 28 |
| `GetWitnesses` (390, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/checker/rule_checker.go:628` | `pkg/core/region_test.go:TestShouldRemoveFromSubTree` | 28 |
| `GetPeer` (395, `RegionInfo`) | 公开；函数/方法 | `server/handler.go:441` | `pkg/core/region_tree_test.go:TestRegionInfo` | 28 |
| `Rule` (405) | 公开；类型/接口 | `tests/testutil.go:463` | `pkg/mcs/scheduling/server/rule/watcher_test.go:TestRuleWatcherReconcilesInitialLoadRetry` | 36 |
| `String` (419, `Rule`) | 公开；函数/方法 | `pkg/core/store.go:952` | `pkg/mcs/scheduling/server/config/config_test.go:TestPersistConfigDefaultStoreLimit` | 36 |
| `NewRule` (433) | 公开；函数/方法 | `pkg/schedule/schedulers/init.go:594` | `pkg/core/region_test.go:TestCodecRule` | 36 |
| `MarshalJSON` (447, `Rule`) | 公开；函数/方法 | `client/http/types.go:435` | `pkg/response/region_test.go:TestEasyJSONCompatibility` | 36 |
| `UnmarshalJSON` (452, `Rule`) | 公开；函数/方法 | `client/http/types.go:461` | `pkg/response/region_test.go:TestEasyJSONCompatibility` | 36 |
| `GetDownPeer` (468, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/placement/fit.go:459` | `pkg/core/region_tree_test.go:TestRegionInfo` | 28 |
| `GetDownVoter` (478, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 28 |
| `GetDownLearner` (488, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 28 |
| `GetPendingPeer` (498, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/placement/fit.go:461` | `pkg/core/region_tree_test.go:TestRegionInfo` | 28 |
| `GetPendingVoter` (508, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/operator/step.go:230` | 无直接用例，负责任务补 oracle | 28 |
| `GetPendingLearner` (518, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/operator/step.go:534` | 无直接用例，负责任务补 oracle | 28 |
| `GetStorePeer` (528, `RegionInfo`) | 公开；函数/方法 | `server/handler.go:439` | `pkg/statistics/hot_peer_cache_test.go:TestCheckPeerFlowWithMoreExplicitPeersThanRegionPeers` | 28 |
| `GetStoreVoter` (538, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/operator/step.go:208` | 无直接用例，负责任务补 oracle | 28 |
| `GetStoreLearner` (548, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/operator/step.go:526` | 无直接用例，负责任务补 oracle | 28 |
| `GetStoreWitness` (558, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/filter/region_filters.go:172` | 无直接用例，负责任务补 oracle | 28 |
| `GetStoreIDs` (568, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store.go:807` | `pkg/core/region_tree_test.go:TestRegionInfo` | 28 |
| `GetFollowers` (578, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store.go:824` | `pkg/core/region_test.go:TestShouldRemoveFromSubTree` | 28 |
| `GetNonWitnessVoters` (590, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store.go:867` | 无直接用例，负责任务补 oracle | 28 |
| `GetDiffFollowers` (603, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 28 |
| `GetID` (621, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store.go:378` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRejectsAffinityAutoSplit` | 30/61 |
| `GetMeta` (626, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store.go:235` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRejectsAffinityAutoSplit` | 30/61 |
| `GetStat` (634, `RegionInfo`) | 公开；函数/方法 | `pkg/syncer/server.go:453` | `tests/server/region_syncer/region_syncer_test.go:TestRegionSyncer` | 30/61 |
| `UpdateBuckets` (647, `RegionInfo`) | 公开；函数/方法 | `tests/testutil.go:719` | `pkg/core/region_test.go:TestInherit` | 34 |
| `GetReportBuckets` (662, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestInherit` | 34 |
| `CompareAndSetReportBuckets` (667, `RegionInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1332` | 无直接用例，负责任务补 oracle | 34 |
| `GetBuckets` (687, `RegionInfo`) | 公开；函数/方法 | `tests/testutil.go:719` | `pkg/core/region_test.go:TestInherit` | 34 |
| `GetStorePeerApproximateSize` (703, `RegionInfo`) | 公开；函数/方法 | `pkg/statistics/region.go:146` | 无直接用例，负责任务补 oracle | 30 |
| `GetApproximateSize` (712, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_tree.go:254` | `tests/integrations/mcs/scheduling/server_test.go:TestServerTestSuite` | 30 |
| `GetStorePeerApproximateKeys` (717, `RegionInfo`) | 公开；函数/方法 | `pkg/statistics/region.go:147` | 无直接用例，负责任务补 oracle | 30 |
| `GetApproximateKvSize` (726, `RegionInfo`) | 公开；函数/方法 | `pkg/response/region.go:182` | `pkg/core/region_test.go:TestRegionFromHeartbeatIAKVSize` | 30 |
| `GetApproximateIAKvSize` (731, `RegionInfo`) | 公开；函数/方法 | `pkg/statistics/region.go:101` | `pkg/core/region_test.go:TestRegionFromHeartbeatIAKVSize` | 30 |
| `GetApproximateColumnarKvSize` (736, `RegionInfo`) | 公开；函数/方法 | `pkg/response/region.go:183` | 无直接用例，负责任务补 oracle | 30 |
| `GetApproximateKeys` (741, `RegionInfo`) | 公开；函数/方法 | `server/api/region.go:525` | `tests/integrations/mcs/scheduling/server_test.go:TestServerTestSuite` | 30 |
| `GetInterval` (746, `RegionInfo`) | 公开；函数/方法 | `server/handler.go:436` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 30 |
| `GetDownPeers` (751, `RegionInfo`) | 公开；函数/方法 | `pkg/response/region.go:173` | `pkg/core/region_test.go:TestSortedEqual` | 30 |
| `GetPendingPeers` (756, `RegionInfo`) | 公开；函数/方法 | `pkg/response/region.go:174` | `pkg/core/region_test.go:TestSortedEqual` | 30 |
| `GetCPUUsage` (763, `RegionInfo`) | 公开；函数/方法 | `server/api/region.go:744` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 30 |
| `GetReadCPUUsage` (768, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/scatter/region_scatterer.go:904` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 30 |
| `GetBytesRead` (776, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store_stats.go:99` | `pkg/statistics/hot_peer_cache_test.go:TestCacheInherit` | 30 |
| `GetRoundBytesRead` (781, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestRegionRoundingFlow` | 30 |
| `GetBytesWritten` (789, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store_stats.go:92` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 30 |
| `GetRoundBytesWritten` (794, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 30 |
| `GetKeysWritten` (802, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store_stats.go:106` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 30 |
| `GetKeysRead` (807, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store_stats.go:113` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 30 |
| `GetWriteRate` (812, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_tree.go:177` | `pkg/core/region_test.go:TestRegionWriteRate` | 30 |
| `GetLeader` (822, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store.go:849` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 28/10/30 |
| `GetStartKey` (827, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_tree.go:36` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestCreateEmptyRegionTailUsesTiKVStore` | 28/10/30 |
| `GetFlowRoundDivisor` (832, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `tests/integrations/client/client_test.go:TestClientStatelessTestSuite` | 28/10/30 |
| `GetEndKey` (837, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_tree.go:46` | `pkg/replication/replication_mode_test.go:TestRecoverProgressWithSplitAndMerge` | 28/10/30 |
| `GetPeers` (842, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_option.go:63` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestCreateEmptyRegionTailUsesTiKVStore` | 28/10/30 |
| `GetRegionEpoch` (847, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_option.go:144` | `pkg/storage/storage_test.go:TestTrySwitchRegionStorage` | 28/10/30 |
| `GetReplicationStatus` (852, `RegionInfo`) | 公开；函数/方法 | `server/server.go:869` | `pkg/replication/replication_mode_test.go:TestInitial` | 28/10/30 |
| `IsFlashbackChanged` (857, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 28/10/30 |
| `isInvolved` (861, `RegionInfo`) | 内部；函数/方法 | `pkg/core/region_tree.go:459` | 无直接用例，负责任务补 oracle | 10/45 |
| `isRegionRecreated` (865, `RegionInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 10/45 |
| `contain` (875, `RegionInfo`) | 内部；函数/方法 | `pkg/core/region_tree.go:322` | `pkg/mcs/discovery/register_test.go:TestRegister` | 10/45 |
| `RegionGuideFunc` (882) | 公开；类型/接口 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 35 |
| `GenerateRegionGuideFunc` (886) | 公开；函数/方法 | `pkg/syncer/client.go:91` | `pkg/core/region_test.go:TestNeedSync` | 35 |
| `RWLockStats` (1030) | 公开；类型/接口 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 59；宿主锁指标 |
| `Lock` (1039, `RWLockStats`) | 公开；函数/方法 | `pkg/core/store.go:910` | `pkg/syncer/server_test.go:TestSyncFullRegionsBuffersLiveRecords` | 59；宿主锁指标 |
| `Unlock` (1048, `RWLockStats`) | 公开；函数/方法 | `pkg/core/store.go:911` | `pkg/syncer/server_test.go:TestSyncFullRegionsBuffersLiveRecords` | 59；宿主锁指标 |
| `RLock` (1053, `RWLockStats`) | 公开；函数/方法 | `pkg/core/store.go:156` | `pkg/syncer/server_test.go:TestBindStreamWaitsForAppendHistoryRecordsBoundary` | 59；宿主锁指标 |
| `RUnlock` (1062, `RWLockStats`) | 公开；函数/方法 | `pkg/core/store.go:157` | `pkg/syncer/server_test.go:TestBindStreamWaitsForAppendHistoryRecordsBoundary` | 59；宿主锁指标 |
| `RegionsInfo` (1067) | 公开；类型/接口 | `pkg/core/basic_cluster.go:25` | `pkg/response/region_test.go:TestEasyJSONCompatibility` | 44 |
| `NewRegionsInfo` (1083) | 公开；函数/方法 | `pkg/core/basic_cluster.go:32` | `pkg/core/region_test.go:TestSetRegionConcurrence` | 44 |
| `GetRegion` (1098, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:46` | `pkg/syncer/server_test.go:TestClientFinishesFullSyncWithOldLeaderKeepAlive` | 44 |
| `getRegionLocked` (1105, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 44 |
| `CheckAndPutRegion` (1113, `RegionsInfo`) | 公开；函数/方法 | `pkg/syncer/client.go:205` | `pkg/storage/storage_test.go:TestLoadRegions` | 45/46 |
| `PutRegion` (1134, `RegionsInfo`) | 公开；函数/方法 | `pkg/syncer/client.go:162` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRejectsAffinityAutoSplit` | 45/46 |
| `PreCheckPutRegion` (1141, `RegionsInfo`) | 公开；函数/方法 | `pkg/syncer/client.go:150` | 无直接用例，负责任务补 oracle | 45/46 |
| `AtomicCheckAndPutRegion` (1148, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestSetRegionConcurrence` | 46 |
| `CheckAndPutRootTree` (1174, `RegionsInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1406` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 46 |
| `CheckAndPutSubTree` (1198, `RegionsInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1386` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 48 |
| `UpdateSubTreeOrderInsensitive` (1216, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestGetRegionSizeByRange` | 48 |
| `preUpdateSubTreeLocked` (1234, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 47/48 |
| `updateSubTreeLocked` (1275, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 47/48 |
| `getOverlapRegionFromOverlapTreeLocked` (1322, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 47/48 |
| `GetRelevantRegions` (1327, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 44 |
| `check` (1337) | 内部；函数/方法 | `pkg/core/store.go:577` | `pkg/mcs/utils/expected_primary_test.go:TestIsSamePrimary` | 45 |
| `SetRegion` (1361, `RegionsInfo`) | 公开；函数/方法 | `tools/pd-simulator/simulator/raft.go:72` | `pkg/core/region_test.go:TestSetRegion` | 44/46 |
| `setRegionLocked` (1367, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 44/46 |
| `UpdateSubTree` (1418, `RegionsInfo`) | 公开；函数/方法 | `pkg/ratelimit/runner.go:34` | `pkg/core/region_test.go:TestSetRegionConcurrence` | 47/48 |
| `updateSubTreeStat` (1434, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 47/48 |
| `TreeLen` (1460, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 44 |
| `GetOverlaps` (1467, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/region_tree.go:136` | `pkg/core/rangetree/range_tree_test.go:TestRingPutItem` | 44 |
| `RemoveRegion` (1474, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/schedule/schedulers/hot_region_test.go:TestHotReadPeerScheduleWithPlacementScope` | 49 |
| `ResetRegionCache` (1483, `RegionsInfo`) | 公开；函数/方法 | `server/server.go:1001` | `pkg/core/region_test.go:TestCntRefAfterResetRegionCache` | 49 |
| `RemoveRegionFromSubTree` (1500, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/schedule/schedulers/hot_region_test.go:TestHotReadPeerScheduleWithPlacementScope` | 49 |
| `removeRegionFromSubTreeLocked` (1508, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 49 |
| `RemoveRegionIfExist` (1522, `RegionsInfo`) | 公开；函数/方法 | `server/server.go:1040` | `server/cluster/cluster_test.go:TestRemovingProcess` | 49 |
| `peerSlice` (1529) | 内部；类型/接口 | `pkg/core/region_option.go:56` | 无直接用例，负责任务补 oracle | 28/35 |
| `Len` (1531, `peerSlice`) | 公开；函数/方法 | `pkg/core/region_tree.go:115` | `pkg/mcs/utils/expected_primary_test.go:TestDeleteExpectedPrimaryFlagRevokesLease` | 28/35 |
| `Swap` (1534, `peerSlice`) | 公开；函数/方法 | `pkg/keyspace/util.go:258` | `pkg/gc/gc_state_manager_test.go:TestGCStateManager` | 28/35 |
| `Less` (1537, `peerSlice`) | 公开；函数/方法 | `pkg/core/region_tree.go:51` | `pkg/mcs/discovery/register_test.go:TestRegisterRejectsUnleasedExistingKey` | 28/35 |
| `SortedPeersEqual` (1542) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestSortedEqual` | 28/35 |
| `peerStatsSlice` (1555) | 内部；类型/接口 | `pkg/core/region_option.go:35` | 无直接用例，负责任务补 oracle | 28/35 |
| `Len` (1557, `peerStatsSlice`) | 公开；函数/方法 | `pkg/core/region_tree.go:115` | `pkg/mcs/utils/expected_primary_test.go:TestDeleteExpectedPrimaryFlagRevokesLease` | 28/35 |
| `Swap` (1560, `peerStatsSlice`) | 公开；函数/方法 | `pkg/keyspace/util.go:258` | `pkg/gc/gc_state_manager_test.go:TestGCStateManager` | 28/35 |
| `Less` (1563, `peerStatsSlice`) | 公开；函数/方法 | `pkg/core/region_tree.go:51` | `pkg/mcs/discovery/register_test.go:TestRegisterRejectsUnleasedExistingKey` | 28/35 |
| `SortedPeersStatsEqual` (1568) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestSortedEqual` | 28/35 |
| `GetRegionByKey` (1583, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:109` | `tools/pd-simulator/simulator/simutil/key_test.go:TestRegionSplitKey` | 50 |
| `GetPrevRegionByKey` (1590, `RegionsInfo`) | 公开；函数/方法 | `pkg/utils/grpcutil/cluster.go:69` | 无直接用例，负责任务补 oracle | 50 |
| `GetRegions` (1597, `RegionsInfo`) | 公开；函数/方法 | `tests/cluster.go:422` | `pkg/syncer/server_test.go:TestSyncHistoryRecordsSplitBatches` | 50 |
| `GetStoreRegions` (1608, `RegionsInfo`) | 公开；函数/方法 | `tests/cluster.go:451` | `tools/pd-ctl/tests/region/region_test.go:TestRegionTestSuite` | 50 |
| `QueryRegions` (1627, `RegionsInfo`) | 公开；函数/方法 | `pkg/utils/grpcutil/cluster.go:210` | `pkg/core/region_test.go:TestQueryRegions` | 50 |
| `getRegionsByKeys` (1695, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 50 |
| `getRegionsByPrevKeys` (1708, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 50 |
| `getRegionsByIDs` (1720, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 50 |
| `sortOutKeyIDMap` (1734) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 50 |
| `SubTreeRegionType` (1765) | 公开；类型/接口 | `server/api/region.go:268` | `tools/pd-ctl/tests/region/region_test.go:TestRegionTestSuite` | 56 |
| `GetStoreRegionsByTypeInSubTree` (1783, `RegionsInfo`) | 公开；函数/方法 | `server/api/region.go:268` | `tools/pd-ctl/tests/region/region_test.go:TestRegionTestSuite` | 56 |
| `GetStoreLeaderRegionSize` (1819, `RegionsInfo`) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:767` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 53/58 |
| `GetStoreFollowerRegionSize` (1826, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 53/58 |
| `GetStoreLearnerRegionSize` (1833, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 53/58 |
| `GetStoreRegionSize` (1840, `RegionsInfo`) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:768` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 53/58 |
| `getStoreRegionSizeLocked` (1847, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 53/58 |
| `GetStoreLeaderWriteRate` (1852, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:76` | 无直接用例，负责任务补 oracle | 53/58 |
| `GetStoreWriteRate` (1859, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:81` | 无直接用例，负责任务补 oracle | 53/58 |
| `GetNotFromStorageRegionsCnt` (1875, `RegionsInfo`) | 公开；函数/方法 | `pkg/schedule/prepare_checker.go:53` | `server/cluster/cluster_test.go:TestShouldRun` | 53 |
| `GetNotFromStorageRegionsCntByStore` (1882, `RegionsInfo`) | 公开；函数/方法 | `pkg/schedule/prepare_checker.go:68` | 无直接用例，负责任务补 oracle | 53 |
| `IsStorePrepared` (1890, `RegionsInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:2027` | 无直接用例，负责任务补 oracle | 53 |
| `getNotFromStorageRegionsCntByStoreLocked` (1895, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 53 |
| `GetMetaRegions` (1900, `RegionsInfo`) | 公开；函数/方法 | `server/server.go:1761` | `pkg/storage/storage_test.go:TestLoadRegions` | 53/56 |
| `GetStoreStats` (1911, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/store.go:580` | `tests/integrations/mcs/scheduling/server_test.go:TestServerTestSuite` | 53/56 |
| `GetTotalRegionCount` (1919, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:98` | `pkg/storage/storage_test.go:TestLoadRegions` | 53/56 |
| `GetStoreRegionCount` (1926, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:105` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 53/56 |
| `getStoreRegionCountLocked` (1933, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 53/56 |
| `GetStorePendingPeerCount` (1938, `RegionsInfo`) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:766` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 53/56 |
| `GetStoreLeaderCount` (1945, `RegionsInfo`) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:764` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 53/56 |
| `GetStoreFollowerCount` (1952, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `server/cluster/cluster_test.go:TestUpdateAndDeleteLabel` | 53/56 |
| `GetStoreLearnerCount` (1959, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 53/56 |
| `GetStoreWitnessCount` (1966, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `server/cluster/cluster_test.go:TestUpdateAndDeleteLabel` | 53/56 |
| `RandPendingRegions` (1973, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:103` | 无直接用例，负责任务补 oracle | 55 |
| `randLeaderRegion` (1980, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestShouldRemoveFromSubTree` | 55 |
| `RandLeaderRegions` (1987, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:100` | `pkg/core/region_test.go:TestShouldRemoveFromSubTree` | 55 |
| `RandFollowerRegions` (1994, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:99` | 无直接用例，负责任务补 oracle | 55 |
| `RandLearnerRegions` (2001, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:101` | 无直接用例，负责任务补 oracle | 55 |
| `RandWitnessRegions` (2008, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:102` | 无直接用例，负责任务补 oracle | 55 |
| `GetLeader` (2015, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/store.go:849` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 56 |
| `GetFollower` (2025, `RegionsInfo`) | 公开；函数/方法 | `tests/cluster.go:872` | `tests/integrations/mcs/resourcemanager/resource_manager_test.go:TestSwitchModeDuringWorkload` | 56 |
| `GetReadQueryNum` (2035, `RegionInfo`) | 公开；函数/方法 | `server/api/region.go:669` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 31 |
| `GetWriteQueryNum` (2040, `RegionInfo`) | 公开；函数/方法 | `server/api/region.go:641` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 31 |
| `GetReadQueryNum` (2045) | 公开；函数/方法 | `server/api/region.go:669` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 31 |
| `GetWriteQueryNum` (2053) | 公开；函数/方法 | `server/api/region.go:641` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 31 |
| `GetLoads` (2062, `RegionInfo`) | 公开；函数/方法 | `pkg/statistics/hot_peer.go:138` | `pkg/statistics/hot_peer_cache_test.go:TestCheckPeerFlowWithMoreExplicitPeersThanRegionPeers` | 31 |
| `GetWriteLoads` (2076, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 31 |
| `GetStoreLeaderCountByRange` (2090, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:112` | `pkg/core/region_tree_test.go:TestStoreRegionCount` | 54 |
| `GetStorePeerCountByRange` (2100, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:114` | `pkg/core/region_tree_test.go:TestStoreRegionCount` | 54 |
| `GetStoreLearnerCountByRange` (2117, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:113` | `pkg/core/region_tree_test.go:TestStoreRegionCount` | 54 |
| `GetRegionCount` (2127, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/store.go:388` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 54 |
| `ScanRegions` (2135, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:108` | `pkg/utils/grpcutil/cluster_test.go:TestFollowerForwardAndHandleTestSuite` | 51 |
| `BatchScanRegions` (2155, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:110` | `tests/integrations/mcs/router/server_test.go:TestServerTestSuite` | 52 |
| `scanRegion` (2185) | 内部；函数/方法 | `tools/pd-ctl/pdctl/command/region_command.go:140` | `pkg/core/region_test.go:TestScanRegion` | 52 |
| `ScanRegionWithIterator` (2243, `RegionsInfo`) | 公开；函数/方法 | `tools/pd-simulator/simulator/cases/import_data.go:102` | 无直接用例，负责任务补 oracle | 54 |
| `GetRegionSizeByRange` (2250, `RegionsInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:2071` | `pkg/core/region_test.go:TestGetRegionSizeByRange` | 54 |
| `CollectWaitLockMetrics` (2288, `RegionsInfo`) | 公开；函数/方法 | `pkg/mcs/scheduling/server/cluster.go:801` | 无直接用例，负责任务补 oracle | 59；后续锁指标 |
| `GetAdjacentRegions` (2321, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:107` | `tools/pd-ctl/tests/region/region_test.go:TestRegionTestSuite` | 39/50 |
| `GetRangeHoles` (2336, `RegionsInfo`) | 公开；函数/方法 | `tests/cluster.go:444` | 无直接用例，负责任务补 oracle | 54/53 |
| `GetAverageRegionSize` (2362, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:104` | `pkg/schedule/schedulers/balance_region_test.go:TestSingleRegionOnLargeEmptyDiskDoesNotMigrate` | 54/53 |
| `ValidRegion` (2372, `RegionsInfo`) | 公开；函数/方法 | `server/cluster/cluster_worker.go:86` | `tests/server/cluster/cluster_work_test.go:TestRaftClusterTestSuite` | 45 |
| `DiffRegionPeersInfo` (2389) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_tree_test.go:TestRegionInfo` | 36 |
| `DiffRegionKeyInfo` (2419) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestRegionKey` | 36 |
| `ToUpperASCIIInplace` (2436) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 36 |
| `EncodeToString` (2458) | 公开；函数/方法 | `pkg/keyspace/util.go:153` | `pkg/keyspace/util_test.go:TestMakeLabelRule` | 36 |
| `HexRegionKey` (2466) | 公开；函数/方法 | `pkg/core/region_tree.go:439` | `server/api/region_test.go:TestRegionsInfoMarshal` | 36 |
| `HexRegionKeyStr` (2472) | 公开；函数/方法 | `pkg/response/region.go:168` | `pkg/storage/hot_region_storage_test.go:TestHotRegionWrite` | 36 |
| `RegionToHexMeta` (2478) | 公开；函数/方法 | `pkg/core/region_tree.go:201` | `pkg/core/region_test.go:TestRegionKey` | 36 |
| `HexRegionMeta` (2486) | 公开；类型/接口 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 36 |
| `String` (2490, `HexRegionMeta`) | 公开；函数/方法 | `pkg/core/store.go:952` | `pkg/mcs/scheduling/server/config/config_test.go:TestPersistConfigDefaultStoreLimit` | 36 |
| `RegionsToHexMeta` (2499) | 公开；函数/方法 | `server/cluster/cluster_worker.go:259` | 无直接用例，负责任务补 oracle | 36 |
| `HexRegionsMeta` (2507) | 公开；类型/接口 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 36 |
| `String` (2509, `HexRegionsMeta`) | 公开；函数/方法 | `pkg/core/store.go:952` | `pkg/mcs/scheduling/server/config/config_test.go:TestPersistConfigDefaultStoreLimit` | 36 |
| `NeedTransferWitnessLeader` (2521) | 公开；函数/方法 | `pkg/schedule/schedulers/scheduler_controller.go:493` | 无直接用例，负责任务补 oracle | 28/32 |
| `SplitRegions` (2529) | 公开；函数/方法 | `client/client.go:92` | `pkg/core/region_tree_test.go:TestRegionTreeSplitAndMerge` | 37 |
| `MergeRegions` (2554) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_tree_test.go:TestRegionTreeSplitAndMerge` | 38 |
| `NewTestRegionInfo` (2581) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/utils/grpcutil/cluster_test.go:TestFollowerForwardAndHandleTestSuite` | 28；测试辅助 |

**region_option.go**

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `RegionCreateOption` (29) | 公开；类型/接口 | `pkg/core/region.go:143` | `pkg/core/region_test.go:TestNeedSync` | 29 |
| `WithDownPeers` (32) | 公开；函数/方法 | `tools/pd-simulator/simulator/raft.go:141` | `pkg/statistics/region_collection_test.go:TestRegionStatistics` | 29 |
| `WithFlowRoundByDigit` (40) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestRegionRoundingFlow` | 30 |
| `GetFlowRoundDivisorByDigit` (48) | 公开；函数/方法 | `server/grpc_service.go:1284` | `tests/server/cluster/cluster_test.go:TestStaleTermHeartbeat` | 30 |
| `WithPendingPeers` (53) | 公开；函数/方法 | `tools/pd-simulator/simulator/raft.go:140` | `pkg/statistics/region_collection_test.go:TestRegionStatistics` | 29 |
| `WithWitness` (61) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:441` | `pkg/schedule/scatter/region_scatterer_test.go:TestScatterReservesPeerRole` | 29 |
| `WithWitnesses` (72) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/schedule/checker/rule_checker_test.go:TestRuleCheckerTestSuite` | 29 |
| `WithLearners` (87) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/schedule/checker/rule_checker_test.go:TestRuleCheckerTestSuite` | 29 |
| `WithLeader` (102) | 公开；函数/方法 | `pkg/schedule/operator/test_util.go:30` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 29 |
| `WithStartKey` (109) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:473` | `pkg/replication/replication_mode_test.go:TestStateSwitch` | 29 |
| `WithEndKey` (116) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:474` | `pkg/replication/replication_mode_test.go:TestStateSwitch` | 29 |
| `WithNewRegionID` (123) | 公开；函数/方法 | `tools/pd-simulator/simulator/raft.go:135` | `pkg/core/region_test.go:TestSetRegion` | 29 |
| `WithNewPeerIDs` (130) | 公开；函数/方法 | `tools/pd-simulator/simulator/raft.go:136` | 无直接用例，负责任务补 oracle | 29 |
| `WithIncVersion` (142) | 公开；函数/方法 | `tools/pd-simulator/simulator/raft.go:137` | `tests/integrations/mcs/router/server_test.go:TestServerTestSuite` | 29 |
| `WithDecVersion` (157) | 公开；函数/方法 | 本文件内部/未见外部引用 | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 29 |
| `WithIncConfVer` (167) | 公开；函数/方法 | `tools/pd-simulator/simulator/task.go:311` | `pkg/schedule/placement/region_rule_cache_test.go:TestRegionRuleFitCache` | 29 |
| `WithDecConfVer` (182) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 29 |
| `WithFlashback` (192) | 公开；函数/方法 | 本文件内部/未见外部引用 | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 29 |
| `SetCPUUsage` (200) | 公开；函数/方法 | 本文件内部/未见外部引用 | `server/api/region_test.go:TestRegionsInfoMarshal` | 30 |
| `SetWrittenBytes` (207) | 公开；函数/方法 | `pkg/syncer/client.go:140` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 30 |
| `SetWrittenKeys` (214) | 公开；函数/方法 | `pkg/syncer/client.go:141` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 30 |
| `WithRemoveStorePeer` (221) | 公开；函数/方法 | `pkg/schedule/operator/test_util.go:47` | `pkg/statistics/region_collection_test.go:TestRegionStatistics` | 29 |
| `SetBuckets` (234) | 公开；函数/方法 | `tests/testutil.go:718` | `pkg/core/region_test.go:TestInherit` | 29 |
| `SetReadBytes` (250) | 公开；函数/方法 | `pkg/syncer/client.go:142` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 30 |
| `SetReadKeys` (257) | 公开；函数/方法 | `pkg/syncer/client.go:143` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 30 |
| `SetReadQuery` (264) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:489` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 31 |
| `SetWrittenQuery` (272) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:565` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 31 |
| `SetQueryStats` (282) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 31 |
| `AddQueryStats` (291) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 31 |
| `SetApproximateSize` (299) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:433` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRecordsSplitScatterInSchedulingService` | 30 |
| `SetApproximateKvSize` (306) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/statistics/region_collection_test.go:TestRegionStatsObserveIAKVSize` | 30 |
| `SetApproximateIAKvSize` (313) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/statistics/region_collection_test.go:TestRegionStatsObserveIAKVSize` | 30 |
| `SetApproximateKeys` (320) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:433` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRecordsSplitScatterInSchedulingService` | 30 |
| `SetReportInterval` (328) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:488` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 30 |
| `SetRegionConfVer` (335) | 公开；函数/方法 | `server/cluster/cluster.go:1400` | `tests/integrations/mcs/scheduling/api_test.go:TestAPI` | 29 |
| `SetRegionVersion` (346) | 公开；函数/方法 | `server/cluster/cluster.go:1400` | `tests/integrations/mcs/scheduling/api_test.go:TestAPI` | 29 |
| `SetPeers` (357) | 公开；函数/方法 | `pkg/schedule/operator/builder.go:332` | `pkg/core/region_tree_test.go:TestRegionInfo` | 29 |
| `SetReplicationStatus` (364) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/replication/replication_mode_test.go:TestStateSwitch` | 29 |
| `WithAddPeer` (371) | 公开；函数/方法 | `pkg/schedule/operator/test_util.go:39` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 29 |
| `WithRole` (383) | 公开；函数/方法 | `tools/pd-simulator/simulator/task.go:311` | `pkg/schedule/operator/operator_controller_test.go:TestOperatorControllerTestSuite` | 29 |
| `WithReplacePeerStore` (394) | 公开；函数/方法 | `pkg/schedule/checker/affinity_checker.go:480` | 无直接用例，负责任务补 oracle | 29 |
| `WithReplaceLeaderStore` (405) | 公开；函数/方法 | `pkg/schedule/checker/affinity_checker.go:482` | 无直接用例，负责任务补 oracle | 29 |
| `WithInterval` (417) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/statistics/hot_peer_cache_test.go:TestHotPeerCacheTopNThreshold` | 30 |
| `SetSource` (424) | 公开；函数/方法 | `tests/testutil.go:303` | `server/server_test.go:TestDeleteFollowerRegion` | 29 |
| `RandomKindReadQuery` (431) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:737` | 无直接用例，负责任务补 oracle | 31 |
| `RandomKindWriteQuery` (451) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:745` | 无直接用例，负责任务补 oracle | 31 |
| `mergeQueryStat` (486) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 31 |
| `resetReadQuery` (510) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 31 |
| `resetWriteQuery` (519) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 31 |

**region_tree.go**

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `regionItem` (32) | 内部；类型/接口 | `pkg/core/region.go:1070` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestMain` | 39 |
| `GetStartKey` (37, `regionItem`) | 公开；函数/方法 | `pkg/core/region.go:187` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestCreateEmptyRegionTailUsesTiKVStore` | 39 |
| `GetID` (42, `regionItem`) | 公开；函数/方法 | `pkg/core/store.go:378` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRejectsAffinityAutoSplit` | 39 |
| `GetEndKey` (47, `regionItem`) | 公开；函数/方法 | `pkg/core/region.go:187` | `pkg/replication/replication_mode_test.go:TestRecoverProgressWithSplitAndMerge` | 39 |
| `Less` (52, `regionItem`) | 公开；函数/方法 | `pkg/core/region.go:1537` | `pkg/mcs/discovery/register_test.go:TestRegisterRejectsUnleasedExistingKey` | 39 |
| `regionTree` (62) | 内部；类型/接口 | `pkg/core/region.go:1069` | `pkg/core/region_tree_test.go:TestRegionTree` | 39 |
| `newRegionTree` (74) | 内部；函数/方法 | `pkg/core/region.go:1298` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestCreateEmptyRegionDisableParanoidCheck` | 39 |
| `newRegionTreeWithCountRef` (84) | 内部；函数/方法 | `pkg/core/region.go:1085` | 无直接用例，负责任务补 oracle | 39 |
| `GetCountByRange` (96, `regionTree`) | 公开；函数/方法 | `pkg/core/region.go:2094` | 无直接用例，负责任务补 oracle | 42 |
| `length` (122, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1459` | `pkg/tso/keyspace_group_manager_test.go:TestFinishSplitKeyspaceGroupDoesNotBlockStateReads` | 41 |
| `notFromStorageRegionsCount` (129, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1878` | 无直接用例，负责任务补 oracle | 41 |
| `overlaps` (137, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1078` | `pkg/core/rangetree/range_tree_test.go:TestRingPutItem` | 40 |
| `updateRef` (163, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1267` | 无直接用例，负责任务补 oracle | 41 |
| `update` (174, `regionTree`) | 内部；函数/方法 | `pkg/core/store.go:161` | `pkg/mcs/tso/server/config_test.go:TestLoadFromConfig` | 40 |
| `updateStat` (219, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1396` | 无直接用例，负责任务补 oracle | 41 |
| `remove` (244, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:94` | `pkg/mcs/utils/expected_primary_test.go:TestMain` | 40 |
| `search` (268, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:53` | `pkg/core/region_tree_test.go:TestRegionTree` | 39/50 |
| `searchPrev` (278, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1593` | `pkg/core/region_tree_test.go:TestRegionTree` | 39/50 |
| `searchByKeys` (295, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1701` | 无直接用例，负责任务补 oracle | 39/50 |
| `searchByPrevKeys` (305, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1712` | 无直接用例，负责任务补 oracle | 39/50 |
| `find` (315, `regionTree`) | 内部；函数/方法 | `pkg/core/store.go:584` | `pkg/btree/btree_generic_test.go:TestBTreeG` | 39/50 |
| `scanRange` (331, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:2139` | 无直接用例，负责任务补 oracle | 42 |
| `scanRanges` (348, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1613` | `pkg/core/region_tree_test.go:TestStoreRegionCount` | 42 |
| `getAdjacentRegions` (360, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:2324` | `pkg/core/region_tree_test.go:TestRegionTree` | 39 |
| `getAdjacentItem` (366, `regionTree`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 39 |
| `randomRegion` (384, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1983` | `pkg/core/region_tree_test.go:TestRandomRegion` | 43 |
| `RandomRegions` (393, `regionTree`) | 公开；函数/方法 | `pkg/core/region.go:1976` | `pkg/core/region_tree_test.go:TestRandomRegion` | 43 |
| `TotalSize` (499, `regionTree`) | 公开；函数/方法 | `pkg/core/region.go:1822` | `pkg/core/region_test.go:TestSetRegion` | 41 |
| `TotalWriteRate` (508, `regionTree`) | 公开；函数/方法 | `pkg/core/region.go:1855` | `pkg/core/region_test.go:TestSetRegion` | 41 |

**store.go**

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `StoreInfo` (63) | 公开；类型/接口 | `pkg/core/store_option.go:29` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestEpochComparison` | 11 |
| `NewStoreInfo` (88) | 公开；函数/方法 | `server/cluster/cluster.go:1639` | `pkg/mcs/scheduling/server/server_test.go:TestRegionHeartbeatReturnsNotBootstrappedWhenHeartbeatStreamsMissing` | 11 |
| `NewStoreInfoWithLabel` (104) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/store_topology_test.go:TestTopologyWeight` | 11 |
| `Clone` (122, `StoreInfo`) | 公开；函数/方法 | `pkg/core/region.go:322` | `pkg/mcs/resourcemanager/metadataapi/config_service_test.go:TestConfigServiceMetadataWriteDisabledReturns403` | 16 |
| `LimitVersion` (155, `StoreInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 16 |
| `Feedback` (162, `StoreInfo`) | 公开；函数/方法 | `pkg/core/storelimit/limit.go:60` | `pkg/core/storelimit/rust_core_task_03_oracle_test.go:TestRustCoreTask03Oracle` | 16 |
| `ShallowClone` (169, `StoreInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 16 |
| `AllowLeaderTransferIn` (201, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:128` | `server/cluster/cluster_test.go:TestStores` | 18/19 |
| `AllowLeaderTransferOut` (207, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:129` | `pkg/schedule/schedulers/grant_leader_test.go:TestGrantLeaderScheduler` | 18/19 |
| `EvictedAsSlowStore` (212, `StoreInfo`) | 公开；函数/方法 | `pkg/schedule/filter/filters.go:400` | `pkg/schedule/schedulers/evict_slow_store_test.go:TestEvictSlowStoreTestSuite` | 18/19 |
| `EvictedAsStoppingStore` (217, `StoreInfo`) | 公开；函数/方法 | `pkg/schedule/filter/filters.go:409` | 无直接用例，负责任务补 oracle | 18/19 |
| `IsEvictedAsSlowTrend` (222, `StoreInfo`) | 公开；函数/方法 | `pkg/schedule/filter/filters.go:418` | 无直接用例，负责任务补 oracle | 18/19 |
| `IsAvailable` (227, `StoreInfo`) | 公开；函数/方法 | `pkg/schedule/filter/filters.go:445` | `server/cluster/cluster_test.go:TestCheckCache` | 18/19 |
| `IsTiKV` (234, `StoreInfo`) | 公开；函数/方法 | `server/server.go:1395` | `pkg/statistics/store_collection_test.go:TestTiKVNotAffectedByTiFlashCompute` | 13 |
| `IsTiFlashWrite` (240, `StoreInfo`) | 公开；函数/方法 | `pkg/statistics/collector.go:101` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestSelectLeader` | 13 |
| `IsTiFlashCompute` (245, `StoreInfo`) | 公开；函数/方法 | `pkg/schedule/handler/handler.go:1022` | `pkg/core/store_test.go:TestStoreInfoIsTiFlash` | 13 |
| `Engine` (250, `StoreInfo`) | 公开；函数/方法 | `server/api/stats.go:46` | `tests/integrations/mcs/scheduling/config_test.go:TestConfig` | 13 |
| `IsUp` (258, `StoreInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1805` | `server/cluster/cluster_test.go:TestSetOfflineStore` | 12 |
| `IsPreparing` (263, `StoreInfo`) | 公开；函数/方法 | `pkg/core/store_topology.go:73` | 无直接用例，负责任务补 oracle | 12 |
| `IsServing` (268, `StoreInfo`) | 公开；函数/方法 | `pkg/core/store_topology.go:73` | `pkg/mcs/resourcemanager/server/manager_test.go:TestMain` | 12 |
| `IsRemoving` (273, `StoreInfo`) | 公开；函数/方法 | `pkg/progress/progress.go:169` | `pkg/schedule/checker/affinity_checker_test.go:TestAffinityAvailabilityCheckWithOfflineStore` | 12 |
| `IsRemoved` (278, `StoreInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:62` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestRemoveFailedStores` | 12 |
| `GetSlowScore` (283, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:121` | 无直接用例，负责任务补 oracle | 23 |
| `GetNetworkSlowScores` (290, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:122` | 无直接用例，负责任务补 oracle | 23 |
| `GetNetworkSlowTriggers` (300, `StoreInfo`) | 公开；函数/方法 | `pkg/schedule/schedulers/evict_slow_store.go:552` | 无直接用例，负责任务补 oracle | 23 |
| `WitnessScore` (308, `StoreInfo`) | 公开；函数/方法 | `pkg/schedule/placement/fit.go:140` | 无直接用例，负责任务补 oracle | 23 |
| `IsSlow` (313, `StoreInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:446` | 无直接用例，负责任务补 oracle | 23 |
| `IsStopping` (320, `StoreInfo`) | 公开；函数/方法 | `pkg/schedule/schedulers/evict_stopping_store.go:289` | `pkg/schedule/schedulers/evict_stopping_store_test.go:TestEvictStoppingStoreTestSuite` | 23 |
| `GetSlowTrend` (327, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:98` | 无直接用例，负责任务补 oracle | 23 |
| `IsPhysicallyDestroyed` (334, `StoreInfo`) | 公开；函数/方法 | `pkg/statistics/store.go:143` | `server/cluster/cluster_test.go:TestSetOfflineStore` | 23 |
| `DownTime` (339, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:146` | 无直接用例，负责任务补 oracle | 17 |
| `GetMeta` (344, `StoreInfo`) | 公开；函数/方法 | `pkg/core/region.go:625` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRejectsAffinityAutoSplit` | 11/12/13 |
| `GetState` (349, `StoreInfo`) | 公开；函数/方法 | `pkg/core/region.go:1011` | `pkg/keyspace/tso_keyspace_group_test.go:TestRemoveKeyspacesFromGroupStaysWithinEtcdLimits` | 11/12/13 |
| `GetNodeState` (354, `StoreInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:334` | `tests/integrations/client/client_test.go:TestClientStatelessTestSuite` | 11/12/13 |
| `GetStatusAddress` (359, `StoreInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:328` | 无直接用例，负责任务补 oracle | 11/12/13 |
| `GetAddress` (364, `StoreInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:327` | `tests/server/cluster/cluster_test.go:TestGetPutConfig` | 11/12/13 |
| `GetVersion` (369, `StoreInfo`) | 公开；函数/方法 | `pkg/core/region.go:655` | `pkg/storage/storage_test.go:TestTrySwitchRegionStorage` | 11/12/13 |
| `GetLabels` (374, `StoreInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:333` | `pkg/core/store_topology_test.go:TestTopologyWeightWithEmptyStoreLabelValue` | 11/12/13 |
| `GetID` (379, `StoreInfo`) | 公开；函数/方法 | `pkg/core/region.go:620` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRejectsAffinityAutoSplit` | 11/12/13 |
| `GetLeaderCount` (384, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:111` | `tests/server/cluster/cluster_test.go:TestTransferLeaderForScheduler` | 20 |
| `GetRegionCount` (389, `StoreInfo`) | 公开；函数/方法 | `pkg/core/region.go:2126` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 20 |
| `GetLearnerCount` (394, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:119` | 无直接用例，负责任务补 oracle | 20 |
| `GetWitnessCount` (399, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:120` | 无直接用例，负责任务补 oracle | 20 |
| `GetLeaderSize` (404, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:114` | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 20 |
| `GetRegionSize` (409, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:118` | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 20 |
| `GetPendingPeerCount` (414, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:126` | `server/cluster/cluster_test.go:TestUpdateAndDeleteLabel` | 20 |
| `GetLeaderWeight` (419, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:112` | `pkg/storage/storage_test.go:TestStoreWeight` | 20 |
| `GetRegionWeight` (424, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:116` | `pkg/storage/storage_test.go:TestStoreWeight` | 20 |
| `GetLastHeartbeatTS` (429, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:137` | `pkg/statistics/store_collection_test.go:TestStoreStatistics` | 17/19 |
| `NeedPersist` (434, `StoreInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1232` | 无直接用例，负责任务补 oracle | 17/19 |
| `GetStoreLimit` (439, `StoreInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1199` | `pkg/mcs/scheduling/server/config/config_test.go:TestPersistConfigDefaultStoreLimit` | 17/19 |
| `LeaderScore` (449, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:60` | `pkg/schedule/schedulers/balance_leader_test.go:TestBalanceLeaderMaxBatchSchedule` | 20 |
| `RegionScore` (464, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:64` | `pkg/core/store_test.go:TestRegionScore` | 20 |
| `regionScoreV1` (475, `StoreInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 21 |
| `regionScoreV2` (520, `StoreInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/store_test.go:TestLowSpaceScoreV2` | 22 |
| `StorageSize` (565, `StoreInfo`) | 公开；函数/方法 | `client/http/types.go:321` | `tests/server/api/stats_test.go:TestStatTestSuite` | 23/20 |
| `AvailableRatio` (570, `StoreInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 23/20 |
| `IsLowSpace` (579, `StoreInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1226` | `pkg/core/store_test.go:TestLowSpaceRatio` | 23/20 |
| `ResourceCount` (593, `StoreInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 23/20 |
| `ResourceSize` (605, `StoreInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 23/20 |
| `ResourceWeight` (617, `StoreInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 23/20 |
| `GetStartTime` (637, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:134` | `pkg/schedule/operator/operator_test.go:TestPeerInfluenceUsesRegionKeys` | 17 |
| `GetUptime` (642, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:140` | 无直接用例，负责任务补 oracle | 17 |
| `GetMinResolvedTS` (651, `StoreInfo`) | 公开；函数/方法 | `server/api/router.go:369` | `tests/server/cluster/cluster_test.go:TestTransferLeaderForScheduler` | 17 |
| `NeedAwakenStore` (657, `StoreInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1888` | `server/cluster/cluster_test.go:TestAwakenStore` | 17 |
| `IsDisconnected` (672, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:148` | `pkg/schedule/scatter/region_scatterer_test.go:TestSomeStoresFilteredScatterGroupInConcurrency` | 17 |
| `IsUnhealthy` (677, `StoreInfo`) | 公开；函数/方法 | `pkg/statistics/store.go:143` | 无直接用例，负责任务补 oracle | 17 |
| `GetLabelValue` (682, `StoreInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1677` | `pkg/schedule/scatter/region_scatterer_test.go:TestScatterRegions` | 24 |
| `CompareLocation` (693, `StoreInfo`) | 公开；函数/方法 | `pkg/core/store_topology.go:100` | `pkg/schedule/scatter/region_scatterer_test.go:TestScatterPlacementLabelCase` | 24 |
| `DistinctScore` (709) | 公开；函数/方法 | `pkg/schedule/placement/fit.go:432` | `pkg/core/store_test.go:TestDistinctScore` | 24 |
| `MergeLabels` (724) | 公开；函数/方法 | `server/cluster/cluster.go:1644` | 无直接用例，负责任务补 oracle | 24 |
| `StoresInfo` (760) | 公开；类型/接口 | `pkg/core/basic_cluster.go:24` | `pkg/schedule/placement/region_rule_cache_test.go:TestSetCacheDoesNotPromoteStaleFitAfterStoreRemoval` | 26 |
| `NewStoresInfo` (766) | 公开；函数/方法 | `pkg/core/basic_cluster.go:31` | `pkg/storage/storage_test.go:TestLoadStores` | 26 |
| `GetStore` (775, `StoresInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:51` | `pkg/replication/replication_mode_test.go:TestAsynctimeout` | 26 |
| `GetStores` (786, `StoresInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:57` | `pkg/statistics/hot_peer_cache_test.go:TestRemoveExpireItems` | 26 |
| `GetMetaStores` (797, `StoresInfo`) | 公开；函数/方法 | `tests/cluster.go:404` | `pkg/storage/storage_test.go:TestLoadStores` | 26 |
| `GetStoreIDs` (808, `StoresInfo`) | 公开；函数/方法 | `pkg/core/region.go:567` | `pkg/core/region_tree_test.go:TestRegionInfo` | 26 |
| `GetFollowerStores` (820, `StoresInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:146` | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 56 |
| `GetRegionStores` (833, `StoresInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:144` | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 56 |
| `GetLeaderStore` (846, `StoresInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:147` | 无直接用例，负责任务补 oracle | 56 |
| `GetStoreCount` (856, `StoresInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:889` | `pkg/storage/storage_test.go:TestLoadStores` | 56 |
| `GetNonWitnessVoterStores` (863, `StoresInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:145` | 无直接用例，负责任务补 oracle | 56 |
| `GetAvgNetworkSlowScore` (876, `StoresInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:148` | `pkg/core/store_test.go:TestGetAvgNetworkSlowScore` | 26 |
| `PutStore` (909, `StoresInfo`) | 公开；函数/方法 | `tests/testutil.go:281` | `pkg/mcs/scheduling/server/server_test.go:TestRegionHeartbeatReturnsNotBootstrappedWhenHeartbeatStreamsMissing` | 27 |
| `putStoreLocked` (916, `StoresInfo`) | 内部；函数/方法 | `server/cluster/cluster.go:1165` | `pkg/core/store_test.go:TestPutStore` | 27 |
| `ResetStores` (935, `StoresInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:883` | `pkg/statistics/hot_peer_cache_test.go:TestRemoveExpireItems` | 27 |
| `PauseLeaderTransfer` (943, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:113` | `server/cluster/cluster_test.go:TestStores` | 27 |
| `ResumeLeaderTransfer` (962, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:125` | `server/cluster/cluster_test.go:TestStores` | 27 |
| `SlowStoreEvicted` (977, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:137` | `pkg/schedule/schedulers/hot_region_test.go:TestHotReadRegionScheduleSkipsLeaderEvictedMoveLeaderTarget` | 27 |
| `StoppingStoreEvicted` (990, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:145` | `pkg/schedule/schedulers/hot_region_test.go:TestHotReadRegionScheduleSkipsLeaderEvictedMoveLeaderTarget` | 27 |
| `SlowStoreRecovered` (1002, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:168` | 无直接用例，负责任务补 oracle | 27 |
| `StoppingStoreRecovered` (1015, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:175` | 无直接用例，负责任务补 oracle | 27 |
| `SlowTrendEvicted` (1029, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:153` | `pkg/schedule/schedulers/hot_region_test.go:TestHotReadRegionScheduleSkipsLeaderEvictedMoveLeaderTarget` | 27 |
| `SlowTrendRecovered` (1044, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:161` | 无直接用例，负责任务补 oracle | 27 |
| `ResetStoreLimit` (1057, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:284` | `pkg/schedule/checker/rule_checker_test.go:TestRuleCheckerTestSuite` | 27 |
| `DeleteStore` (1066, `StoresInfo`) | 公开；函数/方法 | `server/api/store.go:83` | `pkg/statistics/store_test.go:TestFilterUnhealthyStore` | 27 |
| `UpdateStoreStatus` (1073, `StoresInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:36` | `pkg/schedule/schedulers/scatter_range_test.go:TestScatterRangeBalance` | 27 |
| `TriggerNetworkSlowEvict` (1089, `StoresInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:2994` | `pkg/schedule/schedulers/evict_slow_store_test.go:TestEvictSlowStoreTestSuite` | 27 |
| `ResetTriggerNetworkSlowEvict` (1101, `StoresInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:3022` | 无直接用例，负责任务补 oracle | 27 |
| `IsStoreContainLabel` (1113) | 公开；函数/方法 | `server/grpc_service.go:863` | 无直接用例，负责任务补 oracle | 13/24 |
| `getStoreLabelValue` (1119) | 内部；函数/方法 | `pkg/core/store_topology.go:34` | 无直接用例，负责任务补 oracle | 13/24 |
| `IsAvailableForMinResolvedTS` (1129) | 公开；函数/方法 | `server/cluster/cluster.go:2649` | 无直接用例，负责任务补 oracle | 13/24 |
| `ValidateStoreEngineKey` (1137) | 公开；函数/方法 | `server/grpc_service.go:856` | 无直接用例，负责任务补 oracle | 13/24 |

**store_option.go**

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `StoreCreateOption` (29) | 公开；类型/接口 | `pkg/core/store.go:88` | `pkg/core/store_test.go:TestPutStore` | 11–13/23 |
| `SetStoreAddress` (32) | 公开；函数/方法 | `server/cluster/cluster.go:1647` | 无直接用例，负责任务补 oracle | 11–13/23 |
| `SetStoreLabels` (43) | 公开；函数/方法 | `client/http/interface.go:57` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestSelectLeader` | 11–13/23 |
| `SetStoreStartTime` (52) | 公开；函数/方法 | `server/cluster/cluster.go:1650` | 无直接用例，负责任务补 oracle | 17 |
| `SetStoreVersion` (61) | 公开；函数/方法 | `server/cluster/cluster.go:1648` | 无直接用例，负责任务补 oracle | 11–13/23 |
| `SetStoreDeployPath` (71) | 公开；函数/方法 | `server/cluster/cluster.go:1651` | 无直接用例，负责任务补 oracle | 11–13/23 |
| `SetStoreState` (80) | 公开；函数/方法 | `tests/testutil.go:277` | `pkg/statistics/store_load_test.go:TestHistoryGCDoesNotReaddRemovedStore` | 11–13/23 |
| `SetNodeState` (105) | 公开；函数/方法 | `tests/testutil.go:278` | `pkg/progress/progress_test.go:TestOnlineAndOffline` | 11–13/23 |
| `PauseLeaderTransfer` (114) | 公开；函数/方法 | `pkg/core/store.go:941` | `server/cluster/cluster_test.go:TestStores` | 18 |
| `ResumeLeaderTransfer` (126) | 公开；函数/方法 | `pkg/core/store.go:960` | `server/cluster/cluster_test.go:TestStores` | 18 |
| `SlowStoreEvicted` (139) | 公开；函数/方法 | `pkg/core/store.go:975` | `pkg/schedule/schedulers/hot_region_test.go:TestHotReadRegionScheduleSkipsLeaderEvictedMoveLeaderTarget` | 18 |
| `StoppingStoreEvicted` (147) | 公开；函数/方法 | `pkg/core/store.go:988` | `pkg/schedule/schedulers/hot_region_test.go:TestHotReadRegionScheduleSkipsLeaderEvictedMoveLeaderTarget` | 18 |
| `SlowTrendEvicted` (155) | 公开；函数/方法 | `pkg/core/store.go:1027` | `pkg/schedule/schedulers/hot_region_test.go:TestHotReadRegionScheduleSkipsLeaderEvictedMoveLeaderTarget` | 18 |
| `SlowTrendRecovered` (162) | 公开；函数/方法 | `pkg/core/store.go:1043` | 无直接用例，负责任务补 oracle | 18 |
| `SlowStoreRecovered` (169) | 公开；函数/方法 | `pkg/core/store.go:1001` | 无直接用例，负责任务补 oracle | 18 |
| `StoppingStoreRecovered` (176) | 公开；函数/方法 | `pkg/core/store.go:1014` | 无直接用例，负责任务补 oracle | 18 |
| `SetLeaderCount` (183) | 公开；函数/方法 | `pkg/core/store.go:1077` | `pkg/statistics/store_collection_test.go:TestStoreStatistics` | 20/23 |
| `SetRegionCount` (190) | 公开；函数/方法 | `pkg/core/store.go:1078` | `pkg/core/store_test.go:TestNewStore` | 20/23 |
| `SetLearnerCount` (197) | 公开；函数/方法 | `pkg/core/store.go:1080` | 无直接用例，负责任务补 oracle | 20/23 |
| `SetWitnessCount` (204) | 公开；函数/方法 | `pkg/core/store.go:1079` | 无直接用例，负责任务补 oracle | 20/23 |
| `SetPendingPeerCount` (211) | 公开；函数/方法 | `pkg/core/store.go:1081` | 无直接用例，负责任务补 oracle | 20/23 |
| `SetLeaderSize` (218) | 公开；函数/方法 | `pkg/core/store.go:1082` | `server/cluster/cluster_test.go:TestUpdateAndDeleteLabel` | 20/23 |
| `SetRegionSize` (225) | 公开；函数/方法 | `pkg/core/store.go:1083` | `pkg/core/store_test.go:TestRegionScore` | 20/23 |
| `SetLeaderWeight` (232) | 公开；函数/方法 | `server/cluster/cluster.go:2002` | 无直接用例，负责任务补 oracle | 20/23 |
| `SetRegionWeight` (239) | 公开；函数/方法 | `server/cluster/cluster.go:2003` | 无直接用例，负责任务补 oracle | 20/23 |
| `SetLastHeartbeatTS` (246) | 公开；函数/方法 | `tests/testutil.go:279` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestRemoveFailedStores` | 17 |
| `SetLastPersistTime` (253) | 公开；函数/方法 | `server/cluster/cluster.go:1236` | 无直接用例，负责任务补 oracle | 17 |
| `SetStoreStats` (260) | 公开；函数/方法 | `tests/testutil.go:272` | `pkg/core/store_stats_test.go:TestStoreStats` | 14/15/16 |
| `SetNewStoreStats` (267) | 公开；函数/方法 | `pkg/schedule/schedulers/range_cluster.go:63` | 无直接用例，负责任务补 oracle | 14/15/16 |
| `SetMinResolvedTS` (278) | 公开；函数/方法 | `server/grpc_service.go:2686` | `tests/integrations/client/http_client_test.go:TestHTTPClientTestSuiteWithServiceDiscovery` | 17 |
| `ResetStoreLimit` (285) | 公开；函数/方法 | `pkg/core/store.go:1056` | `pkg/schedule/checker/rule_checker_test.go:TestRuleCheckerTestSuite` | 19 |
| `SetStoreLimit` (298) | 公开；函数/方法 | `server/handler.go:270` | `pkg/statistics/store_collection_test.go:TestStoreLimitMetricsIncludeTransferLeaderIn` | 19 |
| `SetLastAwakenTime` (307) | 公开；函数/方法 | `server/cluster/cluster.go:1216` | `server/cluster/cluster_test.go:TestAwakenStore` | 17 |
| `SetNetworkSlowTriggers` (314) | 公开；函数/方法 | `pkg/core/store.go:1094` | 无直接用例，负责任务补 oracle | 11–13/23 |
| `SetStoreMeta` (322) | 公开；函数/方法 | `pkg/mcs/router/server/meta/watcher.go:81` | 无直接用例，负责任务补 oracle | 11–13/23 |

**store_stats.go**

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `DFSStats` (26) | 公开；类型/接口 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 14/16 |
| `storeStats` (31) | 内部；类型/接口 | `pkg/core/store.go:65` | `pkg/statistics/store_collection_test.go:TestStoreStatistics` | 14/16 |
| `newStoreStats` (39) | 内部；函数/方法 | `pkg/core/store.go:91` | 无直接用例，负责任务补 oracle | 15 |
| `updateRawStats` (46, `storeStats`) | 内部；函数/方法 | `pkg/core/store_option.go:262` | 无直接用例，负责任务补 oracle | 14/16 |
| `GetStoreStats` (57, `storeStats`) | 公开；函数/方法 | `pkg/core/store.go:580` | `tests/integrations/mcs/scheduling/server_test.go:TestServerTestSuite` | 14/16 |
| `CloneStoreStats` (64, `storeStats`) | 公开；函数/方法 | `pkg/schedule/schedulers/range_cluster.go:59` | 无直接用例，负责任务补 oracle | 14/16 |
| `GetCapacity` (72, `storeStats`) | 公开；函数/方法 | `pkg/core/store.go:480` | `pkg/core/store_stats_test.go:TestStoreStats` | 14/16 |
| `GetAvailable` (79, `storeStats`) | 公开；函数/方法 | `pkg/core/store.go:478` | `pkg/core/store_stats_test.go:TestStoreStats` | 14/16 |
| `GetUsedSize` (86, `storeStats`) | 公开；函数/方法 | `pkg/core/store.go:479` | `pkg/core/store_stats_test.go:TestStoreStats` | 14/16 |
| `GetBytesWritten` (93, `storeStats`) | 公开；函数/方法 | `pkg/core/region.go:226` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 14/16 |
| `GetBytesRead` (100, `storeStats`) | 公开；函数/方法 | `pkg/core/region.go:228` | `pkg/statistics/hot_peer_cache_test.go:TestCacheInherit` | 14/16 |
| `GetKeysWritten` (107, `storeStats`) | 公开；函数/方法 | `pkg/core/region.go:227` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 14/16 |
| `GetKeysRead` (114, `storeStats`) | 公开；函数/方法 | `pkg/core/region.go:229` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 14/16 |
| `IsBusy` (121, `storeStats`) | 公开；函数/方法 | `pkg/response/store.go:74` | `pkg/schedule/filter/filters_test.go:TestStoreStateFilter` | 14/16 |
| `GetSendingSnapCount` (128, `storeStats`) | 公开；函数/方法 | `pkg/response/store.go:124` | 无直接用例，负责任务补 oracle | 14/16 |
| `GetReceivingSnapCount` (135, `storeStats`) | 公开；函数/方法 | `pkg/response/store.go:125` | 无直接用例，负责任务补 oracle | 14/16 |
| `GetAvgAvailable` (142, `storeStats`) | 公开；函数/方法 | `pkg/core/store.go:521` | `pkg/core/store_stats_test.go:TestStoreStats` | 15 |
| `climp0` (151) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 15 |

**store_topology.go**

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `GetStoreTopoWeight` (24) | 公开；函数/方法 | `server/cluster/cluster.go:2175` | `pkg/core/store_topology_test.go:TestTopologyWeight` | 25 |
| `buildTopology` (60) | 内部；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/store_topology_test.go:TestTopologyWeightUsesCaseSensitiveLabelKeys` | 25 |
| `updateTopology` (109) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 25 |
| `cleanTopology` (126) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 25 |
| `getTopology` (136) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 25 |
| `putTopology` (142) | 内部；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/store_topology_test.go:TestTopologyWeightUsesCaseSensitiveLabelKeys` | 25 |

**常量、变量与接口方法补表**

| 源文件 | 声明 | 可见性/用途、Rust 缺口与责任 |
| --- | --- | --- |
| `basic_cluster.go:97` RegionSetInformer | `GetTotalRegionCount`, `RandFollowerRegions`, `RandLeaderRegions`, `RandLearnerRegions`, `RandWitnessRegions`, `RandPendingRegions`, `GetAverageRegionSize`, `GetStoreRegionCount`, `GetRegion`, `GetAdjacentRegions`, `ScanRegions`, `GetRegionByKey`, `BatchScanRegions`, `GetRegionCount`, `GetStoreLeaderCountByRange`, `GetStoreLearnerCountByRange`, `GetStorePeerCountByRange` | 大写公开、其余内部；历史缺失快照；当前见任务映射；26/27/50–58；informer/controller 与扫描选项 |
| `basic_cluster.go:140` StoreSetInformer | `GetStores`, `GetStore`, `GetRegionStores`, `GetNonWitnessVoterStores`, `GetFollowerStores`, `GetLeaderStore`, `GetAvgNetworkSlowScore` | 大写公开、其余内部；历史缺失快照；当前见任务映射；26/27/50–58；informer/controller 与扫描选项 |
| `basic_cluster.go:152` StoreSetController | `PauseLeaderTransfer`, `ResumeLeaderTransfer`, `SlowStoreEvicted`, `StoppingStoreEvicted`, `SlowStoreRecovered`, `StoppingStoreRecovered`, `SlowTrendEvicted`, `SlowTrendRecovered` | 大写公开、其余内部；历史缺失快照；当前见任务映射；26/27/50–58；informer/controller 与扫描选项 |
| `factory.go:50` var | `RegionFactory`, `RegionPeerFactory`, `RegionHeartbeatResponseFactory`, `PeerStatsFactory`, `StoreFactory`, `StoreStatsFactory`, `KeyDictionaryFactory`, `MemberFactory`, `ClusterFactory`, `TimeIntervalFactory`, `QueryStatsFactory`, `CPUStatsFactory` | 大写公开、其余内部；历史缺失快照；当前见任务映射；16/29/60/61；工厂不迁移，其他属后续主题 |
| `metrics.go:127` var | `HeartbeatBreakdownHandleDurationSum`, `HeartbeatBreakdownHandleCount`, `AcquireRegionsLockWaitDurationSum`, `AcquireRegionsLockWaitCount`, `waitRegionsLockDurationSum`, `waitRegionsLockCount`, `waitSubRegionsLockDurationSum`, `waitSubRegionsLockCount`, `preCheckDurationSum`, `preCheckCount`, `asyncHotStatsDurationSum`, `asyncHotStatsCount`, `regionGuideDurationSum`, `regionGuideCount`, `checkOverlapsDurationSum`, `checkOverlapsCount`, `validateRegionDurationSum`, `validateRegionCount`, `setRegionDurationSum`, `setRegionCount`, `updateSubTreeDurationSum`, `updateSubTreeCount`, `regionCollectDurationSum`, `regionCollectCount`, `otherDurationSum`, `otherCount`, `queryRegionDuration`, `queryRegionCount`, `queryRegionByKeysDuration`, `queryRegionByPrevKeysDuration`, `queryRegionByIDsDuration`, `queryRegionKeysCount`, `queryRegionPrevKeysCount`, `queryRegionIDsCount`, `bucketEventCounter`, `versionStaleCounter`, `versionNotChangeCounter`, `RegionCacheMissCounter`, `UpdateFailedCounter`, `UpdateSuccessCounter` | 大写公开、其余内部；历史缺失快照；当前见任务映射；59；其他指标属后续宿主主题 |
| `metrics.go:139` 单项 | `tracerPool` | 大写公开、其余内部；历史缺失快照；当前见任务映射；59；其他指标属后续宿主主题 |
| `metrics.go:146` RegionHeartbeatProcessTracer | `Begin`, `OnPreCheckFinished`, `OnAsyncHotStatsFinished`, `OnRegionGuideFinished`, `OnSaveCacheBegin`, `OnSaveCacheFinished`, `OnCheckOverlapsFinished`, `OnValidateRegionFinished`, `OnSetRegionFinished`, `OnUpdateSubTreeFinished`, `OnCollectRegionStatsFinished`, `OnAllStageFinished`, `Release` | 大写公开、其余内部；历史缺失快照；当前见任务映射；59；其他指标属后续宿主主题 |
| `region.go:57` const | `randomRegionMaxRetry`, `ScanRegionLimit`, `batchSearchSize`, `CollectFactor` | 大写公开、其余内部；历史缺失快照；当前见任务映射；10/28/30/36/44/50–56/61；阈值、来源、规则与子树角色 |
| `region.go:114` const | `Storage`, `Sync`, `Heartbeat` | 大写公开、其余内部；历史缺失快照；当前见任务映射；10/28/30/36/44/50–56/61；阈值、来源、规则与子树角色 |
| `region.go:206` const | `EmptyRegionApproximateSize`, `EmptyRegionApproximateKeys`, `ImpossibleFlowSize`, `statsReportMinInterval`, `statsReportMaxInterval`, `InitClusterRegionThreshold` | 大写公开、其余内部；历史缺失快照；当前见任务映射；10/28/30/36/44/50–56/61；阈值、来源、规则与子树角色 |
| `region.go:214` RegionHeartbeatResponse | `GetTargetPeer`, `GetRegionId` | 大写公开、其余内部；历史缺失快照；当前见任务映射；10/28/30/36/44/50–56/61；阈值、来源、规则与子树角色 |
| `region.go:220` RegionHeartbeatRequest | `GetTerm`, `GetRegion`, `GetLeader`, `GetDownPeers`, `GetPendingPeers`, `GetBytesWritten`, `GetKeysWritten`, `GetBytesRead`, `GetKeysRead`, `GetInterval`, `GetQueryStats`, `GetApproximateSize`, `GetApproximateKeys`, `GetBucketMeta` | 大写公开、其余内部；历史缺失快照；当前见任务映射；10/28/30/36/44/50–56/61；阈值、来源、规则与子树角色 |
| `region.go:416` const | `LeaderScatter`, `PeerScatter`, `LearnerScatter`, `Unknown` | 大写公开、其余内部；历史缺失快照；当前见任务映射；10/28/30/36/44/50–56/61；阈值、来源、规则与子树角色 |
| `region.go:1780` const | `AllInSubTree`, `LeaderInSubTree`, `FollowerInSubTree`, `LearnerInSubTree`, `WitnessInSubTree`, `PendingPeerInSubTree` | 大写公开、其余内部；历史缺失快照；当前见任务映射；10/28/30/36/44/50–56/61；阈值、来源、规则与子树角色 |
| `region.go:2285` 单项 | `defaultPollInterval` | 大写公开、其余内部；历史缺失快照；当前见任务映射；10/28/30/36/44/50–56/61；阈值、来源、规则与子树角色 |
| `region_tree.go:60` const | `defaultBTreeDegree` | 大写公开、其余内部；历史缺失快照；当前见任务映射；39；BTree degree |
| `store.go:59` const | `storePersistInterval`, `initialMinSpace`, `slowStoreThreshold`, `awakenStoreInterval`, `EngineKey`, `EngineRoleKey`, `EngineTiFlash`, `EngineTiFlashCompute`, `EngineTiKV` | 大写公开、其余内部；历史缺失快照；当前见任务映射；11–13/17/20–24；状态阈值与引擎名 |
| `store.go:445` 单项 | `minWeight` | 大写公开、其余内部；历史缺失快照；当前见任务映射；11–13/17/20–24；状态阈值与引擎名 |
| `store.go:446` 单项 | `maxScore` | 大写公开、其余内部；历史缺失快照；当前见任务映射；11–13/17/20–24；状态阈值与引擎名 |
| `store.go:667` var | `storeDisconnectDuration`, `storeUnhealthyDuration` | 大写公开、其余内部；历史缺失快照；当前见任务映射；11–13/17/20–24；状态阈值与引擎名 |
| `store.go:705` 单项 | `replicaBaseScore` | 大写公开、其余内部；历史缺失快照；当前见任务映射；11–13/17/20–24；状态阈值与引擎名 |
| `store_topology.go:49` 单项 | `topologyPool` | 大写公开、其余内部；历史缺失快照；当前见任务映射；25；pool 是分配优化 |

### storelimit-contract.md

| Go 源码及符号 | 可见性、契约与用途 | 调用方 | Go 测试与缺口 | 负责任务 |
| --- | --- | --- | --- | --- |
| `limit.go`: `Type`, `AddPeer`, `RemovePeer`, `SendSnapshot`, `TransferLeaderIn` | 公开；数值依次 0、1、2、3 | C1–C8 | 现有测试使用部分枚举；完整数值与名称待补 | 2 类型映射 |
| `limit.go`: `storeLimitTypeLen` | 内部；值 4，分配独立类型桶 | `NewStoreRateLimit` | 四桶隔离待补 | 2、3 |
| `limit.go`: `VersionV1`, `VersionV2` | 公开；字符串 `v1`、`v2` | C1、C5、C7 | 本包无版本断言；C5 cluster 测试有版本切换 | 2 定义，3/6 返回，19 装配 |
| `limit.go`: `StoreLimit` | 公开接口，包含 `Available`, `Take`, `Reset`, `Feedback`, `Ack`, `Version` 六方法；查询、消费、完成返还的 cost 须一致 | C1、C3、C5 | 两实现均编译期断言符合接口；跨实现装配待补 | 3/4 v1，6/7 v2，19 统一装配 |
| `store_limit.go`: `SmallRegionThreshold` | 公开；20，小 region 边界由调用方判断 | C3 | 本包不测分类；operator influence 测试及任务 2 常量 oracle | 2；调度分类范围外 |
| `store_limit.go`: `Unlimited` | 公开；1e8，有限大数标志，非 `f64::MAX` | C3、C5–C8 | 1e8 上下边界待补 | 2 定义，4 重置行为 |
| `store_limit.go`: `influence`, `smallInfluence` | 内部；1000、200 | 影响表、`limit.Reset` | 完整常量待补 | 2、4 |
| `store_limit.go`: `RegionInfluence` | 公开切片，四项均 1000 | C1、C3 | 完整表待补 | 2 |
| `store_limit.go`: `SmallRegionInfluence` | 公开切片，仅 AddPeer/RemovePeer 两项为 200；其余索引越界，不能伪造四项 | C3 | 长度、索引适用范围待补 | 2 |
| `store_limit.go`: `TypeNameValue`, `Type.String` | 公开；四名称 `add-peer`、`remove-peer`、`send-snapshot`、`transfer-leader-in`；未知枚举 String 返回空串，未知名称 map lookup 的 ok 为 false；不依赖 map 遍历顺序 | C3、C5–C7 | 全名称、未知输入待补 | 2 |
| `store_limit.go`: `StoreRateLimit`, `NewStoreRateLimit` | 公开；四个独立 `limit`，逐个以相同 rate Reset，返回接口 | C1、C5 | `TestStoreLimit`，四桶隔离及零初值待补 | 3，零 rate 由 4 完整验证 |
| `store_limit.go`: `StoreRateLimit.Available`, `StoreRateLimit.Take` | 公开；SendSnapshot 直接 true，其余按 Type 选桶；忽略优先级；Available 查询，Take 消耗并在不足时拒绝 | C1–C3 | `TestStoreLimit`；snapshot 旁路、多类型及优先级待补 | 3 |
| `store_limit.go`: `StoreRateLimit.Rate` | 公开；返回原始 rate；槽位 nil 返回 0；不作 Type 越界防护 | C3、C5 | `TestStoreLimit` 仅初始 rate，nil 槽位待补 | 3、4 |
| `store_limit.go`: `StoreRateLimit.Reset` | 公开；snapshot 不处理，其他桶重置 | C1、C3、C5 | `TestStoreLimit` 同 rate 与零 rate；snapshot 旁路待补 | 4 |
| `store_limit.go`: `StoreRateLimit.Ack`, `StoreRateLimit.Feedback`, `StoreRateLimit.Version` | 公开；前两者无操作，版本 v1 | C1、C3、C5 | 无操作及版本的直接断言待补 | 3 |
| `store_limit.go`: `limit` | 内部；limiter 指针、读写锁、原始 rate；初始 rate 为 0 可使首次 Reset(0) 不建立桶 | `StoreRateLimit` | `TestStoreLimit`；新建零 rate 待补 | 3、4 |
| `store_limit.go`: `limit.Reset` | 内部；相同 rate 不变更桶；rate>=1e8 时容量 1e8 且补充速率保留输入；否则容量为 1000 或 rate>1 时截断 rate*1000，补充速率为 rate*1000；改变 rate 新建满桶 | 构造器、`StoreRateLimit.Reset` | `TestStoreLimit`；小数、1、Unlimited、换 rate 及补充待补 | 4 |
| `store_limit.go`: `limit.Available`, `limit.Take`, `limit.GetRatePerSec` | 内部；读锁保护；原始 rate==0 时前两者直接 true，否则委托 `RateLimiter.Available`/`AllowN`；Get 返回原始 rate | 同名公开封装、`Rate` | `TestStoreLimit`；有符号 cost 与时间边界待补 | 3、4 |
| `sliding_window.go`: `minSnapSize`, `defaultWindowSize` | 内部；10、100，最小剩余空间判断及初始容量 | 窗口、构造、反馈 | `TestWindow`, `TestSlidingWindow` | 5、6 |
| `sliding_window.go`: `defaultProportion`, `defaultIntegral` | 内部；20、10，反馈比例与积分系数 | `Feedback` | `TestFeedback` 仅最终区间，确定误差序列待补 | 7 |
| `sliding_window.go`: `SlidingWindows`, `NewSlidingWindows` | 公开；四级窗口，初始容量依次 100、50、25、12，读写锁保护；lastSum 初值 0 | C5；C1 保存接口 | `TestSlidingWindow` 断言构造容量 | 6 |
| `sliding_window.go`: `SlidingWindows.Version`, `SlidingWindows.Reset` | 公开；版本 v2；Reset 对任何 rate/type 均无操作 | C1、C5 | 直接断言待补 | 6 |
| `sliding_window.go`: `SlidingWindows.GetCap`, `SlidingWindows.GetUsed` | 公开；返回第 0 窗口容量和独立 used 切片快照，读锁保护 | C4 | `TestSlidingWindow`, `TestFeedback`；快照隔离待补 | 6、7 |
| `sliding_window.go`: `SlidingWindows.Available` | 公开；snapshot 扫描 0..=level，任一窗口可用即 true；忽略 cost；非 snapshot 恒 true | C1、C2、C3 | `TestSlidingWindow`；四级及门槛待补 | 6 |
| `sliding_window.go`: `SlidingWindows.Take` | 公开；snapshot 从低到高选择首个可用窗口，消费整个 token；非 snapshot 恒 true 且不改变 used | C3 | `TestSlidingWindow`；高、urgent 耗尽路径待补 | 6 |
| `sliding_window.go`: `SlidingWindows.Ack` | 公开；snapshot 从高到低返还，将余数传至下层；非 snapshot 无操作 | C3 | `TestSlidingWindow`；跨四级和过量返还待补 | 6 |
| `sliding_window.go`: `SlidingWindows.Feedback` | 公开；low 尚可用则完全不变；否则 lastSum+=e，cap=max(100,20*e+10*lastSum)，再 set(snapshot) | C1、C5 | `TestFeedback` 使用随机大小和 ticker；确定序列与截断待补 | 7 |
| `sliding_window.go`: `SlidingWindows.set` | 内部；仅 snapshot；负 cap 先改为 10；各层容量按 int64(cap)>>i，再由 reset 保底；不清 used | `Feedback`；`TestSlidingWindow` | `TestSlidingWindow`；负值、截断及保留 used 待补 | 6 基础设置，7 反馈调容 |
| `sliding_window.go`: `window`, `newWindow`, `window.reset` | 内部；capacity 至少 10；used 初值 0，count 为 uint 初值 0；reset 仅改容量 | `NewSlidingWindows`、`set` | `TestWindow`；reset 保留 used/count 待补 | 5 |
| `sliding_window.go`: `window.available`, `window.take`, `window.getUsed` | 内部；used+10<=capacity 即可接纳，take 在判断后增加整个 token 和 count，允许一次越容量；getUsed 返回 used | 多级窗口 | `TestWindow`, `TestSlidingWindow` | 5 |
| `sliding_window.go`: `window.ack` | 内部；used==0 时直接返回 token；否则扣到零并返回剩余 token，每次非空 ack 将 count 减一；并非按 cost 预留的严格配对模型 | `SlidingWindows.Ack` | `TestWindow` 覆盖空、恰好、超量；部分扣减待补 | 5 |
| 代号 | 源码路径与调用 | 迁移责任 |
| --- | --- | --- |
| C1 | [core/store.go](../../pkg/core/store.go)：默认 v1、limiter 字段、Clone/ShallowClone 共享、LimitVersion、Feedback、IsAvailable、GetStoreLimit、StoresInfo.ResetStoreLimit；[store_option.go](../../pkg/core/store_option.go)：ResetStoreLimit、SetStoreLimit | 16 共享；19 装配；27 按 ID 控制 |
| C2 | [filter/filters.go](../../pkg/schedule/filter/filters.go) 与 [region_filters.go](../../pkg/schedule/filter/region_filters.go)：IsAvailable 检查 AddPeer/RemovePeer/TransferLeaderIn/SendSnapshot | 本计划提供 19 的可用性 API；完整 filter/调度迁移范围外 |
| C3 | [operator/influence.go](../../pkg/schedule/operator/influence.go)：StepCost 与大小影响；[step.go](../../pkg/schedule/operator/step.go)：peer/snapshot/leader cost；[operator_controller.go](../../pkg/schedule/operator/operator_controller.go)：检查、Take、Ack、按配置 rate/60 重置、Rate 类型断言 | 2–7 提供行为；完整 operator controller 及调度策略范围外，不把外部 rate/60 换算移入限流库 |
| C4 | [statistics/store_collection.go](../../pkg/statistics/store_collection.go)：断言 SlidingWindows，读取 GetCap/GetUsed，上报 `windows_size` 与 `windows_used_level_` 指标 | 6/7 保留观察能力；statistics 指标采集迁移范围外 |
| C5 | [server/cluster/cluster.go](../../server/cluster/cluster.go)：按版本构造、更换 limiter；snapshot 反馈；Rate 查询及重置；[server/handler.go](../../server/handler.go)：设置 Store limit | 19 覆盖领域装配；心跳服务及配置网络接入范围外 |
| C6 | [schedule/config/config.go](../../pkg/schedule/config/config.go)、[config_provider.go](../../pkg/schedule/config/config_provider.go)、[server/config/persist_options.go](../../server/config/persist_options.go)、[mcs scheduling config](../../pkg/mcs/scheduling/server/config/config.go)：默认值、按类型读写，含 TransferLeaderIn、Unlimited | 2 保留枚举和常量；配置、持久化和 MCS 迁移范围外 |
| C7 | [server/api/store.go](../../server/api/store.go)：v1 校验、名称解析；[pd-ctl store_command.go](../../tools/pd-ctl/pdctl/command/store_command.go)：Unlimited 参数边界 | 2 提供名称查找；HTTP/CLI 校验与工具迁移范围外 |
| C8 | [mockcluster.go](../../pkg/mock/mockcluster/mockcluster.go)：测试集群默认 AddPeer/RemovePeer limit | Go 测试辅助，非生产 Rust API |
| 依赖 | 当前源码 API 和已有测试 | 差异、缺口及后续责任 |
| --- | --- | --- |
| `astersql-pd-core-constant` | lib 入口是 [kind.rs](../../pkg/core/constant/kind.rs)，并非 lib.rs；`PriorityLevel::{LOW,MEDIUM,HIGH,URGENT,LEN}` 数值 0..4，`from_i32`、`as_i32`、Display；[kind_test.rs](../../pkg/core/constant/kind_test.rs) 的 `priority_and_resource_kind` 覆盖四级、长度与未知显示 | 可复用；不存在 `PriorityLevelLen` 或直接整数索引 API；任务 6 显式处理数值转换及无效值。注释的优先级措辞不能覆盖实际枚举顺序/Go 低到高扫描语义 |
| `astersql-pd-ratelimit` | [lib.rs](../../pkg/ratelimit/lib.rs) 重导出 [rate_limiter.rs](../../pkg/ratelimit/rate_limiter.rs) 的 `RateLimiter::new(f64,i64)`、async `available(i64)`、`allow_n(i64)`、`set_limit`、`set_burst`、`limit`、`burst`，另有 allow/wait_n；初始满桶，Tokio Instant 计时 | storelimit 所需查询/消费/补充能力存在，但没有同步 `Available/AllowN`，也没有注入时钟构造器。任务 3/19 必须显式传播 async 或在其故事内解决接口边界；禁止 block_on/新建私有运行时。任务 4 可沿用暂停 Tokio 时钟测试，不得写成已有显式时钟参数 |
| ratelimit 测试 | [rate_limiter_test.rs](../../pkg/ratelimit/rate_limiter_test.rs) 注册于 lib.rs：`token_bucket_oracle_and_updates`、`token_bucket_wait_cancel_deadline_and_refund`、`token_bucket_zero_infinite_and_concurrency`、`token_bucket_wait_serializes_and_drop_refunds`、`token_bucket_fractional_refill_and_burst_growth`、`token_bucket_available_does_not_consume_on_running_clock` 七项 | 源码覆盖查询不消费、耗尽、补充、小数、零/无限和并发；本次未运行。裸桶零 rate 会耗尽，与 storelimit 的零 rate 旁路不同；INF=f64::MAX 与 Unlimited=1e8 不同。任务 3/4 必须自行验证封装语义 |
| storelimit crate | 当前无 manifest、lib 或实现；workspace 中也无 `astersql-pd-storelimit` | 任务 2 登记 canonical crate；3–7 逐项实现。此项是计划交付，不是声称已有的叶子依赖 |

### response-contract.md

| 符号 | 公开或内部用途、调用方 | Go 断言依据（本次未运行） | Rust 缺口与任务 |
| --- | --- | --- | --- |
| MetaPeer、PDPeerStats；fromPeer/fromPeerSlice/fromPeerStats/fromPeerStatsSlice | 公开 HTTP DTO；内部构造由 InitRegion 调用 | pkg/response/region_test.go: TestPeer、TestPeerStats，四种 role 的完整 map | core Peer/DownPeer 已有；JSON 与 nil 兼容归 63 |
| 两个 setDefaultIfNil | 内部 easyjson 空嵌入指针防护，由 covertAPIRegionInfo 调用 | server/api/region_test.go: TestRegionsInfoMarshal 的 nil peer/leader/pending/down peer.peer | 63 的表示与 64/65 序列化入口 |
| RegionInfo、ReplicationStatus；fromPBReplicationStatus | 公开区域 DTO、内部状态转换 | TestEasyJSONCompatibility；server/api/region_test.go: TestRegionCPUUsageCompatibility | core 元数据/统计/复制状态已有，DTO 与转换归 64 |
| NewAPIRegionInfo、InitRegion | 公开构造与复用；server/api/region_test.go 直接构造 | TestRegionCPUUsageCompatibility；TestRegionsInfoMarshal | 64，复用边界在 65 |
| RegionInfo.Adjust、RegionsInfo.Adjust | 公开但仅用于测试，恢复 DownPeers 的嵌入 PeerStats.Peer | server API JSON 反序列化比较；不能作为生产转换 | 64/65 的测试规范化；Rust 不需要复制指针别名 |
| RegionsInfo、MarshalRegionsInfoJSON | 公开列表与取消；server/api/region.go 和 scheduling v1 API | TestRegionsInfoMarshal | 65 |
| MarshalRegionInfoJSON、covertAPIRegionInfo（源码拼写） | 公开单项序列化、内部复用/修复；server/api/region.go | TestEasyJSONCompatibility 仅 roundtrip；nil 行为见 TestRegionsInfoMarshal | 64，取消细节 65 |
| MetaStore、SlowTrend、StoreStatus、StoreInfo、StoresInfo、BuildStoreInfo | 公开 Store DTO；server/api/store.go、label.go；pkg/mcs/scheduling/server/apis/v1/api.go | server/api/store_test.go: TestDownState；response 包无 Store 字段逐项测试 | 66 字段与格式，67 状态 |
| DisconnectedName、DownStateName | 公开常量；store.go 及 server/api/store.go 状态过滤 | TestDownState | 67 |
| 五个 DTO 的 MarshalJSON/MarshalEasyJSON/UnmarshalJSON/UnmarshalEasyJSON | MetaPeer、PDPeerStats、RegionInfo、RegionsInfo、ReplicationStatus 的公开生成方法；easyjsonXXEncode/Decode 为内部函数，含 RegionEpoch helper | TestEasyJSONCompatibility（注意 json.Marshal 也会调用生成 MarshalJSON，并非独立编码器差分） | 63–65 用 serde 等表达契约，不移植生成代码 |

**RegionInfo（任务 64）**

| Go 字段 | JSON key | 类型 | 存在规则 |
| --- | --- | --- | --- |
| ID | `id` | `uint64` | 必有 |
| StartKey | `start_key` | `string` | 必有 |
| EndKey | `end_key` | `string` | 必有 |
| RegionEpoch | `epoch` | `*metapb.RegionEpoch` | 空值省略 |
| Peers | `peers` | `[]MetaPeer` | 空值省略 |
| Leader | `leader` | `MetaPeer` | 必有对象（生成器忽略 struct 的 omitempty） |
| DownPeers | `down_peers` | `[]PDPeerStats` | 空值省略 |
| PendingPeers | `pending_peers` | `[]MetaPeer` | 空值省略 |
| CPUUsage | `cpu_usage` | `uint64` | 必有 |
| WrittenBytes | `written_bytes` | `uint64` | 必有 |
| ReadBytes | `read_bytes` | `uint64` | 必有 |
| WrittenKeys | `written_keys` | `uint64` | 必有 |
| ReadKeys | `read_keys` | `uint64` | 必有 |
| ApproximateSize | `approximate_size` | `int64` | 必有 |
| ApproximateKeys | `approximate_keys` | `int64` | 必有 |
| ApproximateKvSize | `approximate_kv_size` | `int64` | 必有 |
| ApproximateColumnarKvSize | `approximate_columnar_kv_size` | `int64` | 必有 |
| Buckets | `buckets` | `[]string` | 空值省略 |
| BucketVersion | `bucket_version` | `uint64` | 空值省略 |
| ReplicationStatus | `replication_status` | `*ReplicationStatus` | 空值省略 |

**ReplicationStatus（任务 64）**

| Go 字段 | JSON key | 类型 | 存在规则 |
| --- | --- | --- | --- |
| State | `state` | `string` | 必有 |
| StateID | `state_id` | `uint64` | 必有 |

**RegionsInfo（任务 65）**

| Go 字段 | JSON key | 类型 | 存在规则 |
| --- | --- | --- | --- |
| Count | `count` | `int` | 必有 |
| Regions | `regions` | `[]RegionInfo` | 必有 |

**SlowTrend（任务 66）**

| Go 字段 | JSON key | 类型 | 存在规则 |
| --- | --- | --- | --- |
| CauseValue | `cause_value` | `float64` | 必有 |
| CauseRate | `cause_rate` | `float64` | 必有 |
| ResultValue | `result_value` | `float64` | 必有 |
| ResultRate | `result_rate` | `float64` | 必有 |

**StoreStatus（任务 66）**

| Go 字段 | JSON key | 类型 | 存在规则 |
| --- | --- | --- | --- |
| Capacity | `capacity` | `typeutil.ByteSize` | 必有 |
| Available | `available` | `typeutil.ByteSize` | 必有 |
| UsedSize | `used_size` | `typeutil.ByteSize` | 必有 |
| LeaderCount | `leader_count` | `int` | 必有 |
| LeaderWeight | `leader_weight` | `float64` | 必有 |
| LeaderScore | `leader_score` | `float64` | 必有 |
| LeaderSize | `leader_size` | `int64` | 必有 |
| RegionCount | `region_count` | `int` | 必有 |
| RegionWeight | `region_weight` | `float64` | 必有 |
| RegionScore | `region_score` | `float64` | 必有 |
| RegionSize | `region_size` | `int64` | 必有 |
| LearnerCount | `learner_count` | `int` | 空值省略 |
| WitnessCount | `witness_count` | `int` | 空值省略 |
| PendingPeerCount | `pending_peer_count` | `int` | 空值省略 |
| SlowScore | `slow_score` | `uint64` | 空值省略 |
| NetworkSlowScore | `network_slow_score` | `map[uint64]uint64` | 空值省略 |
| SlowTrend | `slow_trend` | `*SlowTrend` | 空值省略 |
| SendingSnapCount | `sending_snap_count` | `uint32` | 空值省略 |
| ReceivingSnapCount | `receiving_snap_count` | `uint32` | 空值省略 |
| IsBusy | `is_busy` | `bool` | 空值省略 |
| StartTS | `start_ts` | `*time.Time` | 空值省略 |
| LastHeartbeatTS | `last_heartbeat_ts` | `*time.Time` | 空值省略 |
| Uptime | `uptime` | `*typeutil.Duration` | 空值省略 |
| PauseLeaderTransferIn | `pause_leader_transfer_in` | `bool` | 空值省略 |
| PauseLeaderTransferOut | `pause_leader_transfer_out` | `bool` | 空值省略 |

**StoreInfo（任务 66）**

| Go 字段 | JSON key | 类型 | 存在规则 |
| --- | --- | --- | --- |
| Store | `store` | `*MetaStore` | 必有 |
| Status | `status` | `*StoreStatus` | 必有 |

**StoresInfo（任务 66）**

| Go 字段 | JSON key | 类型 | 存在规则 |
| --- | --- | --- | --- |
| Count | `count` | `int` | 必有 |
| Stores | `stores` | `[]*StoreInfo` | 必有 |

**嵌入字段（任务 63、66）**

| 类别 | 当前真实行为 | 迁移责任与覆盖边界 |
| --- | --- | --- |
| response 取消、写入错误 | 两个 Marshal 函数返回 ctx.Err 或 out.Error；response 不定义 HTTP 状态 | 64/65 返回可区分错误；宿主决定状态 |
| 参数非法、对象缺失 | server/api/region.go GetRegionByID：ParseUint/零 ID →400，缺失 →404，序列化错误（含取消）→500，成功→200 Data；server/api/store.go Get：解析失败走 ErrorResp InvalidInput，缺失→404，成功→200 JSON | 既有 Go 路由事实，不能一概把取消改写成 408/499；网络迁移不在本计划 |
| 微服务序列化错误 | pkg/mcs/scheduling/server/apis/v1/api.go 使用 500 String 错误，成功 200 Data | 宿主适配，63–67 不修改服务 |
| 领域错误 | core/region_stale.rs 的 StaleRegionError、RegionValidationError::{NotFound,InvalidEpoch}；core/stores_control.rs 的 StoreControlError::{StoreNotFound,SlowTrendEvicted} | core 只保留领域数据/错误；不是 response DTO，也无已实现的通用 HTTP 映射 |
| protobuf 输入缺失 | server/core_region_adapter.rs region_from_heartbeat 返回 missing region / missing down peer 字符串错误 | 61 已有窄适配；不是 HTTP 404，也未接入 region 路由 |
| Rust 现有网络服务 | server/http.rs 仅 health/members，健康失败503；grpc.rs 的 failed_precondition/unavailable/unimplemented/unknown 对应现有身份、ID、TSO 路径 | 不宣称已有 Region/Store HTTP 或领域错误到 gRPC 的完整映射 |


## 检查与仍需保留的证据边界

- 运行声明及调用方 rg 扫描、相对基准只读 Go diff、Markdown 路径核验和 git diff --check。
- Rust 依赖修复使本次需要 Cargo；先原子领取共享槽位，所有命令设置绝对 CARGO_TARGET_DIR，
  无 Go failpoint 注入。实际槽位/命令/结果见最终验证段。
- 表中历史 Go oracle 路径仅在历史提交中存在，不能恢复被用户要求删除的 Go 测试。
- 当前测试通过证明当前行为；原始会话中的前序红绿证据另列于文末，不从 Git 反推
  执行结果。完整来源主题仍由 69 验收。

## 前序红绿运行追溯

已只读核对 64 个原任务会话的命令执行项及完成记录，并与当前模块/测试/提交交叉检查。
日志收集外壳可能退出 0；下表区分外壳结果和 Cargo 错误，不将外壳成功解释为测试成功。
历史会话中的 Go 测试属于当时证据；本审计未重跑它们。

| 任务 | 原会话 ID | 红灯依据 | 历史绿灯记录 |
| --- | --- | --- | --- |
| 2 | `01a163eb-3d0a-7943-bd80-c6e56eab6143` | error[E0432]: unresolved imports crate::REGION_INFLUENCE, crate::SMALL_REGION_INFLUENCE, crate::SMALL_REGION_THRESHOLD, crate::STORE_LIMIT_TYPE_LEN, crate::TYPE_NAME_VALUE, crate::Type, crate::UNLIMITED, crate::VERSION_V1, crate::VERSION_V2, crate::type_from_name；收集命令退出 101 | / make gotest GOTEST_ARGS='./pkg/core/storelimit -run "^(TestStoreLimit.*\/TestRustCoreTask02Oracle)$$" -count=1 -v' / 0；2 项通过 / / make basic-test PACKAGES=github.com/tikv/pd/pkg/core/sto |
| 3 | `01a163f2-46d6-7852-b877-1b656c965d24` | error[E0432]: unresolved import crate::StoreRateLimit；收集命令退出 101 | - Go oracle：两个共享场景通过；make basic-test BASIC_TEST_PKGS='./pkg/core/storelimit' 通过。 - cargo metadata --format-version 1 --no-deps、cargo fmt --all -- --check、cargo check --workspace --loc |
| 4 | `01a163fa-7baa-7532-935f-443f978813ed` | error[E0599]: no method named reset found for struct StoreRateLimit in the current scope；收集命令退出 101 | - cargo test --locked -p astersql-pd-storelimit --lib core_task_04 -- --nocapture：8 项通过。 - cargo test --locked -p astersql-pd-storelimit --lib：15 项通过。 - cargo test --workspace --locked |
| 5 | `01a16401-0ea4-7bc1-a1eb-aebaef3bb69f` | error[E0432]: unresolved import crate::Window；收集命令退出 0 | - cargo test --locked -p astersql-pd-storelimit --lib：退出 0，18 项通过。 - cargo test --workspace --locked：退出 0，252 项通过、1 项忽略。 |
| 6 | `01a16405-b8ab-7b52-82ee-4fda06a9d034` | error[E0432]: unresolved import crate::SlidingWindows；收集命令退出 0 | - 绿灯：同一命令通过 1 个测试；crate 测试通过 19 个。Go 指定命令通过 2 个测试、14 步场景。 - cargo metadata --format-version 1 --no-deps、cargo fmt --all -- --check、cargo check --workspace --locked、cargo test --worksp |
| 7 | `01a1640a-a929-7121-96fa-545d7dbc7316` | error[E0599]: no method named feedback found for struct SlidingWindows in the current scope；收集命令退出 101 | - 同命令绿灯：退出 0，2 个测试通过；crate 全测 21 个通过。 - make gotest GOTEST_ARGS='./pkg/core/storelimit -run "^(TestFeedback.*/TestRustCoreTask07Oracle)$$" -count=1 -v'：退出 0，2 个测试通过，failpoints 已禁用。 - carg |
| 9 | `01a16416-4e9b-7f70-98de-3855b15dcc14` | error[E0432]: unresolved imports super::Peer, super::PeerRole, super::count_in_joint_state, super::is_in_joint_state；收集命令退出 101 |  |
| 10 | `01a1641e-924c-7082-923b-d8291d5ece85` | error[E0583]: file not found for module region_meta；收集命令退出 101 | - cargo test --locked -p astersql-pd-core --lib：退出 0，7 项通过。 - go test -mod=readonly ./pkg/core -run '^(TestRegionKey.*)$' -count=1 -v 及 TestRegionItem：各退出 0，各 1 项通过。 - cargo metadata  |
| 11 | `01a16422-ea67-7e72-9c6d-cfb1e7d306c4` | error[E0583]: file not found for module store_meta；收集命令退出 101 | - go test ./pkg/core -run '^(TestNewStore.*)$' -count=1 -v：退出 0，1 项通过。 - cargo test --locked -p astersql-pd-core --lib：退出 0，9 项通过。 - cargo test --workspace --locked：退出 0，264 项通过、已有 1 项 |
| 12 | `01a16426-50b2-7b62-889b-251668232440` | method is_up not found for this struct；收集命令退出 0 | - metadata、fmt、workspace check/test 均退出 0；全量 268 通过，1 个既有进程兼容测试忽略。 - 已有 Go TestNewStore 通过；状态预期来自只读源码，未宣称运行差分。Go 文件及模块配置未改动。 |
| 13 | `01a1642a-5d39-7613-bb12-2bfb044eadb1` | method labels not found for this struct；收集命令退出 0 | - go test ./pkg/core -run '^(TestStoreInfoIsTiFlash.*)$' -count=1 -v：9 个子场景通过。 - cargo test --locked -p astersql-pd-core --lib core_task_13 -- --nocapture：5 通过。 - cargo test --locked -p |
| 14 | `01a1642f-0d3b-7a62-9fd3-f65c16340a91` | error[E0432]: unresolved imports super::DFSStats, super::StoreStats；收集命令退出 101 | - 绿灯：聚焦 2 项、core 20 项、workspace 275 项通过（1 项忽略）；已有 Go 测试 1 项通过。 |
| 15 | `01a16432-3da6-74a1-a6a7-41b552ff7af6` | - 红灯：聚焦 Rust 命令退出 101，缺少模块与 API。；原会话完成记录 | - go test -mod=readonly ./pkg/core -run '^(TestStoreStats.*)$' -count=1 -v：退出 0，1 个通过。 - cargo test --locked -p astersql-pd-core --lib core_task_15 -- --nocapture：退出 0，4 个通过。 - cargo te |
| 16 | `01a16435-32a1-7ee2-8dbd-46cc4feaebe0` | error[E0599]: no method named clone found for struct store_meta::StoreInfo in the current scope；收集命令退出 0 | - Go：go test ./pkg/core -run '^(TestCloneMetaStore.*)$' -count=1 -v，退出码 0，1 个通过。 - Rust：聚焦测试先因缺失 API 返回 101；实现后 5 个通过，core 全量 29 个通过。 - cargo metadata --format-version 1 --no-deps、cargo |
| 17 | `01a1643a-1a51-7542-8377-220e5a5ea549` | error[E0599]: no method named set_last_heartbeat_unix_nanos found for struct store_meta::StoreInfo in the current scope；收集命令退出 101 | - cargo test --locked -p astersql-pd-core --lib core_task_17 -- --nocapture：2 通过，覆盖 20 个 fixture 场景。 - cargo test --locked -p astersql-pd-core --lib：31 通过。 - cargo test --workspace --lo |
| 18 | `01a1643e-8625-7833-9773-cee793989b86` | method pause_leader_transfer not found for this struct；收集命令退出 0 | - cargo test --locked -p astersql-pd-core --lib core_task_18 -- --nocapture：3 个通过，覆盖 15 组场景。 - cargo test --locked -p astersql-pd-core --lib：34 个通过。 - cargo test --workspace --locked：2 |
| 19 | `01a16442-2350-76b0-8aaf-6a25eaa11f38` | method is_available not found for this struct；收集命令退出 0 | - cargo test --locked -p astersql-pd-core --lib core_task_19 -- --nocapture：3 通过。 - cargo test --locked -p astersql-pd-core --lib：37 通过。 - cargo test --workspace --locked：292 通过、1 个已有忽 |
| 20 | `01a16449-a671-7473-a665-0e7559f8f3e7` | method leader_score not found for this struct；收集命令退出 0 | - Go：go test ./pkg/core -run '^(TestRegionScore.*)$' -count=1 -v，退出 0，1 个测试通过。 - Rust：任务测试 3 个、core 全部 40 个通过；workspace 295 个通过，1 个既有 ignore。 |
| 21 | `01a1644c-9a0f-7772-96bb-2fad1914093e` | method region_score_v1 not found for this struct；收集命令退出 0 | - go test ./pkg/core -run '^(TestRegionScore.*)$' -count=1 -v：1 个通过。 - cargo test --locked -p astersql-pd-core --lib：42 个通过。 - workspace metadata、fmt、check、test：全部通过；297 个测试通过，1 个既有 igno |
| 22 | `01a1644f-0f58-7a13-920e-eb252232e26a` | method region_score not found for this struct；收集命令退出 0 | - 绿灯：聚焦测试 3 个、core 45 个、工作区 300 个通过，另有 1 个既有 ignored。metadata、fmt、workspace check/test 均退出 0。 - Go 只读测试 TestLowSpaceScoreV2 通过；数值 fixture 来自源码推导，不宣称运行差分。 |
| 23 | `01a16454-6f53-79e0-a4b9-47134f4e6f42` | method min_resolved_ts not found for this struct；收集命令退出 0 | - go test -mod=readonly ./pkg/core -run '^(TestLowSpaceRatio.*)$' -count=1 -v：1 项通过。 - cargo test --locked -p astersql-pd-core --lib：48 项通过。 - cargo test --workspace --locked：303 项通过，1 |
| 24 | `01a16457-114c-75f3-8773-30c8c9048b43` | test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 48 filtered out; finished in 0.00s；收集命令退出 101 | / go test -mod=readonly ./pkg/core -run '^(TestDistinctScore.*)$' -count=1 -v / 0；1 个测试通过 / / cargo test --locked -p astersql-pd-core --lib / 0；51 通过 / / cargo test --workspace --locked |
| 25 | `01a16459-acb3-71c1-bda7-a8c4cbf7f6e1` | error[E0432]: unresolved import crate::get_store_topo_weight；收集命令退出 0 | - 绿灯：cargo test --locked -p astersql-pd-core --lib core_task_25 -- --nocapture：1 个测试、9 个场景；core 全量 52 个测试通过，均退出 0。 - cargo metadata --format-version 1 --no-deps、cargo fmt --all -- --che |
| 26 | `01a1645c-7189-7723-8378-d66453554ebe` | could not compile astersql-pd-core (lib test) due to 1 previous error; 1 warning emitted；收集命令退出 0 | - go test ./pkg/core -run '^(TestPutStore.*)$' -count=1 -v：退出 0，2 项通过。 - cargo test --locked -p astersql-pd-core --lib：退出 0，55 项通过。 - cargo test --workspace --locked：退出 0，310 项通过，1 项既有 |
| 27 | `01a16460-2cc2-75e3-a6ff-8464e723ac4d` | error[E0432]: unresolved import crate::StoreControlError；收集命令退出 101 | - go test ./pkg/core -run '^(TestPutStore.*)$' -count=1 -v：2 个通过。 - cargo test --locked -p astersql-pd-core --lib core_task_27 -- --nocapture：3 个通过。 - cargo test --locked -p astersql-pd |
| 28 | `01a16463-e613-7a83-9904-e96f5deaef37` | could not compile astersql-pd-core (lib test) due to 8 previous errors; 1 warning emitted；收集命令退出 0 | - 绿灯：聚焦命令 cargo test --locked -p astersql-pd-core --lib core_task_28 -- --nocapture 通过 3 项；core 全测通过 61 项；workspace 全测通过 316 项，1 项既有忽略。metadata、fmt、workspace check、diff 检查均退出 0。 - 只读 Go 测试 |
| 29 | `01a16467-1249-7530-8eff-16f456e92449` | error[E0433]: cannot find type RegionOption in this scope；收集命令退出 0 | - 只读 Go 测试 TestSortedEqual、TestRegionKey **2 通过**；其余边界依据源码推导，未宣称运行差分。 |
| 30 | `01a1646a-bb7f-7242-9c7b-4436603429be` | error[E0583]: file not found for module region_flow；收集命令退出 0 | - cargo metadata、fmt、workspace check、git diff --check 均通过。 - 已有 Go 测试 2 项通过；Go 文件未改动。新增边界依据源码推导，不宣称运行差分。 |
| 31 | `01a1646e-ccb4-7f73-a843-0733742c4975` | error[E0599]: no method named write_query_num found for struct region::RegionInfo in the current scope；收集命令退出 0 | - 已有 Go TestRegionWriteRate 1/1 通过；其他边界依据 Go 源码推导，未宣称运行差分。 |
| 32 | `01a16472-12ae-7e53-be62-643e3a21b217` | error[E0425]: cannot find function is_empty_region in this scope；收集命令退出 0 | - Go 已有测试 2 个、core 测试 70 个、workspace 测试 325 个全部通过；metadata、fmt、check 均退出 0。 |
| 33 | `01a16474-e61c-7de3-a215-141b100b01d4` | method inherit not found for this struct；收集命令退出 0 | - go test ./pkg/core -run '^(TestInherit/TestGetBucketMeta)$' -count=1 -v：2 项通过。 - cargo test --locked -p astersql-pd-core --lib core_task_33 -- --nocapture：3 项通过。 - cargo test --locked |
| 34 | `01a16477-e996-7d92-a413-89e4873322a5` | error[E0599]: no method named compare_and_set_report_buckets found for struct std::sync::MutexGuard<'_, region::RegionInfo> in the current scope；收集命令退出 0 | - go test ./pkg/core -run '^TestGetBucketMeta.*$' -count=1 -v：1 passed。 - cargo test --locked -p astersql-pd-core --lib：78 passed。 - cargo test --workspace --locked：333 passed，1 ignore |
| 35 | `01a1647b-ace9-7021-85d1-e60697a3a4f4` | error[E0422]: cannot find struct, variant or union type RegionGuide in this scope；收集命令退出 101 | - workspace 335 项通过、已有忽略 1 项；metadata、fmt、check、diff 检查均退出 0。 - 只读 Go 测试通过：TestNeedSync 10 个场景、逻辑存储量 3 个子测试；其余边界依据源码推导。 |
| 36 | `01a1647e-fe78-7e82-90de-6963f8df4fa9` | error[E0425]: cannot find function diff_region_peers_info in this scope；收集命令退出 0 | - 已有 Go 测试 TestCodecRule、TestRegionKey 共 2 个通过；其余边界依据源码推导，未宣称运行差分。 |
| 37 | `01a16483-0e10-7b53-8357-017bf11f026a` | test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 84 filtered out; finished in 0.00s；收集命令退出 0 | - Go：go test -mod=readonly ./pkg/core -run '^(TestRegionTreeSplitAndMerge/TestRegionKey)$' -count=1 -v，退出 0，2 项通过。 - cargo test --locked -p astersql-pd-core --lib：退出 0，87 项通过。 - cargo t |
| 38 | `01a16485-e8e8-7c81-a17c-d54de41b2784` | test result: FAILED. 0 passed; 4 failed; 0 ignored; 0 measured; 87 filtered out; finished in 0.00s；收集命令退出 0 | - 已有 Go 测试 2 项通过；边界预期另有源码推导，未宣称完整跨语言运行差分。Go 文件及模块配置未改动。 |
| 39 | `01a16489-6845-7b92-87e2-225f95037f48` | error[E0433]: cannot find type RegionTree in this scope；收集命令退出 0 | cargo metadata --format-version 1 --no-deps、cargo fmt --all -- --check、cargo check --workspace --locked、git diff --check 均通过。源码推导场景未宣称完成跨语言运行差分。 |
| 40 | `01a1648c-23ca-7fd1-941f-7db423c666c1` | method update not found for this struct；收集命令退出 0 | - go test ./pkg/core -run '^TestRegionTreeSplitAndMerge.*$' -count=1 -v：1 个通过。额外边界依据 Go 源码推导，未宣称运行差分。 |
| 41 | `01a1648f-a1b5-7492-8de7-692f76867fd3` | method not_from_storage_regions_count not found for this struct；收集命令退出 0 | - go test ./pkg/core -run '^(TestRegionTreeStat/TestRegionTreeMerge/TestCntRefAfterFlowOnlyUpdate)$' -count=1 -v：3 个通过。 - cargo test --locked -p astersql-pd-core --lib core_task_41 -- --n |
| 42 | `01a16493-c9b0-71a1-8d8a-de9815d520bd` | error[E0599]: no method named get_count_by_range found for struct region_tree::RegionTree in the current scope；收集命令退出 101 | - go test -mod=readonly ./pkg/core -run '^(TestRegionTree.*/TestStoreRegionCount)$' -count=1 -v：5 项通过。 - cargo metadata --format-version 1 --no-deps、cargo fmt --all -- --check、cargo c |
| 43 | `01a16496-ff1c-7841-8a36-1a38b35eaa42` | method random_regions not found for this struct；收集命令退出 0 | - go test ./pkg/core -run '^(TestRandomRegion.*)$' -count=1 -v：2 项通过。 - cargo test --locked -p astersql-pd-core --lib core_task_43 -- --nocapture、core 全测、workspace 全测均退出 0；workspace 364  |
| 44 | `01a1649a-2717-71a2-9e40-834ce37fed35` | error[E0433]: cannot find type RegionsInfo in this scope；收集命令退出 0 | - Go：TestRegionMap、TestSetRegion，2 项通过。 - Rust 聚焦 2 项、core 111 项通过；workspace 366 项通过、1 项既有忽略。 |
| 45 | `01a1649c-ef91-7663-a6b9-459ccbe66d65` | error[E0433]: cannot find type RegionValidationError in this scope；收集命令退出 0 | - Go 已有测试：2 个通过。 - Rust 聚焦：3 个通过，含 16 个 fixture 场景。 - core 全量：114 个通过。 - workspace：369 个通过，1 个既有忽略项。 - metadata、fmt、workspace check、diff check 通过。 |
| 46 | `01a164a2-1942-7bf0-ac2a-a4217ef9e323` | could not compile astersql-pd-core (lib test) due to 7 previous errors；收集命令退出 0 | - cargo test --locked -p astersql-pd-core --lib core_task_46 -- --nocapture：3 个通过。 - cargo test --locked -p astersql-pd-core --lib：117 个通过。 - workspace 默认测试因端口占用失败；cargo test --workspac |
| 47 | `01a164a5-7c1a-7a11-8640-c87991e1e341` | 红灯为缺少子树类型/API，退出 101。扩展场景依据只读 Go 源码推导，未声称运行差分。Go 文件和模块配置未改动。；原会话完成记录 | - cargo test --workspace --locked：376 项通过，1 项既有忽略。 - Cargo metadata、fmt 检查、workspace check、git diff --check 均通过。 |
| 48 | `01a164a8-6ffe-76a0-aaf1-08a426f572c2` | error[E0063]: missing fields flashback_start_ts and is_in_flashback in initializer of region_meta::RegionMeta；收集命令退出 101 | - 绿灯：cargo test --locked -p astersql-pd-core --lib core_task_48 -- --nocapture：3 通过；core 全量：124 通过。 - cargo metadata --format-version 1 --no-deps、cargo fmt --all -- --check、cargo chec |
| 49 | `01a164ab-8eb0-7851-bbe8-2c65c6b5e8d1` | method reset_region_cache not found for this struct；收集命令退出 0 | - 绿灯：聚焦测试 3 项、core 127 项、workspace 382 项通过（已有 1 项忽略）；metadata、fmt、check 均退出 0。 - 只读 Go 测试 3 项通过；未修改 Go 文件或模块配置。源码推导边界已在 fixture 说明，不冒充运行差分。 |
| 50 | `01a164ae-11f1-7853-975b-bfce60d4bb3d` | test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 127 filtered out; finished in 0.00s；收集命令退出 0 | - go test -mod=readonly ./pkg/core -run '^(TestQueryRegions.*)$' -count=1 -v：退出 0，1 个用例通过。 - cargo test --locked -p astersql-pd-core --lib：退出 0，131 个通过。 - cargo test --workspace --locke |
| 51 | `01a164b1-e0cb-7373-8463-8aae67a7ccfc` | method scan_regions not found for this struct；收集命令退出 0 | - go test ./pkg/core -run '^(TestScanRegion.*)$' -count=1 -v：2 项通过。 - cargo test --locked -p astersql-pd-core --lib core_task_51 -- --nocapture：3 项通过，含 13 个 fixture 场景。 - cargo test --l |
| 52 | `01a164b4-71c9-7032-b8e8-535e502c959a` | method batch_scan_regions not found for this struct；收集命令退出 101 | - 绿灯：聚焦测试 2 个通过，覆盖 17 个场景及限制 1–18；core 全量 136 个通过。 - go test ./pkg/core -run '^TestScanRegion(Limit)?$' -count=1 -v：2 个通过。 - cargo metadata --format-version 1 --no-deps、cargo fmt --all  |
| 53 | `01a164b7-94a3-70b1-9d99-e89cfb961a5f` | method store_region_count not found for this struct；收集命令退出 0 | - go test ./pkg/core -run '^(TestStoreRegionCount.*)$' -count=1 -v：1 项通过。 - cargo test --locked -p astersql-pd-core --lib：138 项通过。 - cargo test --workspace --locked：393 项通过，1 项既有忽略。 -  |
| 54 | `01a164ba-daa2-7c01-8cf1-60b71f12e362` | error[E0599]: no method named get_region_size_by_range found for struct regions::RegionsInfo in the current scope；收集命令退出 0 | - 已有 Go 测试 1/1 通过；工作区 397 项通过，1 项既有测试忽略。 |
| 55 | `01a164be-1827-7e53-af62-ef6647578e6f` | error[E0425]: cannot find function need_transfer_witness_leader in this scope；收集命令退出 101 | - 绿灯：任务 4 项、core 146 项、workspace 401 项通过（既有 1 项 ignored）。 - Go 已有随机测试 2 项通过；角色切换和 witness 场景依据只读源码，未宣称运行差分。 |
| 56 | `01a164c1-5cec-7703-bf75-c74c03c93650` | method get_non_witness_voter_stores not found for this struct；收集命令退出 0 | - Go：go test -mod=readonly ./pkg/core -run '^(TestRegionInfo.*)$' -count=1 -v，1 项通过。 - cargo test --locked -p astersql-pd-core --lib core_task_56 -- --nocapture：2 项通过，覆盖 7 组 fixture 场景。  |
| 57 | `01a164c5-100f-7a92-b92e-ee4409631ead` | error[E0433]: cannot find type BasicCluster in this scope；收集命令退出 0 | - 已有 Go 测试 TestStoreRegionCount 1/1 通过；额外分支依据只读 Go 源码验证，未声称运行差分。 |
| 58 | `01a164c7-c4ad-7a22-91ba-9a9835b29e34` | error[E0599]: no method named get_stores_leader_write_rate found for reference &basic_cluster::BasicCluster in the current scope；收集命令退出 0 | - Go：go test ./pkg/core -run '^(TestRegionWriteRate.*)$' -count=1 -v，退出 0，1 个通过。 - core 全量：153 个通过。 - workspace：408 个通过、1 个已有忽略；metadata、fmt check、check、test 均退出 0。 - git diff --check 通过 |
| 59 | `01a164ca-7758-7d80-b5ec-8ecd8e944941` | error[E0583]: file not found for module core_trace；收集命令退出 0 | - go test ./pkg/core -run '^(TestRegionHeartbeatTracer.*)$' -count=1 -v：**2 项通过**，退出 0。 - cargo test --workspace --locked：**411 项通过，1 项既有忽略**，退出 0。metadata、fmt check、workspace check、diff |
| 60 | `01a164ce-876d-7791-ade4-361191d298ba` | error[E0425]: cannot find type QueryStats in this scope；收集命令退出 101 |  |
| 61 | `01a164d3-d763-70e0-9f97-a59689772996` | test result: FAILED. 0 passed; 4 failed; 0 ig；收集命令退出 101 | - Go：go test ./pkg/core -run '^TestRegionFromHeartbeatIAKVSize.*$' -count=1 -v，1 个通过。 - server：cargo test --locked -p astersql-pd-server --lib，12 个通过。 - workspace：cargo test --workspace |
| 63 | `01a164db-d1f4-76f0-bb4a-649af1b450d9` | error[E0433]: cannot find type PDPeerStats in this scope；收集命令退出 0 | - Go：go test ./pkg/response -run '^(TestPeer.*)$' -count=1 -v，退出码 0，2 项通过。 - workspace metadata、fmt、check、test 均退出码 0；全量测试 423 项通过、1 项既有忽略。 |
| 64 | `01a164de-8477-7c31-b7d8-33d24971a308` | error[E0425]: cannot find function new_api_region_info in this scope；收集命令退出 0 | - 现有 Go 测试：1 个通过。 - 本任务 Rust 测试：4 个通过；response 全部 8 个通过。 - workspace：428 个通过，1 个既有 ignored。 - metadata、fmt、check、diff 检查通过。 |
| 65 | `01a164e2-d4b6-79c3-9f29-3099eb121fc0` | error[E0583]: file not found for module regions；收集命令退出 101 | - 验证均退出 0：聚焦测试 3 个、response 11 个、workspace 431 passed（1 个既有 ignored）；Go 既有测试 1 个。 |
| 66 | `01a164e5-411f-7b33-84ba-8b5f99cb9cbd` | error[E0583]: file not found for module store；收集命令退出 101 | - 验证均退出 0：Go 1 个、任务 Rust 5 个、response 16 个、workspace 436 个通过；1 个既有 ignored。 |
| 67 | `01a164ea-755b-77b3-b625-fc3796bd7997` | test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 16 filtered out; finished in 0.00s；收集命令退出 0 | - 绿灯：聚焦 2 个、response 18 个、workspace 438 个通过（既有忽略 1 个），均退出 0。 - Go 已有测试通过 1 个；Store 状态期望来自只读源码推导，未声称运行差分。 |


## 生成代码声明补表

`pkg/response/region_easyjson.go` 的 12 个内部函数按当前完整名称补列；
其行为归 63–65 的 serde DTO/序列化测试，生成器和函数形状范围外。

| 内部函数 | 当前 Rust 行为及责任 |
| --- | --- |
| `easyjson75d7afa0DecodeGithubComTikvPdPkgResponse` | response peer/region/regions 自有 DTO、serde 编解码；63–65 |
| `easyjson75d7afa0EncodeGithubComTikvPdPkgResponse` | response peer/region/regions 自有 DTO、serde 编解码；63–65 |
| `easyjson75d7afa0DecodeGithubComTikvPdPkgResponse1` | response peer/region/regions 自有 DTO、serde 编解码；63–65 |
| `easyjson75d7afa0EncodeGithubComTikvPdPkgResponse1` | response peer/region/regions 自有 DTO、serde 编解码；63–65 |
| `easyjson75d7afa0DecodeGithubComTikvPdPkgResponse2` | response peer/region/regions 自有 DTO、serde 编解码；63–65 |
| `easyjson75d7afa0EncodeGithubComTikvPdPkgResponse2` | response peer/region/regions 自有 DTO、serde 编解码；63–65 |
| `easyjson75d7afa0DecodeGithubComPingcapKvprotoPkgMetapb` | response peer/region/regions 自有 DTO、serde 编解码；63–65 |
| `easyjson75d7afa0EncodeGithubComPingcapKvprotoPkgMetapb` | response peer/region/regions 自有 DTO、serde 编解码；63–65 |
| `easyjson75d7afa0DecodeGithubComTikvPdPkgResponse3` | response peer/region/regions 自有 DTO、serde 编解码；63–65 |
| `easyjson75d7afa0EncodeGithubComTikvPdPkgResponse3` | response peer/region/regions 自有 DTO、serde 编解码；63–65 |
| `easyjson75d7afa0DecodeGithubComTikvPdPkgResponse4` | response peer/region/regions 自有 DTO、serde 编解码；63–65 |
| `easyjson75d7afa0EncodeGithubComTikvPdPkgResponse4` | response peer/region/regions 自有 DTO、serde 编解码；63–65 |

本次对三个目录生产文件的 func/type 声明扫描共 591 项（含重复方法名），全部进入文本索引；
这只是防遗漏检查，行为覆盖仍由上表的 Go 分支、Rust 符号、测试及红绿记录决定。

## 最终验证（本次运行）

依赖缺口已逐个按失败→实现→通过补齐。本任务的文档审计因此扩展到必要的 Rust
依赖修复；每项独立验证，Go 源码/测试/模块始终只读，未创建或恢复 Go oracle。

- 共享槽位：主要为 1，前一阶段 workspace 尚运行时聚焦阶段领取 2；各阶段原子 mkdir
  领取并由持有进程 trap 释放。`CARGO_TARGET_DIR` 分别为仓库绝对路径下
  `target/rust-slot-1` 和 `target/rust-slot-2`；最终整套检查在槽位 1。缓存保留。
- 聚焦红灯：缺 API/选项 E0432/E0599 或组合写入角色计数 `0 != 1`，Cargo 退出 101；
  各次实现后聚焦通过。新增有效测试共 8 项（core 7、response 1），无忽略。
- `cargo test --locked -p astersql-pd-core --lib`：退出 0，160 通过，0 失败/忽略。
- `cargo test --locked -p astersql-pd-response --lib`：退出 0，19 通过，0 失败/忽略。
- `cargo metadata --format-version 1 --no-deps`：退出 0；core 和 storelimit 生产依赖没有
  proto/transport/storage；response 仅依赖 core、constant、typeutil、serde/serde_json。
- `cargo fmt --all`、`cargo fmt --all -- --check`、`cargo check --workspace --locked`：退出 0。
  仅出现既有 `core_region_adapter.rs` 读取弃用 cpu_usage 协议字段的 warning。
- `cargo test --workspace --locked`：退出 0，446 通过、0 失败、1 个既有 ignored，
  76 个测试结果目标（包括 0 测试的 doc-test）。ignored 为要求另行过程测试的
  `tso_processes`，不作为本次有效覆盖。
- `rg -n '^func |^type |failpoint\.' pkg/core pkg/core/storelimit pkg/response -g '*.go'`：退出 0。
  WitnessScore、peer 统计、组合写入和 JSON 取消调用方均使用 rg/只读源码交叉核验。
- Python 591 项 Go 生产 func/type 名称索引、64 个前序会话记录、Rust 模块注册及
  Markdown 相对文件路径核验：通过；生成 helper 以全名补表，原失效 oracle 链接已改为
  明确历史提交说明。路径存在性不被解释为行为通过。
- `git diff --check`：退出 0；Go 文件和模块配置工作区 diff 无输出；plan.md 不修改。

本审计完成公开能力映射和必要依赖补齐；不关闭来源史诗，不声称 Rust Region/Store
网络路由、完整调度、全部指标宿主装配或全局 Runner 已迁移。任务 69 仍须执行最终回归。
