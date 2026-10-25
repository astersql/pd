# 任务047：合并目标与拆分冷却

状态：已完成。实现位于 `pkg/schedule/checker/merge_decision.rs`，公开
`MergeChecker::{new, record_region_split, check}` 与 `MergeCheckContext`。
生产代码复用前序合并许可、真实 core/config/placement/labeler/filter 与
RangeOperatorFactory；没有新增 crate、依赖、feature 或 Go 变更。

## 依赖与来源

批次46由提交 `70bb72402`（schedule/checker: port merge feasibility guards）和
[任务46证据](task-046.md)确认；其125场景与完整workspace证据可查，本次包级
测试再次验证前序18项测试。已读取整体/包级只读指南和 oracle-index.json。

用户后续授权解除 Go 运行 oracle/运行差分准入限制。本任务采用**只读 Go 源码与
测试契约推导 + 实际 Rust 行为验证**，没有执行 Go，没有 Go 输出或运行差分，
不将源码推导标记为运行证据。[provenance](logs/task_047/source-provenance.json)
记录来源 commit、五个相关 Go 文件和固定 fixture SHA256，Go 输出哈希明确为 null。

来源为 merge_checker.go 的 RecordRegionSplit/Check/checkTarget，region.go 的
NeedMerge（两个阈值均为包含等号），ttl.go 的 get/UpdateTTL，以及
store_config.go 的合并后重拆分检查。merge_checker_test.go 中
TestMergeCheckerTestSuite/TestBasic 提供选择、单向、目标大小、冷却、#4616/#8405
场景；TestMatchPeers 提供真实 peer 对齐语义。TestAllowMergeCrossTable 的许可
矩阵已由任务46覆盖并在包级回归中运行。TestMain 只作 suite 入口，不算独立案例；
以上 Go 测试名称仅为只读来源定位。

## 行为与边界

[固定fixture](fixtures/task_047.json)包含43个独立场景：前后目标择优、等大选后继、
单向/双向、缺邻居、size/keys 等于/超过/零阈值、keys配置回退、空region、
无leader、down/pending/learner/副本不足、来源和目标热度、暂停、目标cap等于/超过、
500下限、store配置比例及数值转换边界、合并后size/keys重拆分检查、placement启用、
真实peer迁移、allocator失败。目标最终大小检查失败不回退；禁用大小阈值仍允许
空region，调度开关/limit由后续controller处理，不额外发明Go未有的门控。

另两个连续时序测试包含15+8=23个顶层状态断言：启动到期等号、源/目标拆分、
精确TTL等号仍有效、同批多个ID、重复刷新、间隔缩短/延长、零间隔、记录时TTL与
cache默认TTL不同、配置更新先于启动门控。时间直接传入Unix纳秒，无sleep或
随机归一化。暂停复用已验证PauseController的Unix秒契约。

成功场景逐项检查配对source/target ID、epoch、kind、默认priority、merge元数据、
active/passive步骤、命令、merge confver与每store influence；peer不同的场景走真实
operator builder并验证前置步骤存在。构建失败保留BuilderError::Allocation且不返回
部分配对；正常同peer路径禁止意外分配。规则初始化使用真实MemoryStore和
StorageEndpoint。生产合并检查无存储写入，故不增加无关持久化故障模拟。

缓存按未暂停的check同步清理严格过期项，替代Go后台GC；配置变化先重置保留项，
随后清理。Go后台GC的偶然执行时间不是本任务对照维度。宿主应周期调用以回收
过期项，不创建私有runtime或后台任务；heat集合由宿主真实statistics快照提供。

## 失败验证与错误识别

red.log/red-assert.log记录测试接线中的类型/常量API编译错误，不算行为失败证据。
[red-behavior.log](logs/task_047/red-behavior.log)为真实失败：暂未实现的check返回None，
正常tie-next和启动到期放行两个断言均失败，exit101，0通过/2失败。
随后最小实现使同一测试通过。首次green为38场景+15时序，扩展后为43场景+23时序。

[mutation runner](logs/task_047/mutation.py)逐项注入9类错误：跳过启动、源拆分、
目标拆分、单向限制、颠倒目标择优、跳过源健康、TTL等号提前过期、不重设配置TTL、
忽略目标大小cap。每项均exit101且由行为断言失败识别，非编译失败；
[结果](logs/task_047/mutation-results.json)和逐项日志均保留。finally恢复正确源码。

扩充命令/influence断言时发现无leader目标不减少leader计数，修正了测试预期；
[influence-expectation.log](logs/task_047/influence-expectation.log)保存实际失败，
它不是生产实现缺陷或故障检测证据。最终完整验证重新运行。

## 验证

[run.py](logs/task_047/run.py)在各阶段动态原子领取共享槽位，写入任务/会话/PID，
等待所有子进程结束，正常/失败/中断均释放自己的锁并保留缓存。所有实际阶段领取
槽位1：`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
最终基线使用`RUST_TEST_THREADS=2`；
[commands.json](logs/task_047/commands.json)保存确切命令、退出码、有效计数和环境。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_047 -- --nocapture` | 0 | 3通过，43固定场景+23时序断言 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 21通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1479通过，0失败，1既有忽略 |

## 差异审计

[before.json](logs/task_047/before.json)保存开始时Go/模块文件与两个只读plan.md
的内容哈希及Go/模块Git状态；[audit.py](logs/task_047/audit.py)重枚举含untracked
文件后验证1103个受保护文件完全未变，结果见[audit.json](logs/task_047/audit.json)。
不运行Go/make/failpoint，不修改Go模块、plan.md或来源主题。Cargo.lock未变。
既有其它任务文件删除排除在本次提交之外。git diff --check通过。

workspace既有忽略为独立进程测试tso_processes；没有新增忽略测试。保留既有弃用
warning，不在本任务扩散修复。所有槽位锁释放、缓存保留。日志仅清除行末空白和
多余末尾空行，不修改实际命令、测试结果或退出码。
