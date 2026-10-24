# 配置 provider 与宿主注册集合

来源：`config_provider.go` 与 `server/config/persist_options.go`，基准
`db1b56605b43b973afbd1eef8d03a7e7ba25e0bf`。真实运行证据见
[任务 11](../../../docs/rust/schedule/config/11-证据.md)。

`ProviderSnapshot<V>` 拥有一个配置代际的 schedule、replication、store、label_properties、
cluster_version、key_type、flow_round_by_digit。宿主先捕获完整代际，再传递共享借用；
四类视图均读同一个对象。`V` 由宿主提供，允许复用 server/config 的 ClusterVersion，
没有领域层到 server 的反向依赖。Clone 隔离下一代值，借用期间 Rust 禁止修改该代际。

公开契约为 `SharedConfigProvider`、`SchedulerConfigProvider`、`CheckerConfigProvider`、
`StoreConfigProvider`、组合 `ConfProvider`。这里是不可变快照读取契约；Go 混在接口内的
发布、Set/Test setters、Persist、Add/RemoveSchedulerCfg、TTL 和日志状态更新属于后续
server/config 编排，不在领域只读视图中提供伪持久化或空实现。
`get_store_limit_by_type` 返回该代际显式 store rate 或默认值，读取不会建立新 store entry；
这对应 Go PeekStoreLimitByType 的无副作用值语义，Go GetStoreLimit 的 create-on-miss
发布副作用需由后续宿主更新流程承担。未知 rate 类型和 Go 一样 panic；SendSnapshot=0。

标签使用 core::StoreLabel，key/value 精确匹配，规则间 OR；空/未知属性 false，
没有 Unicode 或大小写折叠。`REJECT_LEADER` 为 reject-leader。
Scheduler 类型复用 canonical CheckerSchedulerType 和兼容映射；禁用查询取首个旧名称
匹配项，未知类型按 Go map 零值映射为空字符串。`SchedulerRegistry` 是宿主持有的
BTreeSet，只存标识，无全局状态、工厂或未迁移调度器桩；register 幂等，registered
为 Rust 确定性枚举扩展，Go sync.Map 无顺序保证。

Store getter 和尺寸/keys 检查直接委托批次 10 的真实 StoreConfig；merge keys=0
时回退 size×10000（uint64 wrapping），显式 keys 优先；affinity 的零值保持禁用。

## 方法清单

- `register`
- `contains`
- `registered`
- `default`
- `snapshot`
- `get_cluster_version`
- `get_key_type`
- `get_location_labels`
- `check_label_property`
- `get_store_limit_by_type`
- `get_max_replicas`
- `is_placement_rules_enabled`
- `is_placement_rules_cache_enabled`
- `get_affinity_schedule_limit`
- `get_split_scatter_schedule_limit`
- `get_max_snapshot_count`
- `get_max_pending_peer_count`
- `get_low_space_ratio`
- `get_high_space_ratio`
- `get_max_store_down_time`
- `is_use_joint_consensus`
- `is_cross_table_merge_enabled`
- `is_one_way_merge_enabled`
- `get_merge_schedule_limit`
- `get_region_score_formula_version`
- `get_scheduler_max_waiting_operator`
- `is_witness_allowed`
- `get_hot_region_cache_hits_threshold`
- `get_region_schedule_limit`
- `get_max_affinity_merge_region_size`
- `is_scheduler_disabled`
- `get_schedule_config`
- `get_stores_limit`
- `get_leader_schedule_policy`
- `is_trace_region_flow`
- `get_leader_schedule_limit`
- `get_hot_region_schedule_limit`
- `get_witness_schedule_limit`
- `get_max_movable_hot_peer_size`
- `get_tolerant_size_ratio`
- `is_debug_metrics_enabled`
- `is_diagnostic_allowed`
- `get_slow_store_evicting_affected_store_ratio_threshold`
- `store_config`
- `get_region_max_size`
- `get_region_max_keys`
- `get_region_split_size`
- `get_region_split_keys`
- `is_enable_region_bucket`
- `is_raft_kv2`
- `get_unified_read_pool_max_thread_count`
- `check_region_size`
- `check_region_keys`
- `get_switch_witness_interval`
- `is_remove_extra_replica_enabled`
- `is_remove_down_replica_enabled`
- `is_replace_offline_replica_enabled`
- `is_make_up_replica_enabled`
- `is_location_replacement_enabled`
- `get_split_merge_interval`
- `get_patrol_region_interval`
- `get_patrol_region_worker_count`
- `get_max_merge_region_size`
- `get_max_merge_region_keys`
- `get_replica_schedule_limit`
- `get_isolation_level`
- `snapshot`
- `store_config`
