# 任务114：迁移启动调度配置恢复

依赖113为提交 `b1fc5a091`，已核验 canonical Coordinator 组件/API、5项真实行为
测试及1753项 workspace 通过证据。本批次仅执行114，没有并行任务。

依据用户后续明确授权，采用**只读 Go 源码/测试推导 + Rust 真实行为与故障验证**，
替代缺失 Go 运行 oracle/运行差分；没有运行或伪造 Go 调度输出。
[provenance](logs/task_114/provenance.json)冻结 source commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、实际来源 SHA-256、fixture 哈希与
null Go 运行输出字段。当前5份来源均与该 commit 字节一致。
oracle-index 已审查：已有运行案例不能证明 InitSchedulers 完整恢复语义。
只读 `TestTransferLeaderForScheduler` 展示 leader 切换后等待 initialized 并恢复
自定义 evict 配置，未执行该集成测试；精确错误/回退契约来自 coordinator.go。

生产路径：[scheduler_restore.rs](../../../pkg/schedule/scheduler_restore.rs)，
[领域边界](../../../pkg/schedule/scheduler_restore.rs.md)。
`Coordinator::init_schedulers`、`are_schedulers_initialized` 使用同一 canonical
SchedulerController、18类真实 factory 与 StorageEndpoint/MemoryStore：

- 独立配置最多读取10次；全部失败返回类型化错误，初始化保持false。library
  将 Go 的 fatal 路径上交宿主，没有退出进程或私有 runtime。
- 按旧列表首个同类型条目决定独立配置是否可用；disabled/孤立配置不创建；
  独立 JSON 优先，损坏时允许旧 args 回退；重复 Exists 保留旧列表项。
- 无效旧项删除，disabled 原样保留；最终列表发布到同一 controller 的完整配置
  并持久化，保留其它字段。`replace_scheduler_configs` 是唯一必要前置 API 扩展。
- 最终配置保存失败进入报告但仍 initialized=true，与 Go 一致；取消/读取耗尽
  不设置该标记。取消保留已创建项，宿主应让 future 完成，不通过丢弃 future 取消。

宿主提供 `CoordinatorInputs.scheduler_configuration` 完整持久 JSON；controller
配置和报告 valid_configs 是启动结果，供宿主发布到自己的 config provider。
现有泛型 Arc<C> 为只读接口，此入口不引入反向 server 依赖或私有配置可变状态。
`need_run=true` 发布普通策略、false 发布 handler 策略，循环启动仍由宿主负责。

[固定 fixture](fixtures/task_114.json)包含10种输入，在两种模式下合计20项：默认、
独立自定义 batch7 优先、重复、disabled、损坏 JSON 回退到 batch4、孤立、未知类型、
非法 args、Prepare失败清理、disabled未知条目保留。时间/随机输入固定于边界，
没有删除输出字段或时间归一化；名称排序沿用 canonical BTreeMap。

5项显式注册的真实测试：

- `task_114_behavior_cases`：上述20项，核对最终名称/数量、保留配置数量、完整配置
  实际保存结果、无关字段、初始化状态、真实 factory batch 与 prepare清理状态。
- `task_114_all_builtins_restore`：复用冻结 task106 的18类参数与预期名称，运行两种
  模式的36项真实启动恢复，校验类型、实际策略存在及有效配置列表。
- `task_114_cancellation_and_storage_faults`：取消后未发布；持续读取失败恰好10次，
  initialized=false；2次瞬时读取失败后恢复；scheduler独立配置写入成功、最终
  whole-config写入失败时 initialized=true 且报告错误，随后重试成功。
- `task_114_failed_add_cleanup`：两种模式中 grant 的配置写入失败，验证列表清理、
  无策略发布且 store1 的 leader-transfer-out 已恢复。故障仅注入 MetaStore I/O。
- `task_114_restart_operator_result`：第一次启动后保存完整配置并销毁 Coordinator，
  用同一 MemoryStore 和新集群/Coordinator 重启；恢复策略通过真实 rules/core/builder
  生成单步 LEADER operator，region1，TransferLeader from1 to3；两次实际结果一致。

先写测试与空恢复实现获得[红灯](logs/task_114/red-behavior.json)：聚焦命令exit101，
0通过/2失败。默认案例 valid_configs 实际0、预期2；取消未返回 Cancelled。
这是行为断言失败，不以缺API、编译失败或零测试替代红灯。最初锁文件缺少新本地
依赖引用的 exit101 单独记录于 red.json，不计作行为证据。Cargo.lock 最终仅增加
现有 async-trait/thiserror 两项依赖引用，没有远程依赖版本变化。

[最终命令与退出码](logs/task_114/final.json)，stdout/stderr 完整压缩保留为
`logs/task_114/final-0.log.gz` 至 `final-6.log.gz`：

```sh
cargo fmt --all
cargo test -p astersql-pd-schedule --locked --lib task_114 -- --nocapture
cargo test -p astersql-pd-schedule --locked
cargo metadata --format-version 1 --no-deps
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
```

每阶段依次原子mkdir领取槽位1–10中的空闲项；本次实际均为槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
所有子进程继承 `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2`，无其它 target-dir。
正常/失败均在子进程结束后删除自有 owner 文件并 rmdir 自有锁，保留缓存。

workspace 既有 etcd fixture harness 仅编译 `scripts/rust-etcd-fixture` 现有 Go 源码，
二进制与运行数据写入领取槽位；未启用 failpoints，未写入 Go 源码/模块。
[只读清单](logs/task_114/readonly-before.json.gz)冻结1101个Go/模块文件（含未跟踪
非忽略文件）的SHA-256/Git状态及两份plan哈希。任务原有删除不纳入本次提交。

最终7条命令全部exit0：聚焦5通过/0失败，包级14通过/0失败；workspace
1758通过/0失败，1个既有ignored，含2个compile-fail文档测试。
[计数](logs/task_114/test-counts.json)由完整日志的test result行汇总，20+36项
表驱动恢复案例全部通过。workspace fmt没有产生范围外文件修改。
[最终只读审计](logs/task_114/readonly-check.json)再次核对1101个Go/模块文件清单、
内容哈希和Git状态及两份plan哈希完全一致，`git diff --check` exit0。
最终全部子进程已退出，自有槽位1锁已释放，编译缓存保留。
