# 任务112：迁移调度准备判断

前置111为提交 `206b0d444`；已审查其 canonical diagnostic crate、Manager 查询 API、
13项固定案例、3个聚焦/包级测试及1744项 workspace 测试证据。
原有 core `RegionsInfo` 已提供全局/每 store 非 storage 区域计数和 `is_store_prepared`，
`StoreInfo` 已提供生命周期与 raw heartbeat stats，不需要修补或复制前置状态。

按用户后续明确授权，以**只读 Go 源码/测试推导 + Rust 真实行为及故障验证**
替代缺失 Go 运行 oracle/运行差分。未执行 Go 工具或测试，没有 Go 运行输出。
[provenance](logs/task_112/provenance.json)保存来源 commit、四个源码 SHA-256、
fixture SHA-256、授权说明和 null 运行字段。现有 oracle-index 的运行案例不覆盖
本准入故事，不能据其 admission 字样声明本任务有 Go 运行差分。

源码契约来自 `pkg/schedule/prepare_checker.go` 的完整分支顺序、
`coordinator.go` 的五分钟常量及 `ShouldRun` 委托、`pkg/core/region.go` 的90%
准备阈值。只读 `tests/integrations/mcs/scheduling/server_test.go` 中
`TestPrepareChecker` 提供重启后未准备、少量心跳不足、补齐后准入的源码断言。
没有把源码断言当作执行证据。

生产实现 [prepare.rs](../../../pkg/schedule/prepare.rs)，
[边界说明](../../../pkg/schedule/prepare.rs.md)，canonical crate 为
`astersql-pd-schedule`。公开 API：`PrepareChecker::new/check/should_run/is_prepared/set_prepared`，
`COLLECT_TIMEOUT`。宿主提供单调时钟与实际总区域数 provider；准备标志由 mutex 保护。
使用借入的真实 `BasicCluster`，collect/reactive 经已有区域索引更新，不另设心跳集合。
`should_run` 是后续 Coordinator 可复用的准入入口，本任务不声称已完成 Coordinator 装配。
读取失败返回 false、保持未准备并允许恢复；严格超过超时或已准备时绕过 provider；
全局与两个不同 store 阈值都需满足。准备标志 sticky，新实例重新检查。

[fixture](fixtures/task_112.json)冻结16项预期，均从源码规则推导：
全局8/10、9/10、10/10；独立全局9/11（store条件满足）；raw store报告不足；
indexed store不足；Preparing阈值；Removing/Removed/Unknown跳过；空集；
小数量取整边界；精确300秒仍不超时、再加1ns超时；Sync active、Storage inactive。
时间作为原始整数纳秒输入；无输出字段归一化；最终16通过/0失败。

4个显式注册的独立行为测试：

- `task_112_behavior_cases`：16项固定输入，各自校验 ShouldRun 返回及 prepared 状态。
- `task_112_live_collection_duplicates_and_sticky_admission`：真实 core 双层索引；
  同一region心跳重复20次仍只计1；补齐9/10后准入；退化回Storage不撤销；新gate拒绝。
- `task_112_provider_failure_recovery_timeout_and_override`：在总数读取I/O边界注入类型化
  Unavailable错误，校验拒绝和未准备；恢复后准入，后来故障不撤销；300秒处错误仍拒绝，
  300秒+1ns超时准入；显式override无需访问会panic的provider。调用数是辅助断言，
  主要验证实际状态和返回。此纯读取门无持久化，不引入无关MetaStore。
- `task_112_concurrent_sticky_transition`：16个真实并发调用共享同一gate，均成功且
  prepared为true，provider只执行一次，证明sticky转移受锁保护。

红灯记录：初轮故意屏蔽全局阈值，15项案例仍通过，揭示全局与store条件耦合造成
覆盖不足，保留 [初轮](logs/task_112/red.json) 及原始日志，不冒充有效失败证据。
补上独立全局9/11案例，再对同一错误行为执行同一聚焦命令；
[有效红灯](logs/task_112/red-global.json) exit101，3通过/1失败，
`global_threshold_independent_of_store` 在真实断言上 actual=true、expected=false。
恢复正确阈值后进行最终验证；没有缺API编译失败充当行为红灯。

[最终命令与退出码](logs/task_112/final.json)，完整 stdout/stderr 为
`logs/task_112/final-0.log.gz` 至 `final-6.log.gz`：

```sh
cargo fmt --all
cargo test -p astersql-pd-schedule --locked --lib task_112 -- --nocapture
cargo test -p astersql-pd-schedule --locked
cargo metadata --format-version 1 --no-deps
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
```

每个验证阶段以原子mkdir从1–10动态领取共享槽位，并记录计划/任务/会话/PID；
本任务各阶段实际均领取槽位1，绝对 `CARGO_TARGET_DIR` 为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；
`CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2`，所有Cargo及子进程继承。
异常时terminate/wait使用槽位的子进程，再删除自有owner并rmdir自有锁；缓存保留。

Cargo.lock只增加本地crate及已有依赖；初次离线生成带来的无关pulldown-cmark升级
已恢复，最终没有远程版本变更。workspace fmt未造成其它Rust文件差异。
[只读审计](logs/task_112/readonly-check.json)验证1101个Go/模块文件的清单、
SHA-256和Git状态均一致，两份plan哈希不变，`git diff --check` exit0。
其它任务既有删除不纳入提交。

最终七条命令全部 exit0；聚焦4通过/0失败，包级4通过/0失败；workspace
1748通过/0失败、1个既有ignored（含2个compile-fail文档测试）。
[数量汇总](logs/task_112/test-counts.json)从原始test result行统计。
所有使用槽位产物的子进程均已退出，自有槽位1锁已释放，缓存保留。
