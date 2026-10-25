# 任务045：规则孤儿与位置改进

状态：已完成。全部必需验证退出0，2个Rust测试/40场景通过。生产入口为 `pkg/schedule/checker/rule_location.rs` 的
`RuleLocationChecker::fix_orphan_peers` 与 `fix_better_location`，已在 canonical
`astersql-pd-schedule-checker` 的 lib.rs 显式注册；测试位于独立 task_045_test.rs。
没有新增 crate、依赖、feature 或 Cargo.lock 改动。宿主在孤儿阶段之后执行已有
规则修复，并在每条规则修复之后调用位置改进；本任务不扩展 controller。

## 依赖与来源

批次44由提交 `a81636086`（schedule/checker: port rule fault and role repairs）与
[任务44证据](task-044.md) 确认；其4项测试/33场景及workspace1472测试已通过，
本次包级验证也重新运行它们。复用批次40的 `ReplicaStrategy`、placement真实 fit、
批次31的 `PeerOperatorFactory` 和批次44的 recorder，未复制第二套生产实现。

已读取 oracle-index.json；用户后续授权解除 Go 运行 oracle/运行差分准入限制。
本任务是**只读 Go 源码和测试契约推导 + Rust 行为验证**，未执行 Go，不声称获得
Go 原生输出或运行差分。[provenance](logs/task_045/source-provenance.json) 保存来源
commit `a81636086`、逐文件 SHA256 及冻结 fixture SHA256。

来源为 rule_checker.go 的 fixOrphanPeers、fixBetterLocation、handleFilterState、
recorder.refresh；replica_strategy.go 的 getBetterLocation；placement/fit.go 的
Replace/ExtraCount；operator/create_operator.go、builder.go、kind.go 的工厂、步骤和 kind。
相邻 Go 测试包括 TestFixOrphanPeers、TestFixToManyOrphanPeers、
TestFixToManyOrphanPeers2、TestFixOrphanPeers2、TestPriorityFitHealthWithDifferentRole1/2、
TestFixBetterLocationEngineConstraint、TestRecorderRefreshLooksUpStoreIDNotCount。
这些名称只作只读契约定位，未算作 Go 已运行测试。

## 场景及故障

[fixture](fixtures/task_045.json) 保存39个固定场景：健康 fit 清首个 orphan、无孤儿、
计数未满足/拟合 peer pending/down 的安全返回、断连后的 learner 晋升、witness 和
unhealthy/disconnected orphan 拒绝、标签和多样性拒绝、同角色替换、联合共识降级、
关闭联合共识的类型化错误、多孤儿保留健康备用、unhealthy/断连/缺失 store 优先、
extra_count=0 保护、20s边界、严格位置改进、空 location labels、相同位置不替换、
永久/临时过滤、源端临时阻塞且有/无改进目标、标签/engine约束、allocator失败、
暂停、无leader、witness fast failover、orphan leader先转移再删除。

另一个独立测试覆盖 recorder：13次真实替换构建得到计数(5,4,4)，验证第二次替换
先转移leader；allocator失败不增加计数；五分钟边界保持，超过边界按缺失的store ID
清空（计数值5对应live store5）。这部分复用已有 RuleFaultChecker，没有新增recorder。
共2个Rust测试，40个场景。成功操作比较描述、有序步骤、kind、MEDIUM优先级、
region ID/epoch；真实RegionInfo心跳逐步推进至SUCCESS，并重新fit验证每条规则
计数、角色满足度和isolation score不降低。原输入保持不变。

只有时钟与ID分配I/O边界可控；无新MetaStore写入，不虚构持久化故障。
位置移动复用实际strategy，永久失败请求pending，临时失败请求pending_processed。

## 失败与错误识别

[red-api.log](logs/task_045/red-api.log) 保存首次缺失模块/API的exit101。
green-first至green-5保留开发过程：依据Go builder/kind契约补计OpRegion(128)、
调整隔离拒绝输入使原fit为三个不同zone；将demote案例配置为canonical joint能力并
检查ChangePeerV2Enter后删除；显式配置v2负载公式，使source3具有唯一更高负载；
临时source案例同时阻塞两个同等多样性的最佳source，避免另一个健康source合法替换。
这些fixture修正不充当错误行为识别证据。

[mutation.py](logs/task_045/mutation.py) 在正确生产源中逐项注入六种错误：忽略fit健康、
绕过替换fit、忽略extra_count、忽略断连优先、反转filter state、遗漏witness标志。
[mutations.json](logs/task_045/mutations.json) 记录六项均exit101，全部由测试文件中的
真实行为断言识别，未用编译失败替代。finally恢复生产源后重新运行完整基线。

## 验证与共享槽位

[run.py](logs/task_045/run.py) 每阶段从1至10依次原子mkdir动态领取共享槽位，记录
计划/任务/会话/PID；等待子进程结束后释放自己的锁，保留缓存。所有实际阶段领取
槽位1：`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[validate.py](logs/task_045/validate.py)、[commands.json](logs/task_045/commands.json)
保存确切命令、退出码和有效测试计数。workspace阶段使用 `RUST_TEST_THREADS=2`。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_045 -- --nocapture` | 0 | 2通过、0失败、0忽略；40场景 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 16通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1474通过、0失败、1既有忽略 |

既有忽略为独立进程测试 `tso_processes`；未新增忽略测试。

## 交付审计

[before.json](logs/task_045/before.json) 保存开始时1101个Go/模块文件及两个只读plan.md
的逐文件哈希和Git状态。[audit.json](logs/task_045/audit.json) 保存结束时比较、fixture
SHA256、无新增Go文件及diff检查结果。Go/模块及两个plan.md内容全部未变。
未运行Go/make/failpoint。日志仅修整行末空白与多余末尾空行，未改运行结果。
全部阶段的槽位1锁已释放，缓存保留；既有编号任务删除不纳入本次提交。
