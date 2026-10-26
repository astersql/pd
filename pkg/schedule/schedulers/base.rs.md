# Scheduler 基础契约

`base.rs` 是 `astersql-pd-schedule-schedulers` 的 canonical 领域入口。
依据只读 `base_scheduler.go`、`scheduler.go`、`config.go` 实现；Go 运行 oracle
限制由用户解除，本文与 task-065 记录的是源码推导及真实 Rust 测试，不是运行差分。

- `BaseScheduler` 提供名称回退、类型、10ms 最小间隔、5s 指数增长上限、
  null 配置编码、reload/prepare/clean 的基础默认行为。
- 间隔使用有符号纳秒。指数乘 1.3 后截断，线性加 3s，零增长保持原值且不截上限。
  线性采用 Go 有符号整数的 wrapping 加法；指数浮点转整型在正常调度范围一致，
  不承诺超出 i64 转换范围的 Go 平台相关行为。
- `Scheduler<C>` 使用已有 `SchedulerCluster`、`Operator`、`BalanceSchedulerPlan`；
  具体策略必须实现 schedule 与 allowed。Base 自身不提供虚假空调度成功。
  HTTP 留给适配层，库不创建 runtime、全局注册表或后台任务。
- 配置 JSON 编解码以 serde 类型提供，分别保留 Encode/Decode 错误。
  编码保留 Go HTML 与 U+2028/2029 转义。输入契约是有限 JSON 值，
  不承诺任意 Go 动态类型、非有限浮点、无效 UTF-8 或全部数值字面量的逐字节兼容。
- `ConfigJsonDecoder` 持有输入，可延迟按目标类型解码。
  `to_payload` 跳过坏 JSON、同名后一个有效项覆盖前一个、允许 null；
  递归数字转换为 Go any 的 float64，包括大整数舍入。
  输出 map 顺序规范为排序顺序；configs 多余项被忽略，短于 names 时返回
  PayloadLength，替代 Go 的越界 panic。这是明确的领域错误边界。
- 默认类型来自 canonical types crate，支持宿主当前内核与显式 Classic/NextGen 列表。
  默认身份与禁用能力分开：没有 default config 的 set_disabled 成功但无效果。
  `DefaultSchedulerConfig` 已移入 [config.rs](config.rs.md)，公开 API 和行为不变。
  它复用真实 StorageEndpoint 与 ClusterPaths，保留完整配置
  字段；每次查询尝试加载 disabled，缺失、null 或读取/解码错误保留本地值。
  set 先改本地值再保存，失败不回滚；后续成功读取恢复已持久化值。
  独占可变借用保证单实例修改顺序；多实例共享状态仍由宿主协调。
- `update_config` 对基础类型返回 Unsupported。注册表目前为空，已知的 18 个字符串
  不等于已移植策略：lookup/切片 decoder 全部显式报未注册。
  名称查找仅保留 Go 即使空表也存在的 scatter-range 历史别名。

测试见 `task_065_test.rs`，完整来源哈希、失败断言和验证命令见
[task-065](../../../docs/rust/schedule-execution/task-065.md)。
