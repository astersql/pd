# 任务072：区域均衡调度

状态：已完成。

前置批次71提交 `bb3c8d6e6` 的配置/调度 API 与证据已核对；其包级26项及全仓
1575项通过记录可审查。本任务包级29项重新覆盖前置API和新增行为。
两份 plan.md 保持只读。

生产路径：[balance_region.rs](../../../pkg/schedule/schedulers/balance_region.rs)，
[领域边界](../../../pkg/schedule/schedulers/balance_region.rs.md)。
公开 `BalanceRegionScheduler::schedule`、`is_schedule_allowed`、
`BalanceRegionConfig::new/ranges` 和 `RegionBalanceContext`。
复用真实 core/filter/placement/statistics/operator，执行一次最多一个 peer 搬迁，
保留 role、REGION kind、MEDIUM 优先级、评分附加信息及健康检查。
没有新增 crate、依赖、HTTP 路由或 Go 写入。

## 来源与期望

按用户后续授权采用**只读Go源码/测试推导 + Rust行为实测**。
没有本任务 Go 运行产物或 Go/Rust 运行差分，未将 oracle-index 的其它条目冒充证据。
[provenance](logs/task_072/provenance.json) 保存来源提交、来源文件和fixture SHA256，
Go binary/output SHA256 均为 null。

`balance_region.go` 的 Schedule/transferPeer 提供来源排序、pending/follower/leader/learner
选择顺序、区域过滤、目标排除、placement、重试、工厂和诊断契约；`utils.go`
提供 influenceAmp=5 和对称 max(tolerantResource,candidateSize) 投影。
`balance_region_test.go` 的 TestInfluenceAmp、
TestBalanceRegionOrdinaryMoveNotBlockedByCandidateSize、
TestBalanceRegionLargeCandidateDoesNotOvershoot、
TestSingleRegionOnLargeEmptyDiskDoesNotMigrate 与
TestBalanceRegionRealScheduleDoesNotMoveSoleRegion 是评分与防抖行为的源码依据。
这些测试没有被执行；fixture 使用相同评分边界及真实 Rust 调度对象验证。
测试套件的其它选择/范围/复制契约也用于设计场景；不宣称逐条运行Go对照。

[fixture](fixtures/task_072.json) 手工冻结22个调度输入与预期命令数。
其中 ordinary/large/sole/small/empty/range-empty 覆盖大小，outside/managed-exclusion
覆盖范围，placement/placement-diversity 覆盖规则及legacy位置保护，
pending-influence/pending-source 覆盖在途影响与优先选择，另有down、欠复制、
witness、offline target、hot、no leader、leader source、allocation failure、balanced。
sole场景交换store ID，两方向各重复调度10次均无搬迁。
随机索引固定0、shuffle保持输入顺序，时钟固定；peer ID为1000。
同分候选不规范化操作差异；有解场景固定来源1与唯一有效目标4。

## 失败和通过验证

- `task_072_behavior_cases`：22/22调度场景；检查命令数、目标/来源、AddLearner和
  RemovePeer端点、REGION kind、MEDIUM、health check。逐条执行真实操作步骤到
  canonical RegionInfo，再更新区域索引，核对成员变化、成功终态、confver=3、
  双向region_size影响量、输入influence不变及实际存储尺寸分布改善。
  allocation失败保留类型化错误并记录step4 CREATE_OPERATOR_FAILED。
- `task_072_influence_amp_and_candidate_margin`：5/5评分输入，含108/95与107/95的
  放大影响量边界、普通96候选可移动、100大候选拒绝过冲、6TiB空盘单10候选不移动。
- `task_072_config_and_limits`：范围二进制key十六进制JSON往返、非法hex拒绝、
  空范围归一、限额严格小于及零限额拒绝。

[red-api](logs/task_072/red-api.log) 初次缺失API导致编译失败，退出101，
也包含测试装配时配置类型不匹配，未当作行为失败。
装配阶段明确配置max_replicas=3，避免ProviderSnapshot零默认值把所有区域拒绝。
[red-behavior](logs/task_072/red-behavior.log) 临时绕过投影评分准入，普通案例通过，
large-overshoot实际产生1条命令（期望0）断言失败，2通过/1失败，退出101。
随后恢复代码并通过，证明确实能检测错误决策。
首轮实现还暴露了共享influence查询补零，已改为本轮副本并验证调用方快照保持不变。

## 验证记录

所有确切命令、退出码、环境和stdout/stderr保存在 [logs/task_072](logs/task_072/)。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_072 -- --nocapture` | 0 | 3通过，22调度+5评分场景 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 29通过、0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，仅既有cpu_usage废弃警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1578通过、0失败、1既有忽略 |

全仓数量含实际执行的doc tests，零测试目标不作行为验收。
既有忽略为需要独立进程二进制的tso_processes，未新增忽略。
[计数](logs/task_072/counts.json) 从实际test result行汇总。
每个Cargo阶段均通过run.py原子mkdir领取共享槽位，实际使用 **1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
正常/失败/中断等待子进程结束后释放自己锁，保留缓存，不清理其他活跃任务。

[审计](logs/task_072/audit.json) 核对开始/结束1101个Go及模块文件SHA256一致，
包含未跟踪文件扫描，Go Git状态为空；两份只读plan.md无差异。
`git diff --check` 和提交前staged diff检查通过；只包含本任务生产模块、
注册、测试、fixture、说明与证据。工作区原有其它任务删除不纳入本任务提交。
全部命令结束后自己的槽位锁已释放，缓存保留。完成后删除自身编号任务文件。
日志仅去掉末尾空行以通过diff检查，保留命令输出内容。
