# 任务113：迁移协调器组件装配

依赖112为提交 `60db02166`，已核验其 PrepareChecker、16项固定案例、4项真实行为
测试及1748项 workspace 通过证据。当前批次仅113；未并行执行其它任务。

依据用户后续明确授权，使用**只读 Go 源码/测试推导 + Rust 真实行为和故障验证**
替代缺失 Go 调度运行 oracle/差分准入；没有声称取得 Go 调度运行输出。
[provenance](logs/task_113/provenance.json)记录源码 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、7份源码 SHA-256、fixture 哈希、
授权说明和 null Go运行输出字段。7份当前源码均与该 commit 字节一致。
oracle-index 已审查，其既有运行案例不能证明 Coordinator 所有装配行为。
已有只读 Go 集成测试通过 Coordinator 取得 operator controller；rule/getter/TTL/
callback 的精确契约取自 canonical Go 源码，不把测试名或源码断言冒充运行结果。

生产入口：[coordinator.rs](../../../pkg/schedule/coordinator.rs)，
[领域边界](../../../pkg/schedule/coordinator.rs.md)。canonical crate 为
`astersql-pd-schedule`。`Coordinator::new` 保留同一 cluster/config，装配 admission、
terminal records、CheckerDispatcher、SchedulerController、PrepareChecker 与诊断记录。
`get_rule_checker/get_merge_checker` 返回 dispatcher 内部组件的锁定视图；
`is_pending_region` 委托 rule pending，而 scatter/splitter 的 replication 故障回调
进入同一 pending-processed TTL 队列，两类 pending 不混淆。
`record_op_step_with_ttl` 使用当前配置间隔写入 rule checker 的真实晋升缓存。
成功通知复用 weak checker hook 和 canonical records 身份去重，不建立强引用环。
借用的 scatter/splitter/diagnostic 视图保留真实算法及控制器，不另建业务副本。

只对必要前置 API 做小幅扩展：CheckerDispatcher 暴露 rule/merge 只读访问器；
RuleFaultChecker 将现有 witness repair 的严格到期判断提取为
`is_recently_promoted`，仍是修复路径使用的同一判断。相关 `.rs.md` 已同步。
没有新增后台调度循环或 private runtime；cache shutdown、heartbeat routing、时钟、
I/O 与后续调度驱动归宿主。测试的 heartbeat 输入是未启动的真实 routing handle，
本任务不声称验证了 heartbeat 传输。共享 MetaStore 使用既有 MemoryStore 后端。

[fixture](fixtures/task_113.json)冻结5项 success callback 矩阵和3项 TTL 边界，
合计8项表驱动输入，全部通过。success/limit10/related2 同时缓存两个ID；limit0、
related0、cancel、replace均不缓存。TTL从100记录10ns，109有效、110/111无效。
所有输入整数时间均保留；不删除输出字段；集合排序来自既有 canonical BTree 容器。

5项显式注册的真实行为测试：

- `task_113_behavior_cases`：上述5项矩阵，验证真实 operator 状态、记录发布、
  重复 burial 不通知、同一 checker 两个 region 的 affinity 缓存与过期。
- `task_113_ttl_getters_and_admission_fault`：上述3项严格 TTL 边界；组件身份；
  region count I/O 读取故障拒绝准入且不置 prepared，恢复后准入，后续错误不撤销。
- `task_113_scatter_splitter_shared_pending_and_admission`：空 scatter 请求保留错误；
  单副本不足触发 scatter pending-processed；清空后 splitter 故障写回同一队列；
  真实已复制区域 split 提交的 operator 与 facade admission 取得的 Arc 身份相同。
  配置明确设置 waiting limit10，避免零值限额使正常提交被拒绝。
- `task_113_diagnostic_live_scheduler_state`：同一 controller 新增 scheduler 前 disabled，
  新增后缺 recorder 返回错误，更新 canonical recorder 后 pending/scheduling 立即可见。
- `task_113_rule_pending_facade_reads_dispatcher_state`：真实 placement/labeler/affinity/
  MemoryStore/dispatcher 执行找不到副本目标，实际 rule pending 从false转true；
  facade读取一致、其它ID仍false；rule/merge getter 与 dispatcher 内地址一致，
  facade完成记录在该 rule checker 中可见并严格到期。

红灯是明确的行为变异验证，未将缺API或依赖编译错误当成验收证据：临时断开
`ControllerRecords` 的成功回调注册，同一聚焦命令 exit101，4通过/1失败，
`task_113_behavior_cases` 的 success/limit10/related2 案例实际false、预期true。
[红灯命令](logs/task_113/red.json)及完整压缩stdout/stderr保留；随后恢复正确接线。

[最终命令与退出码](logs/task_113/final.json)，完整stdout/stderr为
`logs/task_113/final-0.log.gz` 至 `final-6.log.gz`：

```sh
cargo fmt --all
cargo test -p astersql-pd-schedule --locked --lib task_113 -- --nocapture
cargo test -p astersql-pd-schedule --locked
cargo metadata --format-version 1 --no-deps
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
```

每个 Cargo 阶段从1–10依次原子mkdir领取空闲槽位，记录任务/计划/会话/PID，
所有实际阶段均领取槽位1，`CARGO_TARGET_DIR` 为绝对路径
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
所有 Cargo 子进程继承 `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2` 和该 target。
等待子进程退出后清理自有 owner 文件并 rmdir 自有锁，保留缓存。
本任务没有重用其它任务锁、另建 target 或指定其它 target-dir。

Cargo.lock仅扩充本地 schedule crate 的已有依赖引用；离线生成附带的无关
pulldown-cmark升级已撤销，没有远程依赖版本变化。workspace fmt无额外文件差异。
workspace 中既有 etcd 合约 harness 可编译现有 Go fixture，产物和运行数据写入
领取槽位；未启用 failpoint、运行 Go 调度测试或写入 Go 源码/模块。
[只读审计](logs/task_113/readonly-check.json)核对1101个 Go/模块文件的清单、
SHA-256和Git状态及两份plan哈希；其它任务已有删除均排除于本任务提交。

最终7条验证命令全部 exit0；聚焦5通过/0失败，包级9通过/0失败；workspace
1753通过/0失败、1个既有ignored（含2个compile-fail文档测试）。
[数量汇总](logs/task_113/test-counts.json)由原始test result行累计。
全量验证后再次审计，1101个Go/模块文件清单、哈希和状态全部一致；
两份只读plan哈希一致，`git diff --check` exit0。
初始审计清单保存在 `logs/task_113/readonly-before.json.gz`。
槽位1所有验证子进程已结束，自有锁已释放，编译缓存保留。
