# 服务配置契约盘点证据

## 批次 83：盘点配置字段与外部依赖

日期：2026-10-24（Asia/Shanghai）。状态：**已完成（盘点任务）**。
规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223`；本轮起点
`803ca3a8b364410ef88d1a4adc4e951cec7d27c0`。三份指定 Go 来源相对规划基准
`git diff <基准> HEAD -- <三文件>` 输出为空。以下行号和哈希固定本轮源码；
事实清单经逐段阅读复核，不把符号搜索结果当作分支穷举。

已重新核验批次 82 提交 `803ca3a8b` 的实际实现、metadata 连线、五项独立测试
及 lib.rs 显式注册，并核对[配置值工具证据](theme3-配置值工具.md)任务 3：
聚焦红灯 1 passed / 4 failed、同组绿灯 5 passed / 0 failed，workspace
670 passed / 0 failed / 1 既有 ignored；提交为当前 HEAD，前置准入满足。
这些是复用的前序运行证据；本任务不重跑 Cargo，不声称新增运行差分。

只适用根 AGENTS.md；已读 Copilot 指南、ADR-001、依赖基线和移植指南。
范围仅三文件的字段、符号、Adjust/Validate 分支归属及边界；不实现生产代码。
只读总计划、来源主题、Go 源码和模块文件保持不变。

### 归属编号与规则

表中 T2–T16 指本服务配置契约计划的本地编号行为，均为**后续拟实施**，
不表示对应文件已经存在：T2 序列化/presence；T3 URL 值校验；T4 基础标量默认；
T5 TSO 归一化；T6 PDServer 默认；T7 GC 并发校验；T8 PDServer 其余校验；
T9 Keyspace；T10 ReplicationMode/DR；T11 Microservice；T12 Dashboard 纯值；
T13 Middleware；T14 废弃项；T15 外部嵌套装配；T16 全量差分回归。
U 表示来源主题 3 明确排除的 PersistOptions/TTL 热更新、启动/CLI/文件系统/
embedded etcd，须上层主题单独实施；E 表示 T15 的外部拥有包准入门。
T2 仅可先做本包基础标量，不得用透传 JSON 代表未就绪的完整嵌套 Config。

字段表保留 JSON tag 选项和 TOML tag 原文；`无显式 tag` 不能解释为忽略字段，
后续 decoder 须核对 Go 库对字段名/匿名嵌入的实际处理。JSON `-` 才表示忽略。
除特别说明外，缺失字段解码为 Go 零值，随后才执行默认逻辑；显式零值与缺失
是否相同由下述条件决定，不能一律预填默认再判断 presence。所有字段共享 T2/T16
序列化验收；表内额外归属负责行为。无默认表示本文件未调整，外部类型另见准入表。

### 来源文件指纹

| 文件 | 行数 | SHA-256 |
| --- | --- | --- |
| [config.go](../../server/config/config.go) | 1033 | `fe6565d6d88cebbec8171adea49bba688762be8b32b2884ed4220d3c4d6448f8` |
| [service_middleware_config.go](../../server/config/service_middleware_config.go) | 107 | `bf5fcd9ef17ac7810e84b2f2447c20178f6ff6c12dc7537e91a6f83d2d2399b1` |
| [persist_options.go](../../server/config/persist_options.go) | 1138 | `d49284b2eee7f9a509f32598ed69fe47bdccda674cb4940d80c2ac7ad10b62ff` |

### 逐字段覆盖清单

#### `server/config/config.go`

| 类型.字段（行） | Go 类型 | TOML | JSON | 缺失/零值/默认与归属 |
| --- | --- | --- | --- | --- | --- |
| Config.ClientUrls (51) | `string` | `client-urls` | `client-urls` | 空串默认 http://127.0.0.1:2379；T3/T4 |
| Config.PeerUrls (52) | `string` | `peer-urls` | `peer-urls` | 空串默认 http://127.0.0.1:2380；T3/T4 |
| Config.AdvertiseClientUrls (53) | `string` | `advertise-client-urls` | `advertise-client-urls` | 空串继承 ClientUrls；T3/T4 |
| Config.AdvertisePeerUrls (54) | `string` | `advertise-peer-urls` | `advertise-peer-urls` | 空串继承 PeerUrls；T3/T4 |
| Config.Name (56) | `string` | `name` | `name` | 空串取 pd-<宿主 hostname>，失败返回错误；宿主注入 U/T4 |
| Config.DataDir (57) | `string` | `data-dir` | `data-dir` | 空串取 default.<Name>，随后 AdjustPath；文件系统 U/T4 |
| Config.ForceNewCluster (58) | `bool` | `无显式 tag` | `force-new-cluster` | false 或输入；CLI/embedded etcd U |
| Config.EnableGRPCGateway (59) | `bool` | `无显式 tag` | `enable-grpc-gateway` | 缺失赋 true；显式 false 保留；T4 |
| Config.InitialCluster (61) | `string` | `initial-cluster` | `initial-cluster` | 空串按 AdvertisePeerUrls 逐项拼 Name=url；T4 |
| Config.InitialClusterState (62) | `string` | `initial-cluster-state` | `initial-cluster-state` | 空串取 new；T4 |
| Config.InitialClusterToken (63) | `string` | `initial-cluster-token` | `initial-cluster-token` | 空串取 pd-cluster；T4 |
| Config.Join (66) | `string` | `join` | `join` | 与 InitialCluster 互斥；非空逐项 parseUrls；T3 |
| Config.LeaderLease (72) | `int64` | `lease` | `lease` | 仅 0 取 5 秒，负值保留；T4 |
| Config.Log (75) | `log.Config` | `log` | `log` | adjustLog：缺失 disable-error-verbose=true；空 format=text/level=info；E/T15 |
| Config.LogFileDeprecated (78) | `string` | `log-file` | `log-file,omitempty` | 非空警告，log.file.filename 空时回填；T14/U |
| Config.LogLevelDeprecated (79) | `string` | `log-level` | `log-level,omitempty` | 非空警告，log.level 空时回填；T14/U |
| Config.MaxConcurrentTSOProxyStreamings (85) | `int` | `max-concurrent-tso-proxy-streamings` | `max-concurrent-tso-proxy-streamings` | 仅 0 取 5000；负数保留（无限）；T5 |
| Config.TSOProxyRecvFromClientTimeout (88) | `typeutil.Duration` | `tso-proxy-recv-from-client-timeout` | `tso-proxy-recv-from-client-timeout` | <=0 取 1h；T5 |
| Config.TSOSaveInterval (91) | `typeutil.Duration` | `tso-save-interval` | `tso-save-interval` | <=0 取固定 5s（不随自定义 lease 改变）；T5 |
| Config.TSOUpdatePhysicalInterval (98) | `typeutil.Duration` | `tso-update-physical-interval` | `tso-update-physical-interval` | <=0 先取 50ms，再钳位 [1ms,10s]；T5 |
| Config.EnableLocalTSO (101) | `bool` | `enable-local-tso` | `enable-local-tso` | Deprecated 注释；无本文件默认/迁移；T14 |
| Config.Metric (103) | `metricutil.MetricConfig` | `metric` | `metric` | interval<=0 取15s；job 空取 Name；address 保留；E/T15 |
| Config.Schedule (105) | `sc.ScheduleConfig` | `schedule` | `schedule` | 委托 schedule.Adjust(child,reloading)；E/T15 |
| Config.Replication (107) | `sc.ReplicationConfig` | `replication` | `replication` | 委托 replication.Adjust(child)；E/T15 |
| Config.PDServerCfg (109) | `PDServerConfig` | `pd-server` | `pd-server` | 嵌套值见对应字段表；T6/T7/T8 |
| Config.ClusterVersion (111) | `semver.Version` | `cluster-version` | `cluster-version` | 零 semver 或输入；semver 编码 E/T15 |
| Config.Labels (116) | `map[string]string` | `labels` | `labels` | nil 变空 map，其他保留；T4 |
| Config.QuotaBackendBytes (120) | `typeutil.ByteSize` | `quota-backend-bytes` | `quota-backend-bytes` | 缺失无条件赋 8GiB；显式 0 保留；T4 |
| Config.AutoCompactionMode (122) | `string` | `auto-compaction-mode` | `auto-compaction-mode` | 空取 periodic；T4 |
| Config.AutoCompactionRetention (129) | `string` | `auto-compaction-retention` | `auto-compaction-retention-v2` | 空取 1h；T4 |
| Config.TickInterval (132) | `typeutil.Duration` | `tick-interval` | `tick-interval` | <=0 取 500ms；T4 |
| Config.ElectionInterval (134) | `typeutil.Duration` | `election-interval` | `election-interval` | <=0 取 3000ms；T4 |
| Config.PreVote (139) | `bool` | `enable-prevote` | `enable-prevote` | 缺失赋 true；显式 false 保留；T4 |
| Config.MaxRequestBytes (141) | `uint` | `max-request-bytes` | `max-request-bytes` | 缺失无条件赋 150MiB；显式 0 保留；T4 |
| Config.Security (143) | `configutil.SecurityConfig` | `security` | `security` | 委托 Encryption.Adjust；TLS/redact 值契约另需拥有包；E/T15 |
| Config.LabelProperty (145) | `LabelPropertyConfig` | `label-property` | `label-property` | nil map 或输入；StoreLabel/Clone 值契约 T2/T15 |
| Config.WarningMsgs (148) | `[]string` | `无显式 tag` | `-` | CheckUndecoded/废弃项追加警告；不作 fatal；T14 |
| Config.DisableStrictReconfigCheck (150) | `bool` | `无显式 tag` | `-` | false 或输入；embedded etcd U |
| Config.HeartbeatStreamBindInterval (152) | `typeutil.Duration` | `无显式 tag` | `-` | <=0 取 1min，JSON 隐藏；T4/U |
| Config.LeaderPriorityCheckInterval (153) | `typeutil.Duration` | `无显式 tag` | `-` | <=0 取 1min，JSON 隐藏；T4/U |
| Config.Logger (155) | `*zap.Logger` | `无显式 tag` | `-` | nil 或宿主提供；U |
| Config.LogProps (156) | `*log.ZapProperties` | `无显式 tag` | `-` | nil 或宿主提供；U |
| Config.Dashboard (158) | `DashboardConfig` | `dashboard` | `dashboard` | 嵌套值见对应字段表；T12 |
| Config.ReplicationMode (160) | `ReplicationModeConfig` | `replication-mode` | `replication-mode` | 嵌套值见对应字段表；T10 |
| Config.Keyspace (162) | `KeyspaceConfig` | `keyspace` | `keyspace` | 嵌套值见对应字段表；T9 |
| Config.Microservice (164) | `MicroserviceConfig` | `micro-service` | `micro-service` | 嵌套值见对应字段表；T11 |
| Config.Controller (166) | `rm.ControllerConfig` | `controller` | `controller` | 委托 Controller.Adjust(child)；E/T15 |
| Config.Metering (168) | `config.MeteringConfig` | `metering` | `metering` | Config.Adjust 未处理；拥有包值与编码契约 E/T15 |
| PDServerConfig.UseRegionStorage (511) | `bool` | `use-region-storage` | `use-region-storage,string` | 缺失赋 true；T6/T8 |
| PDServerConfig.MaxResetTSGap (513) | `typeutil.Duration` | `max-gap-reset-ts` | `max-gap-reset-ts` | <=0 取24h；T6/T8 |
| PDServerConfig.KeyType (516) | `string` | `key-type` | `key-type` | 缺失赋 table；显式空串随后校验失败；T6/T8 |
| PDServerConfig.RuntimeServices (518) | `typeutil.StringSlice` | `runtime-services` | `runtime-services` | 缺失赋空 slice；显式 nil/空遵循输入；T6/T8 |
| PDServerConfig.MetricStorage (521) | `string` | `metric-storage` | `metric-storage` | 保留零值/输入；无本文件默认；T6/T7/T8 |
| PDServerConfig.DashboardAddress (523) | `string` | `dashboard-address` | `dashboard-address` | 缺失赋 auto；显式空串随后校验失败；T6/T8 |
| PDServerConfig.FlowRoundByDigit (525) | `int` | `flow-round-by-digit` | `flow-round-by-digit` | 仅缺失且0时取3；显式0保留；T6/T8 |
| PDServerConfig.MinResolvedTSPersistenceInterval (527) | `typeutil.Duration` | `min-resolved-ts-persistence-interval` | `min-resolved-ts-persistence-interval` | 仅缺失且<=0取1s；显式0保留；T6/T8 |
| PDServerConfig.ServerMemoryLimit (529) | `float64` | `server-memory-limit` | `server-memory-limit` | 仅缺失且0时取0；总是钳位[0,0.99]；T6/T8 |
| PDServerConfig.ServerMemoryLimitGCTrigger (531) | `float64` | `server-memory-limit-gc-trigger` | `server-memory-limit-gc-trigger` | 仅缺失且0时取0.7；总是钳位[0.5,0.99]；T6/T8 |
| PDServerConfig.EnableGOGCTuner (533) | `bool` | `enable-gogc-tuner` | `enable-gogc-tuner,string` | 缺失赋 true；T6/T8 |
| PDServerConfig.GCTunerThreshold (535) | `float64` | `gc-tuner-threshold` | `gc-tuner-threshold` | 仅缺失且0时取0.6；总是钳位[0,0.9]；T6/T8 |
| PDServerConfig.BlockSafePointV1 (537) | `bool` | `block-safe-point-v1` | `block-safe-point-v1,string` | 保留零值/输入；无本文件默认；T6/T7/T8 |
| PDServerConfig.GCKeyspaceIndexLoadConcurrency (539) | `int` | `gc-keyspace-index-load-concurrency` | `gc-keyspace-index-load-concurrency` | 仅缺失且0时取4；Validate 范围[1,16]，显式0拒绝；T6/T7 |
| PDServerConfig.GCStateCacheWarmupConcurrency (541) | `int` | `gc-state-cache-warmup-concurrency` | `gc-state-cache-warmup-concurrency` | 仅缺失且0时取4；Validate 范围[1,16]，显式0拒绝；T6/T7 |
| StoreLabel.Key (667) | `string` | `key` | `key` | 保留零值/输入；无本文件默认；T2/T15 |
| StoreLabel.Value (668) | `string` | `value` | `value` | 保留零值/输入；无本文件默认；T2/T15 |
| DashboardConfig.TiDBCAPath (781) | `string` | `tidb-cacert-path` | `tidb-cacert-path` | 保留零值/输入；无本文件默认；T12（TLS IO U） |
| DashboardConfig.TiDBCertPath (782) | `string` | `tidb-cert-path` | `tidb-cert-path` | 保留零值/输入；无本文件默认；T12（TLS IO U） |
| DashboardConfig.TiDBKeyPath (783) | `string` | `tidb-key-path` | `tidb-key-path` | 保留零值/输入；无本文件默认；T12（TLS IO U） |
| DashboardConfig.PublicPathPrefix (784) | `string` | `public-path-prefix` | `public-path-prefix` | 保留零值/输入；无本文件默认；T12（TLS IO U） |
| DashboardConfig.TempDir (785) | `string` | `temp-dir` | `temp-dir` | 保留零值/输入；无本文件默认；T12（TLS IO U） |
| DashboardConfig.InternalProxy (786) | `bool` | `internal-proxy` | `internal-proxy` | 保留零值/输入；无本文件默认；T12（TLS IO U） |
| DashboardConfig.EnableTelemetry (787) | `bool` | `enable-telemetry` | `enable-telemetry` | 缺失赋 edition 默认（当前 false）；显式值保留；T12 |
| DashboardConfig.EnableExperimental (788) | `bool` | `enable-experimental` | `enable-experimental` | 保留零值/输入；无本文件默认；T12（TLS IO U） |
| DashboardConfig.DisableCustomPromAddr (789) | `bool` | `disable-custom-prom-addr` | `disable-custom-prom-addr` | 保留零值/输入；无本文件默认；T12（TLS IO U） |
| ReplicationModeConfig.ReplicationMode (818) | `string` | `replication-mode` | `replication-mode` | 缺失或 Normalize 返回空则赋 majority；合法原文不改写；T10 |
| ReplicationModeConfig.DRAutoSync (819) | `DRAutoSyncReplicationConfig` | `dr-auto-sync` | `dr-auto-sync` | 无条件调用其 adjust；T10 |
| DRAutoSyncReplicationConfig.LabelKey (847) | `string` | `label-key` | `label-key` | 保留零值/输入；无本文件默认；T10 |
| DRAutoSyncReplicationConfig.Primary (848) | `string` | `primary` | `primary` | 保留零值/输入；无本文件默认；T10 |
| DRAutoSyncReplicationConfig.DR (849) | `string` | `dr` | `dr` | 保留零值/输入；无本文件默认；T10 |
| DRAutoSyncReplicationConfig.PrimaryReplicas (850) | `int` | `primary-replicas` | `primary-replicas` | 保留零值/输入；无本文件默认；T10 |
| DRAutoSyncReplicationConfig.DRReplicas (851) | `int` | `dr-replicas` | `dr-replicas` | 保留零值/输入；无本文件默认；T10 |
| DRAutoSyncReplicationConfig.WaitStoreTimeout (852) | `typeutil.Duration` | `wait-store-timeout` | `wait-store-timeout` | 仅缺失赋1min；显式0/负值保留；T10 |
| DRAutoSyncReplicationConfig.WaitRecoverTimeout (853) | `typeutil.Duration` | `wait-recover-timeout` | `wait-recover-timeout` | 保留零值/输入；无本文件默认；T10 |
| DRAutoSyncReplicationConfig.PauseRegionSplit (854) | `bool` | `pause-region-split` | `pause-region-split,string` | 保留零值/输入；无本文件默认；T10 |
| MicroserviceConfig.EnableSchedulingFallback (865) | `bool` | `enable-scheduling-fallback` | `enable-scheduling-fallback,string` | 缺失赋 true；显式值保留；T11 |
| MicroserviceConfig.EnableTSODynamicSwitching (866) | `bool` | `enable-tso-dynamic-switching` | `enable-tso-dynamic-switching,string` | 缺失赋 false；显式值保留；T11 |
| MicroserviceConfig.EnableResourceManagerFallback (870) | `bool` | `enable-resource-manager-fallback` | `enable-resource-manager-fallback,string` | 缺失赋 true；显式值保留；T11 |
| KeyspaceConfig.PreAlloc (909) | `[]string` | `pre-alloc` | `pre-alloc` | 保留零值/输入；无本文件默认；T9 |
| KeyspaceConfig.WaitRegionSplit (911) | `bool` | `wait-region-split` | `wait-region-split` | 缺失赋 true；T9 |
| KeyspaceConfig.WaitRegionSplitTimeout (913) | `typeutil.Duration` | `wait-region-split-timeout` | `wait-region-split-timeout` | 缺失赋30s；显式0/负值保留；T9 |
| KeyspaceConfig.CheckRegionSplitInterval (915) | `typeutil.Duration` | `check-region-split-interval` | `check-region-split-interval` | 缺失赋50ms；显式0/负值保留；T9 |
| KeyspaceConfig.MetaServiceGroups (918) | `map[string]string` | `meta-service-groups` | `meta-service-groups` | trim/完整校验成功后替换原 map；T9 |
#### `server/config/service_middleware_config.go`

| 类型.字段（行） | Go 类型 | TOML | JSON | 缺失/零值/默认与归属 |
| --- | --- | --- | --- | --- | --- |
| ServiceMiddlewareConfig.AuditConfig (27) | `(匿名嵌入)` | `无显式 tag` | `audit` | 保留零值/输入；无本文件默认；T13 |
| ServiceMiddlewareConfig.RateLimitConfig (28) | `(匿名嵌入)` | `无显式 tag` | `rate-limit` | 保留零值/输入；无本文件默认；T13 |
| ServiceMiddlewareConfig.GRPCRateLimitConfig (29) | `(匿名嵌入)` | `无显式 tag` | `grpc-rate-limit` | 保留零值/输入；无本文件默认；T13 |
| AuditConfig.EnableAudit (62) | `bool` | `无显式 tag` | `enable-audit,string` | NewServiceMiddlewareConfig 赋 true；裸零值 false；T13 |
| RateLimitConfig.EnableRateLimit (74) | `bool` | `无显式 tag` | `enable-rate-limit,string` | NewServiceMiddlewareConfig 赋 true；裸零值 false；T13 |
| RateLimitConfig.LimiterConfig (76) | `map[string]ratelimit.DimensionConfig` | `无显式 tag` | `limiter-config` | 构造器建空 map；裸零值 nil；Clone 复制 map；T13 |
| GRPCRateLimitConfig.EnableRateLimit (93) | `bool` | `无显式 tag` | `enable-grpc-rate-limit,string` | NewServiceMiddlewareConfig 赋 true；裸零值 false；T13 |
| GRPCRateLimitConfig.LimiterConfig (95) | `map[string]ratelimit.DimensionConfig` | `无显式 tag` | `grpc-limiter-config` | 构造器建空 map；裸零值 nil；Clone 复制 map；T13 |
#### `server/config/persist_options.go`

| 类型.字段（行） | Go 类型 | TOML | JSON | 缺失/零值/默认与归属 |
| --- | --- | --- | --- | --- | --- |
| PersistOptions.persistMu (50) | `sync.Mutex` | `无显式 tag` | `无显式 tag` | 运行时原子/缓存/锁或持久化装配；不作为纯 Config 字段迁移；U |
| PersistOptions.ttl (53) | `*cache.TTLString` | `无显式 tag` | `无显式 tag` | 运行时原子/缓存/锁或持久化装配；不作为纯 Config 字段迁移；U |
| PersistOptions.schedule (54) | `atomic.Value` | `无显式 tag` | `无显式 tag` | 运行时原子/缓存/锁或持久化装配；不作为纯 Config 字段迁移；U |
| PersistOptions.replication (55) | `atomic.Value` | `无显式 tag` | `无显式 tag` | 运行时原子/缓存/锁或持久化装配；不作为纯 Config 字段迁移；U |
| PersistOptions.pdServerConfig (56) | `atomic.Value` | `无显式 tag` | `无显式 tag` | 运行时原子/缓存/锁或持久化装配；不作为纯 Config 字段迁移；U |
| PersistOptions.replicationMode (57) | `atomic.Value` | `无显式 tag` | `无显式 tag` | 运行时原子/缓存/锁或持久化装配；不作为纯 Config 字段迁移；U |
| PersistOptions.labelProperty (58) | `atomic.Value` | `无显式 tag` | `无显式 tag` | 运行时原子/缓存/锁或持久化装配；不作为纯 Config 字段迁移；U |
| PersistOptions.keyspace (59) | `atomic.Value` | `无显式 tag` | `无显式 tag` | 运行时原子/缓存/锁或持久化装配；不作为纯 Config 字段迁移；U |
| PersistOptions.microservice (60) | `atomic.Value` | `无显式 tag` | `无显式 tag` | 运行时原子/缓存/锁或持久化装配；不作为纯 Config 字段迁移；U |
| PersistOptions.storeConfig (61) | `atomic.Value` | `无显式 tag` | `无显式 tag` | 运行时原子/缓存/锁或持久化装配；不作为纯 Config 字段迁移；U |
| PersistOptions.clusterVersion (62) | `unsafe.Pointer` | `无显式 tag` | `无显式 tag` | 运行时原子/缓存/锁或持久化装配；不作为纯 Config 字段迁移；U |
| persistedConfig.Config (827) | `(匿名嵌入)` | `无显式 tag` | `无显式 tag` | 运行时原子/缓存/锁或持久化装配；不作为纯 Config 字段迁移；U |
| persistedConfig.StoreConfig (829) | `sc.StoreConfig` | `无显式 tag` | `store` | 运行时原子/缓存/锁或持久化装配；不作为纯 Config 字段迁移；U |

`LabelPropertyConfig` 是 `map[string][]StoreLabel`（非 struct），无额外 tag；
零值 nil，Clone 分配新 map 和各 label slice，空值的 nil/空编码归 T2/T16。
Middleware 三个匿名嵌入字段只有 JSON 名称，没有 TOML tag；DimensionConfig
来自 ratelimit，QPS float64、QPSBurst int、ConcurrencyLimit uint64 均无 JSON/TOML tag，
不可误写成全小写字段名（需 T13 的真实编码证据）。

### Adjust/Validate 与其他分支的逐项归属

| 源码分支 / 顺序 | 可观察行为及归属 |
| --- | --- |
| Config.Parse 280–331 | 配置文件路径非空才读取；失败返回；旧 log-file/level 非空警告，嵌套值为空才回填；disable-raft-learner 存在即警告。CLI 字符串/布尔覆盖由 pflag Changed 决定，最后 Adjust(meta,false)。废弃规则 T14，文件与 CLI U |
| Config.Validate 334–355 | Join 与 InitialCluster 同时非空先拒绝；Abs(DataDir)、Abs(log file)、Rel 各可失败；rel 不以 `..` 开头拒绝 log 位于 data 下。路径及工作目录为宿主 U，互斥纯值 T3/T4；不可移植为简单字符串前缀 |
| Config.Adjust 358–378 | 先未知项警告；Name 空则 hostname（错误提前返回）；DataDir 默认并 AdjustPath；先 Validate 再其余默认。顺序影响错误优先级，T4/T14/T15，环境 U |
| 379–404 地址/初始集群 | 默认监听/广告地址；Metric interval；InitialCluster 空时按逗号列表生成且不 trim；Join 非空逐项 parseUrls，失败包装原 Join 与错误。T3/T4，Metric E |
| 406–423 TSO | 0 stream limit 变5000（注释说0禁用但 Adjust 的实际结果优先）；Duration <=0 回默认；物理间隔上界分支优先再下界；非50ms写 warning log（不是 WarningMsgs）。T5 |
| 425–440 map/大小/时间 | nil Labels 建空map；compaction 空串默认；quota/request 仅按 presence，不按值；tick/election 非正回默认。T4 |
| 442–451 外部和 PDServer | Metric job 空取 Name；Schedule.Adjust(child,reloading)、Replication.Adjust(child)、PDServer.adjust 依次调用，任何错误立即返回。T6/T15/E |
| 453–482 余下嵌套 | adjustLog；两个内部 interval 默认；prevote/gateway 仅缺失赋 true；Dashboard、ReplicationMode；Keyspace 错误立即返回；Microservice；Encryption.Adjust 错误立即返回；Controller 最后调整。T4/T9–T12/T15/E |
| adjustLog 485–491 | disable-error-verbose 仅缺失赋 true；空 format/level 取 text/info，显式空也被替换；E/T15 |
| PDServer.adjust 544–600 | 两GC字段缺失才 AdjustInt（只有0替换）；MaxReset 非正默认；其余 presence 条件见字段表；三个 float 总是钳位，下界分支优先；再 migrateConfigurationFromFile，最后 Validate。T6/T7/T8/T14 |
| migrateConfigurationFromFile 603–611 | 仅 trace-region-flow 存在便报 conflict，不要求 flow-round-by-digit 同时存在；不静默迁移其值。T14 |
| PDServer.Validate 623–652 | GC先校验；DashboardAddress auto/none 直接通过，其他 ValidateURLWithScheme；KeyType 仅 table/raw/txn；FlowRoundByDigit <0 拒绝，无上界；float 超范围拒绝。单独 Validate 不做钳位，NaN 的大小比较为 false。T7/T8 |
| validateGCConcurrency 655–663 | load 然后 warmup；各 <1 或 >16 返回含字段名和16的错误，顺序决定首错。T7 |
| Dashboard.adjust / initByLDFlags | telemetry 仅缺失赋默认；全局初始 false，非Community设false；Community 分支不主动设true。TestDashboardConfig 手动先设true才观察版本差异；T12，宿主传入 edition 不引入全局可变状态 |
| Dashboard.ToTiDBTLSConfig 793–806 | cert/key均非空或CA非空才调用 transport.ClientConfig；错误返回，否则空配置返回nil,nil；TLS文件 IO U，字段值 T12 |
| ReplicationMode.adjust / Normalize 828–842 | 小写+下划线改连字符只用于合法性判断，接受 majority/dr-auto-sync；缺失/非法赋majority；合法原文仍保留；无条件调整DR。T10 |
| DRAutoSync.adjust 857–860 | 仅 wait-store-timeout 缺失赋1min，不校验replica、label或其他timeout。T10 |
| Microservice.adjust 873–883 | 三个独立 presence 条件，分别true/false/true；显式值保留。T11 |
| Keyspace.Validate 922–930 | interval 必须[1ms,100ms]且严格小于timeout，先范围后相对大小。T9 |
| Keyspace.adjust 933–945 | 三个presence默认后只调用 AdjustMetaServiceGroups；不调用 Keyspace.Validate，不得将两入口合并。T9 |
| IsValidMetaServiceGroupID 951–953 | 只拒绝 `/`；空ID本函数返回true，空检查属于上层adjust。T9 |
| AdjustMetaServiceGroups 956–983 | trim ID/endpoint；按空ID、斜杠、空地址、trim后重复ID顺序拒绝；先用临时map验证全部，成功后清空/重填原map；失败不部分写入。多个坏项首错受Go map迭代影响，不能强加排序承诺。T9 |
| Clone / getters | Config.Clone浅拷贝（map/嵌套slice共享）；PDServer复制RuntimeServices；LabelProperty深复制map/slice；Keyspace复制PreAlloc/map，GetPreAlloc/GetMetaServiceGroups返回副本，SetMetaServiceGroups直接赋map；其余值clone按源码。T2/T6/T9–T13/T15，禁止统一改成深clone冒充Go语义 |
| Config.String | MarshalIndent两空格；失败返回 `<nil>`，JSON隐藏内部字段；T2/T16（不能用Display输出替代错误语义） |
| Middleware构造与Clone | 裸零值不同于构造器true+空map；顶层Clone浅拷贝仍共享嵌套map，Rate/GRPC各自Clone复制map；gRPC构造实际使用defaultEnableRateLimitMiddleware。T13 |
| GenEmbedEtcdConfig | 赋值NewConfig、TLS与logger桥接；client仅多CN时AllowedCNs，peer始终AllowedCNs；四地址顺序解析，任一失败返回；tick/election转毫秒再转uint，StrictReconfig取反。全部 U（embedded etcd启动装配） |

`reloading` 只传给 Schedule.Adjust；本包其他默认/校验没有独立 reload 分支。
`PersistOptions.Reload` 先 Adjust(nil,true)，再 LoadConfig 覆盖，之后只检查GC并发、
补默认scheduler/迁移废弃flags、StoreConfig.Adjust；不能解释成对载入对象再次完整Adjust。
JSON/TOML差分应明确测试这一边界，热更新行为仍属于 U。

PD_EDITION：Makefile:21–25 限定 Community/Enterprise，77行以ldflags注入
versioninfo.PDEdition；版本默认 Community。当前默认telemetry=false对两版均成立，
不能照测试手动改全局变量的前置条件推出“Community默认true”。T12负责值契约，
环境读取/编译装配 U；本任务不运行 make。

### 外部嵌套拥有包与准入门

| 嵌套项 | 真实拥有来源、现状 | T15/T13准入（缺一不得装完整Config） |
| --- | --- | --- |
| schedule / replication | `pkg/schedule/config/config.go`；当前无对应Cargo.toml/Rust值实现 | 上层主题4提供canonical默认/Validate/序列化、presence、reloading、废弃flags与Go差分；分别复用ScheduleConfig/ReplicationConfig，不复制算法 |
| controller / request-unit / RU policy | `pkg/mcs/resourcemanager/server/config.go`；无Rust配置实现 | 上层资源管理主题提供ControllerConfig/RequestUnitConfig/RUVersionPolicy真实值契约、缺失与omitempty；Adjust的无效push interval清空address及故障边界须有证据 |
| security / TLS / redact / encryption | `pkg/utils/configutil/configutil.go`、`pkg/utils/grpcutil/grpcutil.go`、`pkg/encryption/config.go`；当前configutil仅默认/metadata/未知项 | 上层安全主题提供canonical Security/TLS/Redact/Encryption及错误/密钥配置差分；不能因configutil crate存在就认定SecurityConfig已迁移；TLS证书读取/KMS不扩进本任务 |
| log | Go `github.com/pingcap/log.Config`（根go.mod固定版本），本地 `pkg/utils/logutil` 使用它 | 上层日志/可观测性主题提供值模型、log.file字段、旧项优先级/默认/编码；不在库中初始化全局logger |
| metric | `pkg/utils/metricutil/metricutil.go:37`，job/address/interval；无Rust配置crate | 上层可观测性主题提供canonical MetricConfig及编码，保持interval<=0取15s、job空取Name；Push后台IO由宿主负责 |
| metering | Go `github.com/pingcap/metering_sdk/config.MeteringConfig` | 上层计量/资源管理主题固定依赖来源和字段契约；本Go Adjust无调用，不能凭空套默认或丢弃字段 |
| semver | Go `github.com/coreos/go-semver/semver.Version` | T15确认JSON/TOML值表达、零值与真实Go编码证据，不以任意字符串替代 |
| middleware limiter dimension | `pkg/ratelimit/limiter.go:30`；Rust已有limiter/controller，通过ControllerOption::Dimensions传递值，未实现Go同名配置struct | T13复用canonical ratelimit并验证Go无tag字段名QPS/QPSBurst/ConcurrencyLimit、默认及map复制语义；现有运行时限流测试不代表配置codec完成 |

T15 为明确外部准入门；本盘点完成不代表主题4/7/8或整个配置迁移完成。
所有外部字段均已有上述归属；未就绪时后续任务保留阻塞，不使用Value、空struct
或恒成功validator绕过。新建server/config/Cargo.toml、lib.rs及config_decode.rs等
均仍为拟建，当前该目录没有Rust配置实现。

### 反向依赖、持久化 key/value、事务及 watch/lease

只读调用方：`cmd/pd-server/main.go:221`创建Config/Parse；
`server/server.go:258–304`装配Config、NewPersistOptions、NewServiceMiddlewareConfig、
GenEmbedEtcdConfig；437行重用embedded配置；1218行GetConfig导出API。
`server/api/config.go`通过反射公共tag查更新键，分别更新schedule/replication/
PDServer/replication-mode/keyspace/microservice；`server/api/service_middleware.go`
依赖middleware JSON；MCS scheduling/router的server.go与testutil.go消费Config。
这些消费者说明公共tag和clone语义不能任意变更，HTTP/CLI/启动实现 U。

PersistOptions三文件中的所有符号归U，包含atomic读写/CAS、锁、缓存、各类配置代理；
不在本计划下沉第二份存储契约。persistMu串行化schedule读改写与整配置保存：
UpdateScheduleConfig在callback错误或changed=false时不发布，保存失败回滚内存；
SetSchedulers/SetStoreLimit/Add/RemoveScheduler基于最新clone，避免旧快照覆盖。
GetStoreLimit缺失时创建条目，PeekStoreLimitByType无创建副作用；SetAllStoresLimit
还更新Go全局默认。scheduler默认项移除是disable，其余删除；label属性去重/删空；
TTL getters覆盖静态配置，merge-keys可由TTL merge-size乘10000推导，转换失败回默认。
这些是源码推导的热更新职责，不声称本任务已验证其并发或故障行为。

持久化路径依据 `pkg/utils/keypath/config.go` 和 `pkg/storage/endpoint/config.go`：
Config保存为 `/pd/{cluster_id}/config` 的JSON，middleware单独保存于
`/pd/{cluster_id}/service_middleware`（格式见absolute_key_path.go:42/47）；persistedConfig匿名嵌入
Config并注入JSON `store`，只聚合Schedule/Replication/PDServer/ReplicationMode/
LabelProperty/Keyspace/Microservice/ClusterVersion/StoreConfig。
UnmarshalJSON额外读取schedule raw JSON做废弃flag迁移。persistFail为已有故障注入点。
当前Rust `pkg/storage/endpoint/config_endpoint.rs` 已有泛型JSON load/save及middleware
load/save，`ClusterPaths`提供key；它们不提供服务默认/校验，更不代表PersistOptions。

TTL前缀为 `/config/ttl`（schedule/config.go:109），每项key拼 `/` 和字段名，value是
字符串；非零ttl调用EtcdKVPutWithTTL绑定lease（注释记录最小5s），零ttl调用Delete，
成功后写内存TTL；LoadTTLFromEtcd做prefix读取、逐条TimeToLive并恢复缓存。
supportedTTLConfigs有13项，不能把所有getTTLNumberOr的键自动视为受支持项。
Config纯值无事务/watch/lease；PersistOptions的互斥与回滚不是后端compare事务，
这里没有watch循环，TTL lease/缓存生命周期由U及已有MetaStore/观察工具契约承接。

### 全部来源符号与归属索引

以下索引包括非公开helper和多行签名UpdateScheduleConfig；常量另列。
具体字段与分支以上表为准，符号索引用来防止遗漏。

#### `config.go` 符号

| 行 | 声明 | 归属 |
| --- | --- | --- |
| 50 | `type Config struct` | T2/T4/T5/T14/T15（按上文分支细分） |
| 172 | `func NewConfig() *Config` | T2/T4/T5/T14/T15（按上文分支细分） |
| 269 | `func init()` | T12；edition由宿主提供 |
| 273 | `func initByLDFlags(edition string)` | T12；edition由宿主提供 |
| 280 | `func (c *Config) Parse(flagSet *pflag.FlagSet) error` | U：CLI/文件系统/TLS/embedded etcd；纯值部分见上表 |
| 334 | `func (c *Config) Validate() error` | T2/T4/T5/T14/T15；宿主项 U |
| 358 | `func (c *Config) Adjust(meta *toml.MetaData, reloading bool) error` | T2/T4/T5/T14/T15；宿主项 U |
| 485 | `func (c *Config) adjustLog(meta *configutil.ConfigMetaData)` | T2/T4/T5/T14/T15；宿主项 U |
| 494 | `func (c *Config) Clone() *Config` | T2/T4/T5/T14/T15；宿主项 U |
| 499 | `func (c *Config) String() string` | T2/T4/T5/T14/T15；宿主项 U |
| 509 | `type PDServerConfig struct` | T2/T4/T5/T14/T15（按上文分支细分） |
| 544 | `func (c *PDServerConfig) adjust(meta *configutil.ConfigMetaData) error` | T6/T7/T8 |
| 603 | `func migrateConfigurationFromFile(meta *configutil.ConfigMetaData) error` | T14 |
| 615 | `func (c *PDServerConfig) Clone() *PDServerConfig` | T6/T7/T8 |
| 623 | `func (c *PDServerConfig) Validate() error` | T6/T7/T8 |
| 655 | `func (c *PDServerConfig) validateGCConcurrency() error` | T6/T7/T8 |
| 666 | `type StoreLabel struct` | T2/T4/T5/T14/T15（按上文分支细分） |
| 673 | `type LabelPropertyConfig map[string][]StoreLabel` | T2/T4/T5/T14/T15（按上文分支细分） |
| 676 | `func (c LabelPropertyConfig) Clone() LabelPropertyConfig` | T2/T4/T5/T14/T15（按上文分支细分） |
| 687 | `func (c *Config) GetLease() int64` | T2/T4/T5/T14/T15；宿主项 U |
| 693 | `func (c *Config) GetMaxConcurrentTSOProxyStreamings() int` | T2/T4/T5/T14/T15；宿主项 U |
| 698 | `func (c *Config) GetTSOProxyRecvFromClientTimeout() time.Duration` | T2/T4/T5/T14/T15；宿主项 U |
| 703 | `func (c *Config) GetTSOUpdatePhysicalInterval() time.Duration` | T2/T4/T5/T14/T15；宿主项 U |
| 708 | `func (c *Config) GetTSOSaveInterval() time.Duration` | T2/T4/T5/T14/T15；宿主项 U |
| 713 | `func (c *Config) GetTLSConfig() *grpcutil.TLSConfig` | T2/T4/T5/T14/T15；宿主项 U |
| 718 | `func (c *Config) GenEmbedEtcdConfig() (*embed.Config, error)` | U：CLI/文件系统/TLS/embedded etcd；纯值部分见上表 |
| 780 | `type DashboardConfig struct` | T2/T4/T5/T14/T15（按上文分支细分） |
| 793 | `func (c *DashboardConfig) ToTiDBTLSConfig() (*tls.Config, error)` | U：CLI/文件系统/TLS/embedded etcd；纯值部分见上表 |
| 809 | `func (c *DashboardConfig) adjust(meta *configutil.ConfigMetaData)` | T12（TLS IO U） |
| 817 | `type ReplicationModeConfig struct` | T2/T4/T5/T14/T15（按上文分支细分） |
| 823 | `func (c *ReplicationModeConfig) Clone() *ReplicationModeConfig` | T10 |
| 828 | `func (c *ReplicationModeConfig) adjust(meta *configutil.ConfigMetaData)` | T10 |
| 837 | `func NormalizeReplicationMode(m string) string` | T10 |
| 846 | `type DRAutoSyncReplicationConfig struct` | T2/T4/T5/T14/T15（按上文分支细分） |
| 857 | `func (c *DRAutoSyncReplicationConfig) adjust(meta *configutil.ConfigMetaData)` | T10 |
| 864 | `type MicroserviceConfig struct` | T2/T4/T5/T14/T15（按上文分支细分） |
| 873 | `func (c *MicroserviceConfig) adjust(meta *configutil.ConfigMetaData)` | T11 |
| 886 | `func (c *MicroserviceConfig) Clone() *MicroserviceConfig` | T11 |
| 892 | `func (c *MicroserviceConfig) IsSchedulingFallbackEnabled() bool` | T11 |
| 897 | `func (c *MicroserviceConfig) IsTSODynamicSwitchingEnabled() bool` | T11 |
| 902 | `func (c *MicroserviceConfig) IsResourceManagerFallbackEnabled() bool` | T11 |
| 907 | `type KeyspaceConfig struct` | T2/T4/T5/T14/T15（按上文分支细分） |
| 922 | `func (c *KeyspaceConfig) Validate() error` | T9 |
| 933 | `func (c *KeyspaceConfig) adjust(meta *configutil.ConfigMetaData) error` | T9 |
| 951 | `func IsValidMetaServiceGroupID(id string) bool` | T9 |
| 956 | `func AdjustMetaServiceGroups(metaGroups map[string]string) error` | T9 |
| 987 | `func (c *KeyspaceConfig) Clone() *KeyspaceConfig` | T9 |
| 1000 | `func (c *KeyspaceConfig) GetPreAlloc() []string` | T9 |
| 1007 | `func (c *KeyspaceConfig) ToWaitRegionSplit() bool` | T9 |
| 1012 | `func (c *KeyspaceConfig) GetWaitRegionSplitTimeout() time.Duration` | T9 |
| 1017 | `func (c *KeyspaceConfig) GetCheckRegionSplitInterval() time.Duration` | T9 |
| 1022 | `func (c *KeyspaceConfig) GetMetaServiceGroups() map[string]string` | T9 |
| 1031 | `func (c *KeyspaceConfig) SetMetaServiceGroups(metaServiceGroups map[string]string)` | T9 |
#### `service_middleware_config.go` 符号

| 行 | 声明 | 归属 |
| --- | --- | --- |
| 26 | `type ServiceMiddlewareConfig struct` | T13 |
| 33 | `func NewServiceMiddlewareConfig() *ServiceMiddlewareConfig` | T13 |
| 54 | `func (c *ServiceMiddlewareConfig) Clone() *ServiceMiddlewareConfig` | T13 |
| 60 | `type AuditConfig struct` | T13 |
| 66 | `func (c *AuditConfig) Clone() *AuditConfig` | T13 |
| 72 | `type RateLimitConfig struct` | T13 |
| 80 | `func (c *RateLimitConfig) Clone() *RateLimitConfig` | T13 |
| 91 | `type GRPCRateLimitConfig struct` | T13 |
| 99 | `func (c *GRPCRateLimitConfig) Clone() *GRPCRateLimitConfig` | T13 |
#### `persist_options.go` 符号

| 行 | 声明 | 归属 |
| --- | --- | --- |
| 49 | `type PersistOptions struct` | U：PersistOptions/TTL 热更新 |
| 66 | `func NewPersistOptions(cfg *Config) *PersistOptions` | U：PersistOptions/TTL 热更新 |
| 84 | `func (o *PersistOptions) GetScheduleConfig() *sc.ScheduleConfig` | U：PersistOptions/TTL 热更新 |
| 89 | `func (o *PersistOptions) SetScheduleConfig(cfg *sc.ScheduleConfig)` | U：PersistOptions/TTL 热更新 |
| 96 | `func (o *PersistOptions) mutateScheduleConfig(update func(next *sc.ScheduleConfig))` | U：PersistOptions/TTL 热更新 |
| 109 | `func (o *PersistOptions) UpdateScheduleConfig(` | U：PersistOptions/TTL 热更新 |
| 131 | `func (o *PersistOptions) SetSchedulers(schedulers sc.SchedulerConfigs)` | U：PersistOptions/TTL 热更新 |
| 138 | `func (o *PersistOptions) GetReplicationConfig() *sc.ReplicationConfig` | U：PersistOptions/TTL 热更新 |
| 143 | `func (o *PersistOptions) SetReplicationConfig(cfg *sc.ReplicationConfig)` | U：PersistOptions/TTL 热更新 |
| 148 | `func (o *PersistOptions) GetPDServerConfig() *PDServerConfig` | U：PersistOptions/TTL 热更新 |
| 153 | `func (o *PersistOptions) SetPDServerConfig(cfg *PDServerConfig)` | U：PersistOptions/TTL 热更新 |
| 158 | `func (o *PersistOptions) GetReplicationModeConfig() *ReplicationModeConfig` | U：PersistOptions/TTL 热更新 |
| 163 | `func (o *PersistOptions) SetReplicationModeConfig(cfg *ReplicationModeConfig)` | U：PersistOptions/TTL 热更新 |
| 168 | `func (o *PersistOptions) GetLabelPropertyConfig() LabelPropertyConfig` | U：PersistOptions/TTL 热更新 |
| 173 | `func (o *PersistOptions) SetLabelPropertyConfig(cfg LabelPropertyConfig)` | U：PersistOptions/TTL 热更新 |
| 178 | `func (o *PersistOptions) GetKeyspaceConfig() *KeyspaceConfig` | U：PersistOptions/TTL 热更新 |
| 183 | `func (o *PersistOptions) SetKeyspaceConfig(cfg *KeyspaceConfig)` | U：PersistOptions/TTL 热更新 |
| 188 | `func (o *PersistOptions) CASKeyspaceConfig(oldCfg, newCfg *KeyspaceConfig) bool` | U：PersistOptions/TTL 热更新 |
| 193 | `func (o *PersistOptions) GetMicroserviceConfig() *MicroserviceConfig` | U：PersistOptions/TTL 热更新 |
| 198 | `func (o *PersistOptions) SetMicroserviceConfig(cfg *MicroserviceConfig)` | U：PersistOptions/TTL 热更新 |
| 203 | `func (o *PersistOptions) GetStoreConfig() *sc.StoreConfig` | U：PersistOptions/TTL 热更新 |
| 208 | `func (o *PersistOptions) SetStoreConfig(cfg *sc.StoreConfig)` | U：PersistOptions/TTL 热更新 |
| 213 | `func (o *PersistOptions) GetClusterVersion() *semver.Version` | U：PersistOptions/TTL 热更新 |
| 218 | `func (o *PersistOptions) SetClusterVersion(v *semver.Version)` | U：PersistOptions/TTL 热更新 |
| 223 | `func (o *PersistOptions) CASClusterVersion(old, new *semver.Version) bool` | U：PersistOptions/TTL 热更新 |
| 228 | `func (o *PersistOptions) GetLocationLabels() []string` | U：PersistOptions/TTL 热更新 |
| 233 | `func (o *PersistOptions) SetLocationLabels(labels []string)` | U：PersistOptions/TTL 热更新 |
| 240 | `func (o *PersistOptions) GetIsolationLevel() string` | U：PersistOptions/TTL 热更新 |
| 245 | `func (o *PersistOptions) IsPlacementRulesEnabled() bool` | U：PersistOptions/TTL 热更新 |
| 250 | `func (o *PersistOptions) GetAffinityScheduleLimit() uint64` | T12；edition由宿主提供 |
| 255 | `func (o *PersistOptions) GetSplitScatterScheduleLimit() uint64` | U：PersistOptions/TTL 热更新 |
| 260 | `func (o *PersistOptions) SetAffinityScheduleLimit(limit uint64)` | T12；edition由宿主提供 |
| 267 | `func (o *PersistOptions) SetSplitScatterScheduleLimit(limit uint64)` | U：PersistOptions/TTL 热更新 |
| 274 | `func (o *PersistOptions) SetPlacementRuleEnabled(enabled bool)` | U：PersistOptions/TTL 热更新 |
| 281 | `func (o *PersistOptions) IsPlacementRulesCacheEnabled() bool` | U：PersistOptions/TTL 热更新 |
| 286 | `func (o *PersistOptions) SetPlacementRulesCacheEnabled(enabled bool)` | U：PersistOptions/TTL 热更新 |
| 293 | `func (o *PersistOptions) GetStrictlyMatchLabel() bool` | U：PersistOptions/TTL 热更新 |
| 298 | `func (o *PersistOptions) GetMaxReplicas() int` | U：PersistOptions/TTL 热更新 |
| 303 | `func (o *PersistOptions) SetMaxReplicas(replicas int)` | U：PersistOptions/TTL 热更新 |
| 326 | `func IsSupportedTTLConfig(key string) bool` | U：PersistOptions/TTL 热更新 |
| 336 | `func (o *PersistOptions) GetMaxSnapshotCount() uint64` | U：PersistOptions/TTL 热更新 |
| 341 | `func (o *PersistOptions) GetMaxPendingPeerCount() uint64` | U：PersistOptions/TTL 热更新 |
| 346 | `func (o *PersistOptions) GetMaxMergeRegionSize() uint64` | U：PersistOptions/TTL 热更新 |
| 351 | `func (o *PersistOptions) GetMaxAffinityMergeRegionSize() uint64` | T12；edition由宿主提供 |
| 357 | `func (o *PersistOptions) GetMaxMergeRegionKeys() uint64` | U：PersistOptions/TTL 热更新 |
| 370 | `func (o *PersistOptions) GetSplitMergeInterval() time.Duration` | U：PersistOptions/TTL 热更新 |
| 375 | `func (o *PersistOptions) SetSplitMergeInterval(splitMergeInterval time.Duration)` | U：PersistOptions/TTL 热更新 |
| 382 | `func (o *PersistOptions) GetSwitchWitnessInterval() time.Duration` | U：PersistOptions/TTL 热更新 |
| 387 | `func (o *PersistOptions) IsDiagnosticAllowed() bool` | U：PersistOptions/TTL 热更新 |
| 392 | `func (o *PersistOptions) SetEnableDiagnostic(enable bool)` | U：PersistOptions/TTL 热更新 |
| 399 | `func (o *PersistOptions) IsWitnessAllowed() bool` | U：PersistOptions/TTL 热更新 |
| 404 | `func (o *PersistOptions) SetEnableWitness(enable bool)` | U：PersistOptions/TTL 热更新 |
| 411 | `func (o *PersistOptions) SetMaxStoreDownTime(time time.Duration)` | U：PersistOptions/TTL 热更新 |
| 418 | `func (o *PersistOptions) SetMaxMergeRegionSize(maxMergeRegionSize uint64)` | U：PersistOptions/TTL 热更新 |
| 425 | `func (o *PersistOptions) SetMaxAffinityMergeRegionSize(maxAffinityMergeRegionSize uint64)` | T12；edition由宿主提供 |
| 432 | `func (o *PersistOptions) SetMaxMergeRegionKeys(maxMergeRegionKeys uint64)` | U：PersistOptions/TTL 热更新 |
| 439 | `func (o *PersistOptions) SetStoreLimit(storeID uint64, typ storelimit.Type, ratePerMin float64)` | U：PersistOptions/TTL 热更新 |
| 450 | `func (o *PersistOptions) SetAllStoresLimit(typ storelimit.Type, ratePerMin float64)` | U：PersistOptions/TTL 热更新 |
| 461 | `func (o *PersistOptions) IsOneWayMergeEnabled() bool` | U：PersistOptions/TTL 热更新 |
| 466 | `func (o *PersistOptions) IsCrossTableMergeEnabled() bool` | U：PersistOptions/TTL 热更新 |
| 471 | `func (o *PersistOptions) GetPatrolRegionInterval() time.Duration` | U：PersistOptions/TTL 热更新 |
| 476 | `func (o *PersistOptions) GetMaxStoreDownTime() time.Duration` | U：PersistOptions/TTL 热更新 |
| 481 | `func (o *PersistOptions) GetMaxStorePreparingTime() time.Duration` | U：PersistOptions/TTL 热更新 |
| 486 | `func (o *PersistOptions) GetLeaderScheduleLimit() uint64` | U：PersistOptions/TTL 热更新 |
| 491 | `func (o *PersistOptions) GetRegionScheduleLimit() uint64` | U：PersistOptions/TTL 热更新 |
| 496 | `func (o *PersistOptions) GetWitnessScheduleLimit() uint64` | U：PersistOptions/TTL 热更新 |
| 501 | `func (o *PersistOptions) GetReplicaScheduleLimit() uint64` | U：PersistOptions/TTL 热更新 |
| 506 | `func (o *PersistOptions) GetMergeScheduleLimit() uint64` | U：PersistOptions/TTL 热更新 |
| 511 | `func (o *PersistOptions) GetHotRegionScheduleLimit() uint64` | U：PersistOptions/TTL 热更新 |
| 516 | `func (o *PersistOptions) GetStoreLimit(storeID uint64) (returnSC sc.StoreLimitConfig)` | U：PersistOptions/TTL 热更新 |
| 532 | `func (o *PersistOptions) GetStoreLimitByType(storeID uint64, typ storelimit.Type) (returned float64)` | U：PersistOptions/TTL 热更新 |
| 546 | `func (o *PersistOptions) PeekStoreLimitByType(storeID uint64, typ storelimit.Type) float64` | U：PersistOptions/TTL 热更新 |
| 555 | `func storeLimitByType(limit sc.StoreLimitConfig, typ storelimit.Type) float64` | U：PersistOptions/TTL 热更新 |
| 572 | `func (o *PersistOptions) GetAllStoresLimit() map[uint64]sc.StoreLimitConfig` | U：PersistOptions/TTL 热更新 |
| 577 | `func (o *PersistOptions) GetStoreLimitVersion() string` | U：PersistOptions/TTL 热更新 |
| 582 | `func (o *PersistOptions) GetTolerantSizeRatio() float64` | U：PersistOptions/TTL 热更新 |
| 587 | `func (o *PersistOptions) GetLowSpaceRatio() float64` | U：PersistOptions/TTL 热更新 |
| 592 | `func (o *PersistOptions) GetSlowStoreEvictingAffectedStoreRatioThreshold() float64` | U：PersistOptions/TTL 热更新 |
| 597 | `func (o *PersistOptions) GetHighSpaceRatio() float64` | U：PersistOptions/TTL 热更新 |
| 602 | `func (o *PersistOptions) GetRegionScoreFormulaVersion() string` | U：PersistOptions/TTL 热更新 |
| 607 | `func (o *PersistOptions) GetSchedulerMaxWaitingOperator() uint64` | U：PersistOptions/TTL 热更新 |
| 612 | `func (o *PersistOptions) GetLeaderSchedulePolicy() constant.SchedulePolicy` | U：PersistOptions/TTL 热更新 |
| 617 | `func (o *PersistOptions) GetKeyType() constant.KeyType` | U：PersistOptions/TTL 热更新 |
| 622 | `func (o *PersistOptions) GetMaxResetTSGap() time.Duration` | U：PersistOptions/TTL 热更新 |
| 627 | `func (o *PersistOptions) GetDashboardAddress() string` | U：PersistOptions/TTL 热更新 |
| 632 | `func (o *PersistOptions) IsUseRegionStorage() bool` | U：PersistOptions/TTL 热更新 |
| 637 | `func (o *PersistOptions) GetServerMemoryLimit() float64` | U：PersistOptions/TTL 热更新 |
| 642 | `func (o *PersistOptions) GetServerMemoryLimitGCTrigger() float64` | U：PersistOptions/TTL 热更新 |
| 647 | `func (o *PersistOptions) GetEnableGOGCTuner() bool` | U：PersistOptions/TTL 热更新 |
| 652 | `func (o *PersistOptions) GetGCTunerThreshold() float64` | U：PersistOptions/TTL 热更新 |
| 657 | `func (o *PersistOptions) IsRemoveDownReplicaEnabled() bool` | U：PersistOptions/TTL 热更新 |
| 662 | `func (o *PersistOptions) IsReplaceOfflineReplicaEnabled() bool` | U：PersistOptions/TTL 热更新 |
| 667 | `func (o *PersistOptions) IsMakeUpReplicaEnabled() bool` | U：PersistOptions/TTL 热更新 |
| 672 | `func (o *PersistOptions) IsRemoveExtraReplicaEnabled() bool` | U：PersistOptions/TTL 热更新 |
| 677 | `func (o *PersistOptions) IsLocationReplacementEnabled() bool` | U：PersistOptions/TTL 热更新 |
| 682 | `func (o *PersistOptions) IsTikvRegionSplitEnabled() bool` | U：PersistOptions/TTL 热更新 |
| 687 | `func (o *PersistOptions) GetMaxMovableHotPeerSize() int64` | U：PersistOptions/TTL 热更新 |
| 692 | `func (o *PersistOptions) IsDebugMetricsEnabled() bool` | U：PersistOptions/TTL 热更新 |
| 697 | `func (o *PersistOptions) IsUseJointConsensus() bool` | U：PersistOptions/TTL 热更新 |
| 702 | `func (o *PersistOptions) SetEnableUseJointConsensus(enable bool)` | U：PersistOptions/TTL 热更新 |
| 710 | `func (o *PersistOptions) IsTraceRegionFlow() bool` | U：PersistOptions/TTL 热更新 |
| 715 | `func (o *PersistOptions) GetHotRegionCacheHitsThreshold() int` | U：PersistOptions/TTL 热更新 |
| 720 | `func (o *PersistOptions) GetPatrolRegionWorkerCount() int` | U：PersistOptions/TTL 热更新 |
| 725 | `func (o *PersistOptions) GetStoresLimit() map[uint64]sc.StoreLimitConfig` | U：PersistOptions/TTL 热更新 |
| 730 | `func (o *PersistOptions) GetSchedulers() sc.SchedulerConfigs` | U：PersistOptions/TTL 热更新 |
| 735 | `func (o *PersistOptions) IsSchedulerDisabled(tp types.CheckerSchedulerType) bool` | U：PersistOptions/TTL 热更新 |
| 747 | `func (o *PersistOptions) GetHotRegionsWriteInterval() time.Duration` | U：PersistOptions/TTL 热更新 |
| 752 | `func (o *PersistOptions) GetHotRegionsReservedDays() uint64` | U：PersistOptions/TTL 热更新 |
| 757 | `func (o *PersistOptions) AddSchedulerCfg(tp types.CheckerSchedulerType, args []string)` | U：PersistOptions/TTL 热更新 |
| 779 | `func (o *PersistOptions) RemoveSchedulerCfg(tp types.CheckerSchedulerType)` | U：PersistOptions/TTL 热更新 |
| 797 | `func (o *PersistOptions) SetLabelProperty(typ, labelKey, labelValue string)` | U：PersistOptions/TTL 热更新 |
| 809 | `func (o *PersistOptions) DeleteLabelProperty(typ, labelKey, labelValue string)` | U：PersistOptions/TTL 热更新 |
| 826 | `type persistedConfig struct` | U：PersistOptions/TTL 热更新 |
| 834 | `func (c *persistedConfig) UnmarshalJSON(data []byte) error` | U：持久化装配 |
| 852 | `func (o *PersistOptions) SwitchRaftV2(storage endpoint.ConfigStorage) error` | U：PersistOptions/TTL 热更新 |
| 860 | `func (o *PersistOptions) Persist(storage endpoint.ConfigStorage) error` | U：PersistOptions/TTL 热更新 |
| 866 | `func (o *PersistOptions) persistLocked(storage endpoint.ConfigStorage) error` | U：PersistOptions/TTL 热更新 |
| 887 | `func (o *PersistOptions) Reload(storage endpoint.ConfigStorage) error` | U：PersistOptions/TTL 热更新 |
| 918 | `func adjustScheduleCfg(scheduleCfg *sc.ScheduleConfig)` | U：PersistOptions/TTL 热更新 |
| 931 | `func (o *PersistOptions) CheckLabelProperty(typ string, labels []*metapb.StoreLabel) bool` | U：PersistOptions/TTL 热更新 |
| 944 | `func (o *PersistOptions) GetMinResolvedTSPersistenceInterval() time.Duration` | U：PersistOptions/TTL 热更新 |
| 949 | `func (o *PersistOptions) SetTTLData(parCtx context.Context, client *clientv3.Client, key string, value string, ttl time.Duration) error` | U：PersistOptions/TTL 热更新 |
| 970 | `func (o *PersistOptions) getTTLNumber(key string) (uint64, bool, error)` | U：PersistOptions/TTL 热更新 |
| 990 | `func (o *PersistOptions) getTTLNumberOr(key string, defaultValue uint64) uint64` | U：PersistOptions/TTL 热更新 |
| 1000 | `func (o *PersistOptions) getTTLBool(key string) (result bool, contains bool, err error)` | U：PersistOptions/TTL 热更新 |
| 1010 | `func (o *PersistOptions) getTTLBoolOr(key string, defaultValue bool) bool` | U：PersistOptions/TTL 热更新 |
| 1021 | `func (o *PersistOptions) GetTTLData(key string) (string, bool)` | U：PersistOptions/TTL 热更新 |
| 1032 | `func (o *PersistOptions) LoadTTLFromEtcd(ctx context.Context, client *clientv3.Client) error` | U：PersistOptions/TTL 热更新 |
| 1056 | `func (*PersistOptions) SetSchedulingAllowanceStatus(halt bool, source string)` | U：PersistOptions/TTL 热更新 |
| 1067 | `func (o *PersistOptions) SetHaltScheduling(halt bool, source string)` | U：PersistOptions/TTL 热更新 |
| 1075 | `func (o *PersistOptions) IsSchedulingHalted() bool` | U：PersistOptions/TTL 热更新 |
| 1083 | `func (o *PersistOptions) GetRegionMaxSize() uint64` | U：PersistOptions/TTL 热更新 |
| 1088 | `func (o *PersistOptions) GetRegionMaxKeys() uint64` | U：PersistOptions/TTL 热更新 |
| 1093 | `func (o *PersistOptions) GetRegionSplitSize() uint64` | U：PersistOptions/TTL 热更新 |
| 1098 | `func (o *PersistOptions) GetRegionSplitKeys() uint64` | U：PersistOptions/TTL 热更新 |
| 1103 | `func (o *PersistOptions) CheckRegionSize(size, mergeSize uint64) error` | U：PersistOptions/TTL 热更新 |
| 1108 | `func (o *PersistOptions) CheckRegionKeys(keys, mergeKeys uint64) error` | U：PersistOptions/TTL 热更新 |
| 1113 | `func (o *PersistOptions) IsEnableRegionBucket() bool` | U：PersistOptions/TTL 热更新 |
| 1118 | `func (o *PersistOptions) IsRaftKV2() bool` | U：PersistOptions/TTL 热更新 |
| 1123 | `func (o *PersistOptions) GetUnifiedReadPoolMaxThreadCount() uint64` | U：PersistOptions/TTL 热更新 |
| 1129 | `func (o *PersistOptions) SetRegionBucketEnabled(enabled bool)` | U：PersistOptions/TTL 热更新 |
| 1136 | `func (o *PersistOptions) GetRegionBucketSize() uint64` | U：PersistOptions/TTL 热更新 |

公开常量：DefaultMinResolvedTSPersistenceInterval=1s（T6）、
DefaultGCKeyspaceIndexLoadConcurrency=4及DefaultGCStateCacheWarmupConcurrency=4（T6/T7）、
DefaultTSOSaveInterval=5s和MaxTSOUpdatePhysicalInterval=10s（T5）。其余非公开常量
均在字段默认/分支表中落位；maxTraceFlowRoundByDigit=5仅供U的IsTraceRegionFlow。
非公开全局defaultRuntimeServices/defaultEnableTelemetry归T6/T12的宿主值参数；
supportedTTLConfigs、haltSchedulingStatus归U，不引入新的全局状态。

### 相邻测试与现有 Rust 能力

Go测试只读审查，未运行：config_test.go 的 TestBadFormatJoinAddr/TestJoinAddr
（坏join/多地址，T3）；TestValidation/TestAdjust（默认/presence/物理间隔钳位，T4/T5/T15）；
TestPDServerConfig（dashboard auto/none/URL，T8）；TestGCConcurrencyConfig及两类Reload
（缺失4、边界1/16、显式0/负值/17/MaxInt拒绝，T7，Reload执行U）；
TestMigrateFlags/TestLegacyDisableRawKVRegionSplitConfigDoesNotPanic（T14/E）；
TestDashboardConfig（edition预设条件与TLS路径，T12）；TestReplicationMode（T10）；
TestAdjustMetaServiceGroups（trim/空/重复/斜杠，T9）；TestConfigClone（值相等，
并不证明所有嵌套都深clone）；TestRateLimitClone（map隔离，T13）。
TestSecurity/TestTLS是安全值依赖；各TestReload*、TestDefaultStoreLimitAdjust、
TestStoreLimitPartialJSONUpdates、TestHotHistoryRegionConfig、TestMaxStorePreparingTime
归U/调度拥有包。util_test.go的TestValidateURLWithScheme归T3。

当前Rust已有configutil的8种默认调整函数、ConfigMetaData存在性/Child、全局有序未知项，
分别在config_adjust_test.rs/config_metadata_test.rs/config_unknown_test.rs显式注册；
typeutil已有Duration/ByteSize/StringSlice及独立测试；jsonutil提供merge而非Config模型。
endpoint的config_endpoint_test.rs覆盖泛型存储，并未测试服务字段默认。
ratelimit已有ControllerOption::Dimensions（controller.rs）与限流器测试，但缺Go DimensionConfig对应codec。
server/config无Rust生产或测试文件：所有服务行为T2–T16尚无本包覆盖，原因是尚未实施，
外部嵌套则受拥有包准入限制；不把工具函数绿灯扩大解释为完整Config已对齐。

未来T16真实字节/JSON/TOML差分必须复用既有只读fixtures/输出/安全既有工具；
本轮只读测试断言与源码推导不冒充实际Go输出。缺输出即记录阻塞，禁止新增Go oracle。

### 本轮验证与审查

实际运行的符号搜索（退出0）：

```sh
rg -n '^(func |type |pub |    pub )' server/config/config.go server/config/service_middleware_config.go server/config/persist_options.go
```

本任务为文档盘点，不适用行为红绿灯、不调用Cargo/Go测试、make、failpoint、
生成器、formatter或依赖命令，不占构建槽位（不适用）。

| 检查 | 结果 |
| --- | --- |
| 任务指定符号搜索命令（见上方原文），作用于三份Go来源 | 退出0；全部func/type另以完整行扫描生成索引，包含多行签名首行 |
| 三份来源相对规划基准的git diff | 退出0，无差异 |
| 文档字段/符号清单对真实来源逐项复核 | 113个struct字段、204个func/type声明（其中184个公开）全部有归属；另列5个公开常量，无未归属占位项 |
| Markdown相对链接及源码路径存在性检查 | 退出0；实际路径存在；未来Rust目标明确标注拟建 |
| `git diff --check`及本任务暂存差异检查 | 退出0 |
| 开始/结束Go与模块文件路径集合、SHA-256及只读plan指纹 | 相同；Go/模块Git状态为空；无生产/测试/配置代码变化 |

审查仅新增本文，任务完成后删除编号文件；其他会话遗留的计划删除保持原样且不纳入提交。
本任务完成不放宽T15外部准入，也不关闭来源主题。后续依赖可据本文与本次提交核验
批次83完成，不能据此认定T2–T16实现已完成。


## 批次 84：基础标量序列化入口

真实 Go 输出取自仓库既有 `bin/pd-server`，SHA-256
`b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`。
`go version -m bin/pd-server`：Go 构建、without_dashboard、darwin/amd64，
`--version` 的 Git hash 为 None，故仅将此二进制作为带哈希的既有 oracle，
不声称来自当前 HEAD。只读源码确认对应日志位于 server/server.go:268
`CreateServer` 的 `zap.Reflect("config", cfg)`。临时 TOML 经 Config.Parse/Adjust
后打印真实 JSON；这是调整后的标量编码证据，原始 presence 根据输入键取得，
不把已调整的零值当作原始输入。临时地址采用 loopback 端口 0；取得日志后，3 秒超时触发 terminate，
进程经取消启动清理并退出 1（不是配置检查失败）；
子进程已等待结束并删除临时目录，不验证服务启动，不写 Go/模块，不启停 failpoint。
缺失与显式零值的 Adjust 结果只作 oracle 差异说明，本任务不实现 Adjust。

运行方式：Python tempfile.TemporaryDirectory 内写以下 input.toml，
`subprocess.Popen([absolute_binary, "--config", absolute_input], cwd=tempdir,
stdout=PIPE, stderr=STDOUT)`，communicate(timeout=3)，超时 terminate 并等待，
再超时 kill 并等待；保留 message=PD config 的原始 JSON 行，无重编码。
二进制配置检查入口只输出检查结果；进一步检查发现日志入口可取得编码证据，
因此最初的证据缺口已解除。

### Go oracle missing

输入 TOML（临时绝对 data-dir 原样保留用于来源核验）：

<!-- batch84-input-missing -->

```toml
name = "codec-oracle"
data-dir = "/var/folders/gr/w4kj8kxx1v53r3p8ycyv4s3c0000gn/T/pd-config84-lba0c113/data"
client-urls = "http://127.0.0.1:0"
peer-urls = "http://127.0.0.1:0"
[log]
format = "json"
```

原始 Go JSON 日志（退出 1）：

<!-- batch84-missing -->

```json
{"level":"INFO","time":"2026/10/24 01:27:12.830 +08:00","caller":"server.go:268","message":"PD config","enable-keyspace-group":false,"config":{"client-urls":"http://127.0.0.1:0","peer-urls":"http://127.0.0.1:0","advertise-client-urls":"http://127.0.0.1:0","advertise-peer-urls":"http://127.0.0.1:0","name":"codec-oracle","data-dir":"/var/folders/gr/w4kj8kxx1v53r3p8ycyv4s3c0000gn/T/pd-config84-lba0c113/data","force-new-cluster":false,"enable-grpc-gateway":true,"initial-cluster":"codec-oracle=http://127.0.0.1:0","initial-cluster-state":"new","initial-cluster-token":"pd-cluster","join":"","lease":5,"log":{"level":"info","format":"json","disable-timestamp":false,"file":{"filename":"","max-size":0,"max-days":0,"max-backups":0},"development":false,"disable-caller":false,"disable-stacktrace":false,"disable-error-verbose":true,"sampling":null,"error-output-path":""},"max-concurrent-tso-proxy-streamings":5000,"tso-proxy-recv-from-client-timeout":"1h0m0s","tso-save-interval":"5s","tso-update-physical-interval":"50ms","enable-local-tso":false,"metric":{"job":"codec-oracle","address":"","interval":"15s"},"schedule":{"max-snapshot-count":64,"max-pending-peer-count":64,"max-merge-region-size":54,"max-merge-region-keys":0,"split-merge-interval":"1h0m0s","switch-witness-interval":"1h0m0s","enable-one-way-merge":"false","enable-cross-table-merge":"true","patrol-region-interval":"10ms","max-store-down-time":"30m0s","max-store-preparing-time":"48h0m0s","leader-schedule-limit":4,"leader-schedule-policy":"count","region-schedule-limit":2048,"witness-schedule-limit":4,"replica-schedule-limit":64,"merge-schedule-limit":8,"hot-region-schedule-limit":4,"affinity-schedule-limit":0,"split-scatter-schedule-limit":4,"hot-region-cache-hits-threshold":3,"default-store-limit":{"add-peer":15,"remove-peer":15,"transfer-leader-in":100000000},"store-limit":{},"tolerant-size-ratio":0,"low-space-ratio":0.8,"high-space-ratio":0.7,"region-score-formula-version":"v2","scheduler-max-waiting-operator":5,"enable-remove-down-replica":"true","enable-replace-offline-replica":"true","enable-make-up-replica":"true","enable-remove-extra-replica":"true","enable-location-replacement":"true","enable-debug-metrics":"false","enable-joint-consensus":"true","enable-tikv-split-region":"true","enable-heartbeat-breakdown-metrics":"true","enable-heartbeat-concurrent-runner":"true","schedulers-v2":[{"type":"balance-region","args":null,"disable":false,"args-payload":""},{"type":"balance-leader","args":null,"disable":false,"args-payload":""},{"type":"hot-region","args":null,"disable":false,"args-payload":""},{"type":"evict-slow-store","args":null,"disable":false,"args-payload":""},{"type":"evict-stopping-store","args":null,"disable":false,"args-payload":""}],"hot-regions-write-interval":"10m0s","hot-regions-reserved-days":7,"max-movable-hot-peer-size":512,"enable-diagnostic":"true","enable-witness":"false","slow-store-evicting-affected-store-ratio-threshold":0.3,"store-limit-version":"v1","patrol-region-worker-count":1,"max-affinity-merge-region-size":256},"replication":{"max-replicas":3,"location-labels":"","strictly-match-label":"false","enable-placement-rules":"true","enable-placement-rules-cache":"false","isolation-level":""},"pd-server":{"use-region-storage":"true","max-gap-reset-ts":"24h0m0s","key-type":"table","runtime-services":"","metric-storage":"","dashboard-address":"auto","flow-round-by-digit":3,"min-resolved-ts-persistence-interval":"1s","server-memory-limit":0,"server-memory-limit-gc-trigger":0.7,"enable-gogc-tuner":"true","gc-tuner-threshold":0.6,"block-safe-point-v1":"false","gc-keyspace-index-load-concurrency":4,"gc-state-cache-warmup-concurrency":4},"cluster-version":"0.0.0","labels":{},"quota-backend-bytes":"8GiB","auto-compaction-mode":"periodic","auto-compaction-retention-v2":"1h","tick-interval":"500ms","election-interval":"3s","enable-prevote":true,"max-request-bytes":157286400,"security":{"cacert-path":"","cert-path":"","key-path":"","cert-allowed-cn":null,"SSLCABytes":null,"SSLCertBytes":null,"SSLKEYBytes":null,"redact-info-log":false,"encryption":{"data-encryption-method":"plaintext","data-key-rotation-period":"168h0m0s","master-key":{"type":"plaintext","key-id":"","region":"","endpoint":"","path":""}}},"label-property":null,"dashboard":{"tidb-cacert-path":"","tidb-cert-path":"","tidb-key-path":"","public-path-prefix":"","temp-dir":"","internal-proxy":false,"enable-telemetry":false,"enable-experimental":false,"disable-custom-prom-addr":false},"replication-mode":{"replication-mode":"majority","dr-auto-sync":{"label-key":"","primary":"","dr":"","primary-replicas":0,"dr-replicas":0,"wait-store-timeout":"1m0s","wait-recover-timeout":"0s","pause-region-split":"false"}},"keyspace":{"pre-alloc":null,"wait-region-split":true,"wait-region-split-timeout":"30s","check-region-split-interval":"50ms","meta-service-groups":null},"micro-service":{"enable-scheduling-fallback":"true","enable-tso-dynamic-switching":"false","enable-resource-manager-fallback":"true"},"controller":{"degraded-mode-wait-duration":"0s","ltb-max-wait-duration":"30s","ltb-token-rpc-max-delay":"1s","request-unit":{"read-base-cost":0.125,"read-per-batch-base-cost":0.5,"read-cost-per-byte":0.0000152587890625,"write-base-cost":1,"write-per-batch-base-cost":1,"write-cost-per-byte":0.0009765625,"read-cpu-ms-cost":0.3333333333333333},"enable-controller-trace-log":"false","push-metrics-address":"","push-metrics-interval":"0s"},"metering":{}}}
```

### Go oracle explicit-zero

输入 TOML（临时绝对 data-dir 原样保留用于来源核验）：

<!-- batch84-input-explicit-zero -->

```toml
name = "codec-oracle"
data-dir = "/var/folders/gr/w4kj8kxx1v53r3p8ycyv4s3c0000gn/T/pd-config84-koo493hj/data"
client-urls = "http://127.0.0.1:0"
peer-urls = "http://127.0.0.1:0"
EnableGRPCGateway = false
enable-prevote = false
max-request-bytes = 0
quota-backend-bytes = "0B"
auto-compaction-retention = "2h"
max-concurrent-tso-proxy-streamings = -1
lease = -1
[log]
format = "json"
```

原始 Go JSON 日志（退出 1）：

<!-- batch84-explicit-zero -->

```json
{"level":"INFO","time":"2026/10/24 01:27:15.716 +08:00","caller":"server.go:268","message":"PD config","enable-keyspace-group":false,"config":{"client-urls":"http://127.0.0.1:0","peer-urls":"http://127.0.0.1:0","advertise-client-urls":"http://127.0.0.1:0","advertise-peer-urls":"http://127.0.0.1:0","name":"codec-oracle","data-dir":"/var/folders/gr/w4kj8kxx1v53r3p8ycyv4s3c0000gn/T/pd-config84-koo493hj/data","force-new-cluster":false,"enable-grpc-gateway":true,"initial-cluster":"codec-oracle=http://127.0.0.1:0","initial-cluster-state":"new","initial-cluster-token":"pd-cluster","join":"","lease":-1,"log":{"level":"info","format":"json","disable-timestamp":false,"file":{"filename":"","max-size":0,"max-days":0,"max-backups":0},"development":false,"disable-caller":false,"disable-stacktrace":false,"disable-error-verbose":true,"sampling":null,"error-output-path":""},"max-concurrent-tso-proxy-streamings":-1,"tso-proxy-recv-from-client-timeout":"1h0m0s","tso-save-interval":"5s","tso-update-physical-interval":"50ms","enable-local-tso":false,"metric":{"job":"codec-oracle","address":"","interval":"15s"},"schedule":{"max-snapshot-count":64,"max-pending-peer-count":64,"max-merge-region-size":54,"max-merge-region-keys":0,"split-merge-interval":"1h0m0s","switch-witness-interval":"1h0m0s","enable-one-way-merge":"false","enable-cross-table-merge":"true","patrol-region-interval":"10ms","max-store-down-time":"30m0s","max-store-preparing-time":"48h0m0s","leader-schedule-limit":4,"leader-schedule-policy":"count","region-schedule-limit":2048,"witness-schedule-limit":4,"replica-schedule-limit":64,"merge-schedule-limit":8,"hot-region-schedule-limit":4,"affinity-schedule-limit":0,"split-scatter-schedule-limit":4,"hot-region-cache-hits-threshold":3,"default-store-limit":{"add-peer":15,"remove-peer":15,"transfer-leader-in":100000000},"store-limit":{},"tolerant-size-ratio":0,"low-space-ratio":0.8,"high-space-ratio":0.7,"region-score-formula-version":"v2","scheduler-max-waiting-operator":5,"enable-remove-down-replica":"true","enable-replace-offline-replica":"true","enable-make-up-replica":"true","enable-remove-extra-replica":"true","enable-location-replacement":"true","enable-debug-metrics":"false","enable-joint-consensus":"true","enable-tikv-split-region":"true","enable-heartbeat-breakdown-metrics":"true","enable-heartbeat-concurrent-runner":"true","schedulers-v2":[{"type":"balance-region","args":null,"disable":false,"args-payload":""},{"type":"balance-leader","args":null,"disable":false,"args-payload":""},{"type":"hot-region","args":null,"disable":false,"args-payload":""},{"type":"evict-slow-store","args":null,"disable":false,"args-payload":""},{"type":"evict-stopping-store","args":null,"disable":false,"args-payload":""}],"hot-regions-write-interval":"10m0s","hot-regions-reserved-days":7,"max-movable-hot-peer-size":512,"enable-diagnostic":"true","enable-witness":"false","slow-store-evicting-affected-store-ratio-threshold":0.3,"store-limit-version":"v1","patrol-region-worker-count":1,"max-affinity-merge-region-size":256},"replication":{"max-replicas":3,"location-labels":"","strictly-match-label":"false","enable-placement-rules":"true","enable-placement-rules-cache":"false","isolation-level":""},"pd-server":{"use-region-storage":"true","max-gap-reset-ts":"24h0m0s","key-type":"table","runtime-services":"","metric-storage":"","dashboard-address":"auto","flow-round-by-digit":3,"min-resolved-ts-persistence-interval":"1s","server-memory-limit":0,"server-memory-limit-gc-trigger":0.7,"enable-gogc-tuner":"true","gc-tuner-threshold":0.6,"block-safe-point-v1":"false","gc-keyspace-index-load-concurrency":4,"gc-state-cache-warmup-concurrency":4},"cluster-version":"0.0.0","labels":{},"quota-backend-bytes":"0B","auto-compaction-mode":"periodic","auto-compaction-retention-v2":"2h","tick-interval":"500ms","election-interval":"3s","enable-prevote":false,"max-request-bytes":0,"security":{"cacert-path":"","cert-path":"","key-path":"","cert-allowed-cn":null,"SSLCABytes":null,"SSLCertBytes":null,"SSLKEYBytes":null,"redact-info-log":false,"encryption":{"data-encryption-method":"plaintext","data-key-rotation-period":"168h0m0s","master-key":{"type":"plaintext","key-id":"","region":"","endpoint":"","path":""}}},"label-property":null,"dashboard":{"tidb-cacert-path":"","tidb-cert-path":"","tidb-key-path":"","public-path-prefix":"","temp-dir":"","internal-proxy":false,"enable-telemetry":false,"enable-experimental":false,"disable-custom-prom-addr":false},"replication-mode":{"replication-mode":"majority","dr-auto-sync":{"label-key":"","primary":"","dr":"","primary-replicas":0,"dr-replicas":0,"wait-store-timeout":"1m0s","wait-recover-timeout":"0s","pause-region-split":"false"}},"keyspace":{"pre-alloc":null,"wait-region-split":true,"wait-region-split-timeout":"30s","check-region-split-interval":"50ms","meta-service-groups":null},"micro-service":{"enable-scheduling-fallback":"true","enable-tso-dynamic-switching":"false","enable-resource-manager-fallback":"true"},"controller":{"degraded-mode-wait-duration":"0s","ltb-max-wait-duration":"30s","ltb-token-rpc-max-delay":"1s","request-unit":{"read-base-cost":0.125,"read-per-batch-base-cost":0.5,"read-cost-per-byte":0.0000152587890625,"write-base-cost":1,"write-per-batch-base-cost":1,"write-cost-per-byte":0.0009765625,"read-cpu-ms-cost":0.3333333333333333},"enable-controller-trace-log":"false","push-metrics-address":"","push-metrics-interval":"0s"},"metering":{}}}
```

### 实现与证据边界

canonical crate `astersql-pd-server-config`（server/config/Cargo.toml、lib.rs）新增
`ConfigScalars`、`DecodedScalars`、`decode_json_scalars`、`decode_toml_scalars`，
22 个基础字段：四类 URL、name/data-dir、force-new-cluster/enable-grpc-gateway、
initial-cluster/state/token、join、lease、log-file/log-level、TSO proxy 并发、
enable-local-tso、quota-backend-bytes、auto-compaction-mode/retention、prevote、
max-request-bytes。复用 ByteSize 和 ConfigMetaData；默认是解码前 Go 零值，
不调用 Adjust、不预填默认值。原始键的存在性独立于 false/0/空串，普通标量 null 保持零值；
ByteSize 自定义解码依据 size.go::UnmarshalJSON 拒绝 null，
TOML 的 EnableGRPCGateway/ForceNewCluster 无显式 tag，retention 的 TOML 与
JSON `-v2` 分别映射；JSON 使用原公开 tag，废弃空字符串保持 omitempty。
JSON 接受对象，类型错误返回 serde_json::Error；TOML 语法/类型错误返回
 toml::de::Error，不承诺错误文本逐字等同 Go。新增入口明确是标量投影，
未知及尚未实现的嵌套字段报告 undecoded，不储存透传 JSON、不提供完整 Config
校验；Duration、maps、外部嵌套装配及 TOML 输出编码仍不在本任务实现范围。

注意 Go 的真实细节：Config 中 EnableGRPCGateway 无 TOML tag，而 Adjust
在 config.go:462 检查 `enable-grpc-gateway`。因此第二组原始输入
`EnableGRPCGateway=false` 的解码值为 false，但已调整日志为 true；测试
分别断言这两个阶段，未把它误解为解码器应填 true。其他已显式给出的
prevote/max-request-bytes/quota/retention/lease/proxy 并发直接与 Go 日志比对。
Go JSON fixture 字节测试按字段从原始日志截取 token，比较 Rust 序列化字节，
不是将 Rust 结果复制为预期，也不将字段顺序差异当成完整 JSON 文本一致。

TOML 依赖在 workspace 集中精确锁定 `=0.9.8`，默认 features；官方
[crate 文档](https://docs.rs/toml/0.9.8/toml/)确认 serde 1.0 兼容，
[官方 manifest](https://docs.rs/crate/toml/0.9.8/source/Cargo.toml)声明
rust-version=1.76，可被现有 Rust 1.99 工具链消费。锁文件仅增加本 crate、
toml 与所需 serde_spanned/toml_datetime/toml_parser/toml_writer/winnow，
未升级已有 package 的版本。

### 本轮验证

所有 Cargo 阶段通过原子 mkdir 动态领取槽位 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，
每阶段 owner 文件记录计划/任务/会话/PID，阶段末 EXIT trap 删除 owner 并 rmdir；
缓存保留。命令从仓库根执行，没有 --target-dir 覆盖。workspace 测试继承
`GOFLAGS=-mod=readonly`；已审查既有 etcd_fixture_test.rs 的 Go build 和
scripts/rust-etcd-fixture/main.go：只构建已有 Go 工具，产物写在所持槽位、
fixture 数据写独立目录，无 generator/failpoint 注入或源码写入。

| 检查（确切命令） | 退出码与有效证据 |
| --- | --- |
| `cargo fmt --all` | 0，仅本任务 Rust 文件格式化 |
| `cargo check --workspace` | 0，同步新增依赖的 Cargo.lock 后审查，无既有依赖版本变化 |
| 首次辅助编译 `cargo test --locked -p astersql-pd-server-config --lib config_decode -- --nocapture` | 101，修正测试误用 Option::unwrap_err；不计行为红灯 |
| 行为红灯：`cargo test --locked -p astersql-pd-server-config --lib config_decode -- --nocapture` | 101，0 passed / 4 failed；真实断言分别命中 presence、TOML tag、JSON 非对象校验、Go 字节场景的 presence |
| 绿灯：相同聚焦命令 | 0，原四项全绿；补充 Go TOML 输入对照后最终 5 passed / 0 failed / 0 ignored |
| `go test -mod=readonly ./server/config -count=1` | 0，包通过，1.122s；只读审查所有该包测试未调用 failpoint，TestMain 仅包装测试与 goleak/etcd 连接清理，安全复用已有测试，未新增 Go oracle |
| `cargo metadata --format-version 1 --no-deps` | 0，新 canonical crate 在 workspace 中，无额外 features |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；既有 core_region_adapter.rs:134 的 deprecated cpu_usage 警告保留 |
| `cargo test --workspace --locked` | 0，675 passed / 0 failed / 1 既有 ignored；包括真实 etcd 场景，所有子进程结束后释放槽位 |
| `git diff --check` | 0 |

本任务五个独立测试在 config_decode_test.rs，lib.rs 显式注册；新增入口均被
聚焦过滤命中。开始/结束 1101 个 Go/模块文件的路径集合与 SHA-256 相同，
Go/模块 Git 状态为空，plan.md 指纹不变；无 Go 文件或模块差异。任务 2
范围完成，后续默认值/URL 校验/完整嵌套装配仍需各自验收，不关闭来源主题。

最终源码审查补充 ByteSize 的自定义 null 分支：已有 size.go::UnmarshalJSON
调用 strconv.Unquote，因此 quota-backend-bytes:null 必须报错，不能像普通
标量一样忽略。原五项中的类型错误测试补充此断言，聚焦命令实际红灯
4 passed / 1 failed（退出101），修正后以同一断言运行全部最终基线。
最终重跑结果：聚焦命令 5 passed / 0 failed / 0 ignored，metadata、fmt check、
workspace check 均退出 0，workspace test 退出 0（675 passed / 0 failed /
1 既有 ignored）；槽位 1 锁已释放，缓存保留。Go/模块及只读计划指纹再次一致。

## 批次 85：连接地址值校验

日期：2026-10-24（Asia/Shanghai）。前置批次 84 已由提交 `375a3aab4`
及本文上一节核验：canonical crate、五项独立测试、675 passed / 0 failed /
1 既有 ignored 的 workspace 证据齐备。本轮三份 Go 来源相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无变化；没有因前序任务文件
被删除而推定完成。适用根 AGENTS.md，无更深规则；已读 Copilot、ADR-001、
移植指南与执行技能，plan.md 和来源主题只读。

### Go 依据与实现边界

- `server/config/util.go::ValidateURLWithScheme` 调用 `url.ParseRequestURI`，
  然后同时要求 Scheme、Host 非空，否则返回 `<raw> has no scheme`。
  `util_test.go::TestValidateURLWithScheme` 的 14 个表项原样映射到 Rust 测试，
  包括空串、相对地址、空 host、HTTP/HTTPS、路径、IPv6、重复端口。
- `util.go::parseUrls` 使用 `strings.Split(s, ",")`，逐项 `url.Parse`，
  不 trim、不限制 HTTP(S)，不强制每项有 scheme/host，首个失败包装
  `errs.ErrURLParse`（`PD:url:ErrURLParse`）。因此 `""`、`,`, `foo`、
  `/foo`、`http://`、`mailto:pd@example.com` 等可解析值不能被擅自拒绝。
- `config.go::Adjust` 的 join 分支只在非空时调用 parseUrls，错误保留
  `failed to parse join addr:<raw>, err:<cause>`；
  `Config.Validate` 禁止 join 与 initial-cluster 同时非空，保留其错误文本。
  `config_test.go::TestBadFormatJoinAddr` 和 `TestJoinAddr` 的六组输入
  （单项、多项、mixed schemes、service endpoints、坏首项/坏第二项）均覆盖。
- 额外边界来自本机 Go **1.26.8** 标准库 `net/url/url.go` 的 `parse`、
  `parseHost`、`unescape`、`getScheme`、`validUserinfo` 以及
  `encoding_table.go`：HTTP(S) 严格拒绝未括起的多冒号 host，非 HTTP
  scheme 保留最后冒号为端口的规则；端口仅检查数字，不做 u16 范围限制；
  IPv6/zone、host/userinfo 字符、path/fragment 的百分号转义与控制字符均校验。
  query 不检查百分号转义，opaque URI 不按 path 解码。
- `ParseRequestURI` 不拆 fragment，`http://pd/path#%zz` 中 `%zz` 是无效路径
  转义，必须报错。初始附加测试把它误列为成功，本轮源码复核后移到失败表，
  保留该断言并非移除场景；`http://pd/path?q=%zz` 仍成功。
  原四测试红灯全部来自真实缺失校验；该附加预期修正另行列出，不冒充实现红灯。

新增 `config_urls.rs` 公开 `validate_url_with_scheme`、`validate_url_list`、
`ConfigScalars::validate_join` 和 `ConfigUrlError`，仅校验纯值，不构造 URL
传输对象、不调整默认值、不连接网络、不调用完整 Config.Validate 的文件系统逻辑。
`validate_url_list` 是 Go parseUrls 的**校验投影**，不是返回完整 Go URL 字段的
替代 API。错误保留 typed cause、PD 稳定错误码、join 包装及缺 scheme/冲突文本；
底层诊断不承诺所有非法 IPv6/netip 错误及控制字符引用逐字等同 Go。
复用已有 `astersql-pd-errs`，Cargo.lock 仅在 server-config 的依赖列表加一行，
未新增或升级第三方包。`config_urls_test.rs` 的四项独立测试在 lib.rs 显式注册，
均由 config_urls 过滤命中。纯值任务无需 MemoryStore/etcd 双入口。

现有 Go 配置测试没有 failpoint 调用，TestMain 只包装 goleak 与 etcd 连接清理；
复用批次 84 的调用链审查后重新运行整个包，未启停 failpoint、未新增 Go oracle。
上述原始表项同时有本轮 Go/Rust 测试运行证据；额外标准库边界明确是**源码推导**，
没有将其冒充新增 Go 运行差分。本任务没有编码输出，故无需新字节 fixture。

Go 来源 SHA-256：

| 文件 | SHA-256 |
| --- | --- |
| util.go | `503276291dee3b7811b0dad72b7a1b95e3a8cc76a79fd3b208ec952072c139fc` |
| util_test.go | `c9d87606ed8d565288206e89058375e227da65036810bfa53d67373b91b039d0` |
| config_test.go | `705c57ffdbd953a5e9c274a1fe7cc31db8b6ebdbf31e4493f798242a245a847f` |

### 本轮验证

所有 Cargo 阶段从仓库根通过原子 mkdir 动态领取槽位 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录计划/任务/会话/PID，EXIT/INT/TERM 覆盖锁清理；所有子进程完成后
释放自己的锁并保留缓存，没有改动其他锁。workspace 测试继承
`GOFLAGS=-mod=readonly`，复核既有 `pkg/storage/kv/etcd_fixture_test.rs` 和
`scripts/rust-etcd-fixture/main.go`，只构建现有 Go fixture 到槽位并写隔离数据目录。

| 确切命令 | 退出码与有效证据 |
| --- | --- |
| `cargo fmt --all` | 0，仅格式化本任务 Rust 文件 |
| 行为红灯：`cargo test --locked -p astersql-pd-server-config --lib config_urls -- --nocapture` | 101；0 passed / 4 failed / 0 ignored，缺失校验触发真实断言 |
| `cargo check --workspace` | 0；同步已存在 errs 依赖后审查锁文件 |
| 首次实现聚焦检查（相同命令） | 101；3 passed / 1 failed，定位上述附加 RequestURI 预期错误 |
| 最终绿灯：相同聚焦命令 | 0；4 passed / 0 failed / 0 ignored |
| `go test -mod=readonly ./server/config -count=1` | 0；包通过，1.159s；包括现有 URL/join 表项 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；既有 core_region_adapter.rs:134 deprecated cpu_usage 警告 |
| `cargo test --workspace --locked` | 0；679 passed / 0 failed / 1 既有 ignored，包含真实 etcd 回归 |
| `git diff --check` | 0 |

本轮状态：**已完成**。全量测试完成后复核 1101 个 Go/模块文件路径集合及
SHA-256 与任务开始完全一致，Go/模块 Git 状态为空；plan.md 指纹不变。
只有本任务 Rust、最小依赖连线及本文证据发生变化，未格式化无关 Rust 文件。
槽位 1 的 owner 与锁已释放，编译缓存保留；原有其他任务文件删除不纳入提交。

## 批次 86：基础标量默认值

日期：2026-10-24（Asia/Shanghai）。核验前置批次 85 提交 `a9648256f`、
canonical `astersql-pd-server-config` 的 URL 实现与四项独立测试，以及上一节
679 passed / 0 failed / 1 ignored 的 workspace 证据，准入满足。
`config.go` 相对规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223`
无差异。已读根 AGENTS.md、Copilot、ADR-001、移植指南、执行技能；server/docs
没有更深 AGENTS.md。本任务仅默认值纯值投影，不涉及完整 Config 的 Validate、
外部嵌套配置、TSO 归一化、启动或存储装配。

### Go 依据与实现

`server/config/config.go::Config.Adjust`（358–465）和常量（177–223）：

- 空 name 取 `pd-<hostname>`，空 data-dir 取 `default.<name>` 后由
  `configutil.AdjustPath` 做 filepath.Abs/Clean；Rust 的
  `ConfigDefaultsContext` 由宿主提供已成功解析的 hostname 与绝对 cwd，库内
  不读取宿主状态、不访问文件系统。hostname/cwd 错误由宿主处理，不用虚构值掩盖。
  edition 不参与本任务字段默认值。路径按宿主路径语法做词法清理，不解析符号链接。
- ClientUrls/PeerUrls 空值分别取 2379/2380 loopback URL，空 advertise 值继承；
  空 InitialCluster 逐个分割 advertise-peer-urls，原样保留空项/空白，拼为
  `name=url`。已有 initial cluster、state、token 保留；空 state/token 取 new/pd-cluster。
- LeaderLease 仅零取 5，负数保留；AutoCompactionMode/Retention 空取 periodic/1h；
  Labels 的 nil 变空 map，已有 map 保留。
- quota、max request **仅缺失 metadata 时无条件**取 8GiB、150MiB；显式零值保留。
  prevote/gateway 缺失置 true，显式 false 保留。metadata 按字面 key 区分大小写；
  Go gateway 无 TOML tag，`EnableGRPCGateway=false` 不代表 metadata 中定义了
  `enable-grpc-gateway`，因此仍被 Adjust 设 true。
- Tick/Election 的非正 duration 分别取 500ms/3s；HeartbeatStreamBindInterval、
  LeaderPriorityCheckInterval 的非正值取 1min；正数（包括 1ns/i64::MAX）保留。
  后两字段只接受原有 Go TOML 字段名，JSON 隐藏。
- 相邻 `config_test.go::TestAdjust` 固定空 name、lease=0、显式 max request
  20000000 与缺失 prevote=true；`TestValidation` 固定 quota/max request 默认。
  本轮只读运行现有整个 Go 配置测试包。源码还确认 TestMain 仅 goleak/etcd 连接清理，
  测试没有 failpoint.Enable；persist_options 的注入点不是这些测试的必要条件。
  未启停 failpoint、未新增 Go oracle。额外边界（负 lease、字面 metadata、duration
  极值、路径清理）明确来自源码推导，不冒充新增 Go 运行差分。

`ConfigScalars::adjust_base_defaults` 位于 config_defaults.rs；在现有 canonical
ConfigScalars 原位增加本任务所属的四项 duration 和 labels，未创建平行 Config。
JSON/TOML 解码扩展仍位于 config_decode.rs；Duration null 按
`pkg/utils/typeutil/duration.go::UnmarshalJSON` 的 strconv.Unquote 错误语义拒绝。
原 `config_decode_go_json_bytes` 测试自动纳入新 JSON 字段 tick-interval、
election-interval、labels：复用上文 batch84 两份**真实已有 Go 输出**，逐字段
比较 JSON 值及原始 wire bytes，没有手抄 Rust 输出充当 Go fixture。
config_defaults_test.rs 独立测试在 lib.rs 显式注册，纯值任务无需 MemoryStore/etcd 入口。
无新增依赖，Cargo.toml/Cargo.lock 不变。

### 本轮验证

Cargo 每阶段通过原子 mkdir 动态领取共享槽位 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 写入计划、任务、会话及持有 shell PID；EXIT/INT/TERM 清理自己的记录与锁，
阶段内子进程全部结束后释放，保留缓存。所有 Cargo 子进程继承该绝对路径；
workspace 测试继承 `GOFLAGS=-mod=readonly`。已复核既有 etcd fixture 调用链：
只读构建现有 Go 源码，二进制及隔离数据目录写入槽位，不写 Go/模块文件。

初始 6 项独立测试在最小可编译接口下执行指定聚焦命令，得到真实行为红灯
**0 passed / 6 failed / 0 ignored，退出 101**；失败来自实际默认值/保留值/
时长/地址断言，不是缺符号或环境错误。最小实现后同样 6 项全部通过，退出 0。
随后补充第 7 项 TOML duration/labels、隐藏 JSON 字段和 Duration null 边界，
原六项断言未削弱，最终按修改后的源码重新运行完整基线。

补充测试首次编译误将 `check_undecoded()` 的 Option 当作 Result 使用
（E0599，退出 101）；修正为 `is_none()` 后重新格式化及验证。这是辅助编译
记录，不计作行为红灯。初版完整 workspace 为 685 passed / 0 failed /
1 既有 ignored；最终源码含第七项测试，以下记录采用最终重跑结果。

| 确切命令 | 最终退出码与有效证据 |
| --- | --- |
| `cargo fmt --all` | 0；只改本任务 Rust 文件 |
| `cargo test --locked -p astersql-pd-server-config --lib config_defaults -- --nocapture` | 0；7 passed / 0 failed / 0 ignored |
| `go test -mod=readonly ./server/config -count=1` | 0；现有配置包通过，1.164s |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；只有既有 core_region_adapter.rs:134 deprecated cpu_usage 警告 |
| `cargo test --workspace --locked` | 0；686 passed / 0 failed / 1 既有 ignored，含真实 etcd 回归；配置 crate 16 项全部通过 |
| `git diff --check` | 0 |

本轮状态：**已完成**。最终全量回归后重新比较 1101 个 Go/模块文件的路径集合与
SHA-256，完全一致；Go/模块 Git 状态为空。只读 plan.md 的 SHA-256 仍为
`c7968f95a13d0d216ced3adafaca3140ad6ef959ff693c613cb903b231a6f777`。
已审查默认值实现、七项测试、canonical 解码连线与本文差异，无无关 Rust 格式化、
无依赖变更。共享槽位 1 的 owner 与锁已清理，缓存保留。其他任务文件的已有删除
不纳入提交；本任务完成不代表完整 Config 或来源主题已完成。

## 批次 87：时间戳配置归一化

日期：2026-10-24（Asia/Shanghai）。前置批次 86 经提交 `fae54ad30`、
ConfigScalars 既有实现和本文上一节 686 passed / 0 failed / 1 ignored
证据核验，准入满足。`server/config/config.go` 相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。只适用根 AGENTS.md；
已读执行技能、Copilot、ADR-001、移植指南；plan.md 和来源主题保持只读。

### Go 依据与实现边界

- `config.go` 常量（229–238）及 `Config.Adjust`（409–419）、
  `pkg/utils/configutil/configutil.go::AdjustInt/AdjustDuration`：proxy 并发数
  仅零取 5000，负数保留无限语义；接收超时非正取 1h；保存间隔非正取
  固定 5s，与用户自定义 LeaderLease 无关；物理更新间隔先对非正值取
  50ms，再钳制到闭区间 [1ms,10s]，合法正值保留。
- 相邻 `config_test.go::TestAdjust` 的默认值、500ns→1ms、15s→10s
  断言本轮通过现有只读 Go 包测试复核。负时长、i64 极值、自定义 lease
  和 proxy 负数边界为源码推导，不冒充新增 Go 运行差分。
- 已复核现有 Go TestMain 的 goleak/WaitForEtcdConnections 包装和配置测试：
  不要求 failpoint 注入，不调用生成器。运行 `go test -mod=readonly`，
  未新增、改写 Go oracle，未启停 failpoint。

`config_tso.rs::ConfigScalars::adjust_tso_defaults` 只调整上述四项纯值，
不安装日志、不启动服务、不调整其他字段。沿用既有 canonical ConfigScalars，
在 `config_decode.rs` 原位补齐三项 TSO Duration 的 JSON/TOML tag、解码及
null 错误语义，这是同一配置类型的必要连线，不创建平行配置实现。
五项独立 `config_tso_*` 测试置于 config_tso_test.rs 并在 lib.rs 显式注册，
覆盖默认值、负值、上下限及相邻值、合法值、幂等、lease 独立性、JSON/TOML
输入和 duration 类型错误。纯值任务无需 MemoryStore/etcd 测试入口。
无新增依赖，Cargo.toml/Cargo.lock 不变，无无关 Rust 格式化。

扩展字段自动纳入既有 `config_decode_go_json_bytes` 的逐字段值及原始 wire
bytes 比对，复用本文 batch84-missing / batch84-explicit-zero 两份**真实已有
Go JSON 输出**，未手抄 Rust 输出作为预期。该既有测试本轮 workspace 回归通过。

### 本轮验证

红灯、绿灯各阶段均通过原子 mkdir 动态取得共享槽位 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录计划、任务、会话与持有 shell PID，EXIT/INT/TERM 清理自己的锁；
所有 Cargo 和 fixture 子进程结束后释放，缓存保留。所有子进程继承该绝对路径
及 `GOFLAGS=-mod=readonly`；已复核既有 etcd fixture 调用链仅构建既有 Go
源码，二进制及隔离数据目录写入槽位，不改 Go/模块。

先补齐可编译字段和空调整接口，运行以下聚焦命令得到真实行为红灯
**0 passed / 5 failed / 0 ignored，退出 101**，失败来自实际时长/并发数断言。
最小实现后相同五项测试不削弱断言、不忽略用例，全部通过。

| 确切命令 | 退出码与有效证据 |
| --- | --- |
| `cargo fmt --all` | 0；红灯与绿灯阶段均先格式化 |
| `cargo test --locked -p astersql-pd-server-config --lib config_tso -- --nocapture` | 红灯 101，0 passed / 5 failed；绿灯 0，5 passed / 0 failed / 0 ignored |
| `go test -mod=readonly ./server/config -count=1` | 0；现有配置包通过，1.161s |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 core_region_adapter.rs:134 deprecated cpu_usage 警告 |
| `cargo test --workspace --locked` | 0；691 passed / 0 failed / 1 既有 ignored，含真实 etcd 回归；配置 crate 21 项全部通过 |
| `git diff --check` | 0 |

本轮状态：**已完成**。完成后复核 1101 个 Go/模块文件路径集合及 SHA-256
与起点完全一致，Go/模块 Git 状态为空；只读 plan.md 指纹不变。
已审查四个 Rust 文件及本文差异，无依赖变更、无无关实现变更。
槽位 1 owner 与锁已释放，缓存保留；其他任务文件已有删除不纳入提交。

## 批次 88：服务配置默认值

日期：2026-10-24（Asia/Shanghai）。前置批次 87 已由提交 `2a0e49d1b`、
config_tso.rs 与五项独立测试及本文 691 passed / 0 failed / 1 ignored
证据核验。`server/config/config.go` 相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。
已读根 AGENTS.md、Copilot、ADR-001、移植指南和 do-task-plans 技能；
server/docs 无更深 AGENTS.md，plan.md 和来源主题保持只读。

### Go 依据与实现边界

`config.go::PDServerConfig`（509–541）、`adjust`（544–600）、常量
（207–225、243–251）以及 configutil.go::AdjustInt/AdjustFloat64/AdjustDuration：

- 两项 GC 并发数仅缺失且零时取 4，已有非零值原样保留；此任务不做范围校验。
- max-gap-reset-ts 总是对非正时长取 24h；min-resolved-ts-persistence-interval
  仅缺失且非正时取 1s，显式零/负数保留。正时长不改变。
- use-region-storage / enable-gogc-tuner 缺失无条件置 true；key-type、
  dashboard-address、runtime-services 缺失无条件取 table / auto / 空列表。
  显式 false、空字符串及已定义列表保留；metadata 由调用者传入 pd-server 子视图。
- flow-round-by-digit 缺失且零取 3，负数/正数保留。
- server-memory-limit、server-memory-limit-gc-trigger、gc-tuner-threshold
  缺失且零分别取 0 / 0.7 / 0.6，随后总是钳位 [0,0.99] / [0.5,0.99] /
  [0,0.9]；NaN 比较为 false，调整后仍为 NaN；正负无穷钳位至端点。
- metric-storage、block-safe-point-v1 没有默认调整，保留输入/零值。
- JSON 的三个 bool 使用带引号字符串，runtime-services 复用既有 StringSlice
  的逗号字符串编码，两个 duration 复用既有 Duration。浮点正零按 Go 输出 `0`，
  JSON 非有限浮点报错。Rust Clone 复制 runtime-services 列表。

在 canonical `astersql-pd-server-config` 新增 pd_server_defaults.rs，公开
PDServerConfig、adjust_defaults 与 decode_toml_pd_server。后者接受不含外层
pd-server header 的原生 TOML 表，保留字段 presence、布尔和字符串数组元素边界。
本任务仅实现 adjust 的纯值阶段，不伪装 Go 的最终 Validate 或废弃字段迁移；
这些行为分别留给后续 T7/T8/T14。不启动本地 region 引擎、GC tuner 或服务。
没有新增依赖或 Cargo.lock 变更。纯值任务不需要 MemoryStore/etcd 双入口。

JSON 字节证据复用本文 batch84-missing / batch84-explicit-zero 两份真实 Go 日志：
测试直接截取原始 `pd-server` JSON token，与 Rust **整个结构的序列化字节**及所有
15 个字段值比较；同时反序列化这些 Go 输出并核对默认调整幂等。
未手抄 Rust 结果为 Go 预期。额外 TOML、显式零、保留值、负数、NaN/无穷等
边界为只读源码推导，不冒充新增 Go 运行差分；本任务未新增 TOML 编码器。

重新审查 server/config 现有 TestMain：仅 goleak 与 WaitForEtcdConnections，
配置测试不要求 failpoint 注入。只读运行整个已有配置包，覆盖 TestPDServerConfig、
TestGCConcurrencyConfig 等原有断言。没有新增 Go oracle、启停 failpoint 或运行生成器。

### 本轮验证

每阶段以原子 mkdir 动态领取共享槽位 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录计划、任务、会话和持有 shell PID；EXIT/INT/TERM 清理自己的记录与锁，
所有子进程退出后释放并保留缓存。workspace 测试继承 `GOFLAGS=-mod=readonly`；
复核既有 etcd fixture 仅只读构建已有 Go 源码，产物及隔离数据目录位于槽位内。

初始可编译接口运行指定聚焦命令，**1 passed / 4 failed / 0 ignored，退出 101**；
四个失败来自默认值、显式零下 duration、缺失覆盖及浮点钳位的真实断言。
实现后相同五项不削弱断言全部通过；补充 TOML 和 float 边界后共七项通过。
补充入口阶段一次辅助编译因序列化错误类型推断 E0790 退出 101；最终入口不包含
该辅助编码器，重新格式化及所有基线通过；此编译失败不计作行为红灯。

第一轮七项与完整 workspace 回归均通过（698 passed / 0 failed / 1 ignored）。
随后审查发现 TOML NaN/inf 经 JSON 中间值会变 null；在既有第七项追加
真实断言后重跑同一聚焦命令，得到 **6 passed / 1 failed，退出 101**，
错误为 `invalid type: null, expected f64`。改为在转换前保留 TOML 原生浮点，
同一断言通过，NaN 保留、正负无穷按 Go adjust 钳位；最终完整基线重新运行。

| 确切命令 | 最终退出码与有效证据 |
| --- | --- |
| `cargo fmt --all` | 0；仅本任务 Rust 文件发生格式化变更 |
| `cargo test --locked -p astersql-pd-server-config --lib pd_server_defaults -- --nocapture` | 0；7 passed / 0 failed / 0 ignored |
| `go test -mod=readonly ./server/config -count=1` | 0；已有配置包通过，1.181s |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 core_region_adapter.rs:134 cpu_usage deprecated 警告 |
| `cargo test --workspace --locked` | 0；698 passed / 0 failed / 1 既有 ignored，含真实 etcd 回归；配置 crate 28 项通过 |
| `git diff --check` | 0 |

本轮状态：**已完成**。最终比较 1101 个 Go/模块文件路径集合与 SHA-256，
与任务开始完全一致，Go/模块 Git 状态为空。只读 plan.md 指纹仍为
`c7968f95a13d0d216ced3adafaca3140ad6ef959ff693c613cb903b231a6f777`。
已审查两个新 Rust 文件、lib.rs 的显式注册和本文证据差异；无依赖变更，
无无关 Rust 文件改动。槽位 1 的 owner 和锁已释放，缓存保留。
其他编号任务文件已有删除不纳入本任务提交。

## 批次 89：回收并发参数校验

日期：2026-10-24（Asia/Shanghai）。前置批次 88 经提交 `9968b0892`、
canonical PDServerConfig、七项默认值测试及本文 698 passed / 0 failed /
1 ignored 完整回归证据核验，准入满足。只适用根 AGENTS.md；已读 Copilot、
ADR-001、移植指南和 do-task-plans 技能。config.go/config_test.go 相对规划
基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。

### Go 依据与实现边界

- `server/config/config.go:655–663` 的 validateGCConcurrency：两字段合法闭区间
  均为 [1,16]，索引加载先于状态预热校验；错误原文分别为
  `gc-keyspace-index-load-concurrency must be between 1 and 16` 和
  `gc-state-cache-warmup-concurrency must be between 1 and 16`。
  Validate 在 dashboard/key-type 等其他检查之前调用该校验。
- `config_test.go::TestGCConcurrencyConfig` 固定默认 4/4、两字段各自的
  -1/0/1/2/4/16/17/128/MaxInt 输入及有效值 JSON 字段保留。
- TestGCConcurrencyConfigReload 固定缺失字段取 4、1/2 和 16/16；
  TestGCConcurrencyConfigReloadRejectsInvalid 固定两字段的 -1/0/17/128/
  MaxInt 拒绝且旧配置保留。persist_options.go:887–913 的 Reload 先默认化
  再加载，GC 校验成功后才发布配置。
- 本轮只读运行现有整个 Go 配置包，覆盖上述测试。已复核 TestMain 仅 goleak
  与 WaitForEtcdConnections；现有测试不要求 failpoint 注入，没有生成器调用。
  未新增 Go oracle、未启停 failpoint。i64::MIN、256 种合法组合及错误优先级
  的额外断言来自源码推导，明确不冒充新增 Go 运行差分。

`pd_server_gc.rs::PDServerConfig::validate_gc_concurrency` 在既有 canonical
类型上提供无副作用的独立校验，返回与 Go 一致的字段错误文本，不做默认调整。
本任务不装配完整 Validate，不迁移 PersistOptions/Reload 或存储；reload 测试
只验证解码、字段 presence、默认调整和真实校验构成的候选值契约，测试局部状态
检查拒绝候选值后保留旧值，不声称实现了生产热重载。纯值任务无需 MemoryStore/
etcd 双入口。四项独立 pd_server_gc_* 测试在 lib.rs 显式注册。

无新增依赖、无 Cargo.lock/Cargo.toml 变更，也没有新增编码实现。有效值的 JSON
字段断言复用现有编码器；完整 workspace 同时回归既有
config_decode_go_json_bytes 与 pd_server_defaults 的真实 Go JSON 字节比较，
预期来自本文保存的 batch84-missing / batch84-explicit-zero 原始输出，未手抄
Rust 结果充当 Go 预期。

### 本轮验证

红灯与绿灯阶段均通过原子 mkdir 动态领取共享槽位 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录计划、任务、会话及 shell PID；EXIT/INT/TERM 清理自己的记录与锁，
所有 Cargo 与 fixture 子进程结束后释放，保留缓存。所有子进程继承该绝对路径
与 `GOFLAGS=-mod=readonly`。复核既有 etcd fixture 只读构建现有 Go，产物和
隔离数据目录写入槽位；proto 构建入口读取已锁定模块及缓存，不写仓库 Go/模块。

先以可编译空校验接口运行指定聚焦命令，得到真实行为红灯 **1 passed /
3 failed / 0 ignored，退出 101**，分别命中非法范围、错误顺序及非法候选值
发布后的终态断言。最小实现后同一四项测试不削弱断言全部通过，并补充合法
reload 候选值的确切数值断言。

| 确切命令 | 退出码与有效证据 |
| --- | --- |
| `cargo fmt --all` | 0；仅本任务 Rust 文件 |
| `cargo test --locked -p astersql-pd-server-config --lib pd_server_gc -- --nocapture` | 红灯 101：1 passed / 3 failed；绿灯 0：4 passed / 0 failed / 0 ignored |
| `go test -mod=readonly ./server/config -count=1` | 0；已有配置包通过，1.188s |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 core_region_adapter.rs:134 cpu_usage deprecated 警告 |
| `cargo test --workspace --locked` | 0；702 passed / 0 failed / 1 既有 ignored，含真实 etcd；配置 crate 32 项通过 |
| `git diff --check` | 0 |

本轮状态：**已完成**。完整回归结束后核对 1101 个 Go/模块文件路径集合及
SHA-256，与起点完全一致，Go/模块 Git 状态为空。只读 plan.md 指纹仍为
`c7968f95a13d0d216ced3adafaca3140ad6ef959ff693c613cb903b231a6f777`。
已审查生产校验、四项独立测试、模块注册与本文差异，无无关 Rust 格式化、
无依赖变更。槽位 1 的 owner 与锁已释放，缓存保留。其他编号任务文件的
已有删除不纳入本任务提交。

## 批次 90：其余服务配置校验

日期：2026-10-24（Asia/Shanghai）。前置批次 89 经提交 `3ca7a9ea2`、
canonical GC 校验、四项独立测试和本文 702 passed / 0 failed / 1 ignored
回归证据核验，准入满足。仅根 AGENTS.md 适用；已读 Copilot、ADR-001、
移植指南和 do-task-plans 技能。config.go/config_test.go 相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。

### Go 依据与实现边界

`server/config/config.go:622–653::PDServerConfig.Validate` 按以下顺序返回
第一个错误：已有 GC 并发检查；dashboard-address 为 auto/none 时通过，否则
调用 ValidateURLWithScheme；key-type 必须为 table/raw/txn；
flow-round-by-digit 不得为负；三项浮点必须分别在闭区间 [0,0.99]、
[0.5,0.99]、[0,0.9] 内。范围检查使用直接比较，因此 NaN 通过，无穷拒绝。
Validate 不调整、钳位或修改配置；其他字段不增加校验。

错误保留字段名及 Go 原文。flow 错误的 RFC code 为
`PD:server:ErrConfiguration`，消息为
`flow round by digit cannot be negative number`；只读核对 Go 锁定依赖
pingcap/errors 的 normalize.go::GenWithStack，确认其替换消息、保留 code，
而不是使用 catalog 的默认消息 `cannot set invalid configuration`。
其余范围/GC/key 错误为普通错误，dashboard 复用既有 URL 错误。

`config_test.go::TestPDServerConfig` 覆盖 http loopback、auto、none、缺失默认、
无 scheme 地址及 foo；`util_test.go::TestValidateURLWithScheme` 为既有 URL
校验器提供相邻断言。本轮复核 TestMain 仅 goleak/WaitForEtcdConnections，
整个配置包没有要求注入 failpoint 的测试或生成器调用，安全只读运行现有包。
没有新增 Go oracle、启停 failpoint 或修改 Go。其余边界、错误优先级及
NaN/无穷/相邻浮点测试来自源码推导，明确不冒充新增 Go 跨语言运行差分。

在 canonical crate 增加 `pd_server_validate.rs::PDServerConfig::validate`，
串接已有 GC 与 URL 校验；公开 PDServerValidationError 区分普通、URL 和
带稳定 code 的错误。五项独立 pd_server_validate_* 测试由 lib.rs 显式注册，
覆盖逐项错误、合法边界、浮点相邻值、NaN/无穷、所有分支优先级及
decode/adjust/validate 的纯值组合。未实现废弃字段迁移、热更新或启动装配。
纯配置任务无需 MemoryStore/etcd 双入口；没有新增依赖或编码实现，
不需要新的 Go 编码字节证据。既有 Go JSON 字节差分由 workspace 回归复用。

### 本轮验证

各阶段均以原子 mkdir 动态领取共享槽位 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录计划、任务、会话和 shell PID，EXIT/INT/TERM 清理自己的记录与锁。
子进程全部退出后释放锁，保留缓存；Cargo/fixture 继承绝对路径及
`GOFLAGS=-mod=readonly`。已只读复核 fixture 仅构建现有 Go，产物和隔离数据
位于槽位；proto 构建读取已锁定依赖和缓存，本轮未重建 proto。

先新增可编译空校验接口和五项测试，指定过滤命令得到 **0 passed /
5 failed / 0 ignored，退出 101**，均为非法配置被错误接受的真实断言失败。
最小实现后同组五项 **5 passed / 0 failed / 0 ignored，退出 0**；错误类型
由临时通用 Error 收窄为公开枚举，保留原断言并匹配 flow 的 TypedError code。

| 确切命令 | 退出码与有效证据 |
| --- | --- |
| `cargo fmt --all` | 0；仅本任务 Rust 文件发生格式化变更 |
| `cargo test --locked -p astersql-pd-server-config --lib pd_server_validate -- --nocapture` | 红灯 101：0 passed / 5 failed；绿灯 0：5 passed / 0 failed / 0 ignored |
| `go test -mod=readonly ./server/config -count=1` | 0；已有配置包通过，1.191s |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 core_region_adapter.rs:134 cpu_usage deprecated 警告 |
| `cargo test --workspace --locked` | 0；707 passed / 0 failed / 1 既有 ignored，含真实 etcd；配置 crate 37 项通过 |
| `git diff --check` | 0 |

本轮状态：**已完成**。完整回归后再次核对 1101 个 Go/模块文件的路径集合及
SHA-256，与任务开始完全一致；Go/模块 Git 状态为空。只读 plan.md 指纹仍为
`c7968f95a13d0d216ced3adafaca3140ad6ef959ff693c613cb903b231a6f777`。
审查新增生产校验、独立测试、lib.rs 注册及本文差异，无依赖/锁文件变更，
无无关 Rust 格式化。槽位 1 的 owner 和锁已释放，编译缓存保留。
其他编号任务文件的既有删除不纳入本任务提交。

## 批次 91：键空间配置值

日期：2026-10-24（Asia/Shanghai）。前置批次 90 经提交 `3beb2ffae`、
canonical PDServerConfig 校验、五项独立测试及本文 707 passed / 0 failed /
1 ignored 完整回归证据核验，准入满足。已读执行技能、根 AGENTS.md、Copilot、
ADR-001 和移植指南；server/docs 无更深 AGENTS.md。config.go/config_test.go
相对规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。

### Go 依据与实现边界

- `server/config/config.go:906–944::KeyspaceConfig/adjust/Validate` 和常量
  254–257：缺失 wait-region-split 无条件取 true，缺失 timeout/interval
  无条件取 30s/50ms；显式 false、零值、负值保留。Validate 独立检查
  interval 闭区间 [1ms,100ms]，随后要求 interval 严格小于 timeout，
  不因 wait-region-split=false 而跳过。错误原文和检查顺序保持一致。
- `config.go:947–984::IsValidMetaServiceGroupID/AdjustMetaServiceGroups`：
  独立 ID 判断仅拒绝 `/`，空 ID 本身返回 true；调整时先 TrimSpace ID，
  依次拒绝空 ID、slash、空白地址、裁剪后的重复 ID。地址不要求是 URL。
  先构造新映射，全部成功后才替换，因此失败保留原映射；scalar 默认已先应用，
  不因映射失败回滚。Go map 遍历无固定顺序；Rust 使用 BTreeMap 确定遍历，
  多个非法组并存时不承诺 Go 本就不稳定的组间首错选择。
- `config.go:986–1033::Clone/GetPreAlloc/GetMetaServiceGroups/SetMetaServiceGroups`：
  clone 深拷贝两项集合，保留 nil；getters 返回独立副本且 nil 取空集合；
  setter 只替换值，不调整/校验。Rust 使用所有权与 Clone，无共享可变映射。
- 现有 `config_test.go::TestAdjustMetaServiceGroups` 七个子场景覆盖裁剪、
  多组、空 ID、空地址、重复、slash 和其他特殊字符。本轮只读运行整个配置包，
  复核 TestMain 仅 goleak/WaitForEtcdConnections，配置测试无需 failpoint
  注入，也没有生成器调用。未新增 Go oracle 或启停 failpoint。
  split 边界、i64 极值、Unicode 空白、失败终态和 clone 的新增断言为源码推导，
  明确不冒充新增跨语言运行差分。

canonical crate 新增 `keyspace_config.rs::KeyspaceConfig`、adjust、validate、
两项组工具函数和 decode_toml_keyspace；后者接受无外层 keyspace header 的
原生 TOML 表，并保留 presence。Option<Vec>/Option<BTreeMap> 保留 nil 与空
集合区别，Duration 复用已有值类型。六项独立 keyspace_config_* 测试位于
keyspace_config_test.rs，在 lib.rs 显式注册。不执行 split，不迁移 keyspace
业务、启动或热更新。纯值任务无需 MemoryStore/etcd 双入口，无新增依赖或
Cargo.lock 变更，无无关 Rust 格式化。

编码证据复用本文 batch84-missing / batch84-explicit-zero 已有真实 Go 日志：
测试直接截取 keyspace 原始 JSON token，与默认调整后的整个 Rust 结构字节
逐字比较，并反序列化回同值；两份日志的 keyspace 默认值相同。
该字节证据限于这两份已有输入；额外 TOML/非空映射测试只根据只读源码断言值，
不将 Rust 生成值充当 Go 编码输出，也不宣称已证明任意输入的 JSON 字节等价。

### 本轮验证

红灯、绿灯与完整基线阶段均以原子 mkdir 动态取得共享槽位 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录计划、任务、会话和持有 shell PID；EXIT/INT/TERM 清理自己的记录与锁，
所有 Cargo/fixture 子进程结束后释放，保留缓存。子进程继承绝对路径与
`GOFLAGS=-mod=readonly`。已复核既有 etcd fixture 仅只读构建已有 Go，二进制
及隔离数据位于槽位；proto 构建读取已锁定模块缓存，不改仓库 Go/模块文件。

先新增可编译空调整/校验接口和六项测试，指定聚焦命令得到真实行为红灯
**1 passed / 5 failed / 0 ignored，退出 101**，分别命中默认值、split 时长、
ID/映射与原始 Go 字节断言。最小实现后同一组六项不削弱断言全部通过。

| 确切命令 | 退出码与有效证据 |
| --- | --- |
| `cargo fmt --all` | 0；红灯、绿灯阶段先格式化，仅本任务 Rust 文件变更 |
| `cargo test --locked -p astersql-pd-server-config --lib keyspace_config -- --nocapture` | 红灯 101：1 passed / 5 failed；绿灯 0：6 passed / 0 failed / 0 ignored |
| `go test -mod=readonly ./server/config -count=1` | 0；现有配置包通过，1.182s |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 core_region_adapter.rs:134 cpu_usage deprecated 警告 |
| `cargo test --workspace --locked` | 0；713 passed / 0 failed / 1 既有 ignored，含真实 etcd；配置 crate 43 项通过 |
| `git diff --check` | 0 |

本轮状态：**已完成**。完整回归后再次核对 1101 个 Go/模块文件的路径集合及
SHA-256，与任务开始完全一致，Go/模块 Git 状态为空。只读 plan.md 指纹仍为
`c7968f95a13d0d216ced3adafaca3140ad6ef959ff693c613cb903b231a6f777`。
已审查生产值类型、独立测试、lib.rs 注册及本文差异，无依赖变更、无无关 Rust
格式化。槽位 1 的 owner 与锁已释放，保留缓存；其他编号任务文件已有删除
不纳入本任务提交。
