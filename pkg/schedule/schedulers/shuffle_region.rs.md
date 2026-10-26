# shuffle_region.rs

领域 API：`ShuffleRegionConfig::{from_args,ranges,roles,is_role_allowed,set_roles}`；
`ShuffleRegionScheduler::{new,config,encode_config,is_schedule_allowed,persist,update_roles,reload,schedule}`。
HTTP role 路由留给适配层；不持有运行时、全局状态、协议对象或私有 cluster。

只读来源为同目录 shuffle_region.go、shuffle_region_config.go、init.go，
scheduler_test.go 的 TestShuffleRegion/TestShuffleRegionRole；replication/placement、
operator 和核心 region 索引沿用已验证 canonical API。用户明确授权源码推导替代
缺失运行 oracle；未执行 Go，不声称 Go/Rust 运行差分。

候选 source 先按 ID 固定无序遍历，再由宿主 shuffle；每个 source 依次尝试 follower、
leader、learner。pending/down/replication 必须通过后才选择首个区域；源已选定后，
无 target 不重试其它 source。target 复用 Low move-region 状态、special-use、
placement safeguard，并排除全部现有 peer stores。随机回调从合法目标中选择一个，
真实 PeerOperatorFactory 保留原 peer role，以 REGION kind 建立 Low 优先级搬迁。
leader 搬迁由 builder 加入必要 transfer-leader；新 peer 不保留 witness 标记，与 Go
构造新 Peer 的字段一致。分配/构建错误保留 BuilderError，不输出半成品操作。

配置复用 ShuffleLeaderConfig 的范围编码。CLI 默认全部角色和全 key 范围；JSON
保留 null/空 roles 和 ranges；Go 的 reload 不验证角色字符串，因此此处同样不拒绝
存储中未知角色，它们不匹配任何采样类别。set_roles/update_roles 验证 leader、
follower、learner，拒绝非法字符串并保留旧值；允许空列表和重复项。

配置读写使用已有 StorageEndpoint/MetaStore；update_roles 在成功持久化后才发布；
reload 解码失败保留旧配置。宿主独占可变 scheduler；并发请求同步由适配层负责。

独立测试显式在 lib.rs 注册。固定 fixture、两种 conf-change 模式、实际步骤应用、
故障 I/O 及配置回滚证据见 [任务102](../../../docs/rust/schedule-execution/task-102.md)。
