# 核心领域模型主题回归

日期：2026-10-23（Asia/Shanghai）。批次 69，依赖批次 68。
验收实现 HEAD：`7e4a00d8a`；Go 规划基准：
`0820f0b67b0e49b289b84fdf66b7b8fb178afe82`。
状态：已完成，关闭渐进迁移主题 2「迁移核心领域模型」。
本次仅回归与审计，没有新增生产行为，TDD 不适用。

## 依赖和覆盖

批次 68 已提交完整[公开能力覆盖审计](core-coverage-audit.md)，含 591 项
Go 生产 func/type 声明索引、前序红绿记录追溯及必要的 Rust API 修复。
结合 [core 契约](core-contract.md)、[storelimit 契约](storelimit-contract.md)、
[response 契约](response-contract.md) 核验当前公开模块与测试注册。
契约文档中的“尚无 Rust”“补 oracle”属于历史规划快照，以覆盖审计中的
当前符号、测试、源码依据和责任边界为准，未改写三个只读契约。

本轮有效测试包含：Store 状态/统计/clone/score/limit，Region peer/epoch/
flow/clone/inherit/bucket/guide，range tree 与角色子树更新、乱序收敛、
BasicCluster 聚合，server 协议适配及 response JSON/取消/状态名。
批次 68 补齐的 `witness_score`、`stats_mut`、`RegionOption`、
`set_store_meta`、`set_new_store_stats`、`set_network_slow_triggers`、
`RegionsInfo::put_region` 和 `marshal_region_info_json_with_cancellation`
均在当前 workspace 测试中实际通过。

三个主题 crate 共 200 项有效测试（core 160、storelimit 21、response 19），
无失败、无忽略。扫描这三个目录的 Rust 源码未发现 `todo!`、`unimplemented!`
或忽略测试。64 个 JSON fixture 均可解析；fixture 本身及文件数量不作为行为
通过的替代证据。逐场景 Go 来源、推导与历史运行边界见覆盖审计和相邻说明。
既有 Go oracle 已按用户要求删除，本轮没有恢复或新建任何 Go oracle。

## 本轮命令及结果

在仓库根原子 `mkdir target/rust-slot-locks/slot-N.lock` 动态领取 **槽位 1**，
记录任务/会话与持有 shell PID。以下全部 Cargo 命令继承：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
没有覆盖 target-dir；全部子进程结束后由 EXIT trap 删除自己的 owner 和锁目录，
保留编译缓存。

| 确切命令 | 退出码 | 实际结果 |
| --- | --- | --- |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace/targets 可解析 |
| `cargo fmt --all -- --check` | 0 | 格式通过；本轮无 Rust 源码修改 |
| `cargo check --workspace --locked` | 0 | 全 workspace 检查通过 |
| `cargo test --workspace --locked` | 0 | 446 passed、0 failed、1 ignored；76 个结果目标，含零测试 doc-test |
| `go test -mod=readonly ./pkg/core/... ./pkg/response -count=1 -v` | 0 | 4 个包，69 个顶层测试通过，含子测试共 85 条 PASS，0 skipped |
| `cargo tree --locked -p astersql-pd-core -e normal` | 0 | 传递依赖审计通过 |
| `cargo tree --locked -p astersql-pd-storelimit -e normal` | 0 | 传递依赖审计通过 |
| `cargo tree --locked -p astersql-pd-response -e normal` | 0 | 传递依赖审计通过 |
| `git diff 0820f0b67b0e49b289b84fdf66b7b8fb178afe82 -- pkg/core pkg/response` | 0 | 差异为 Rust 新增实现/测试/说明；Go 源码无差异 |
| `git diff --check` | 0 | 无空白错误 |

Rust 唯一既有 ignored 为 `tests/rust-compat/tso_test.rs::tso_processes`，
要求另行 Go/Rust 进程测试，属于 TSO 服务主题；不计入 446 项，不据此宣称
TSO 进程兼容验收完成。check/test 的既有 warning 为 server adapter 读取弃用
协议字段 cpu_usage，没有隐藏失败。

## 正常依赖审计

审查实际传递树而非仅 manifest。三棵树均无 `prost`、`tonic`、`axum`、
`etcd-client`、`astersql-pd-proto`、`astersql-pd-server`、`astersql-pd-storage`。
core 正常直接依赖 btree、core-constant、movingaverage、storelimit；
storelimit 依赖 core-constant、ratelimit。response 依赖 core、core-constant、
typeutil、serde、serde_json。经 ratelimit 的传递依赖包含 Tokio、syncutil、
Prometheus 和 errs，未反向进入协议/传输/存储层。协议转换留在 server adapter。

## Go 只读与故障时序证据边界

遵守用户最高优先级约束，用只读 `go test -mod=readonly` 替代任务中的
`make gotest`。未运行 failpoint-enable、failpoint-disable 或任何 Go 注入改写。
运行前后逐文件 SHA-256 核验现有 Go 文件及根/client 模块配置无变化；
基准后的 `pkg/core/**/*.go`、`pkg/core/*.go`、`pkg/response/*.go` 和根模块
配置 diff 无输出。因此不存在本轮遗漏的基准后新增 Go 行为。

- `concurrentBucketHeartbeat`（region.go:680）：Go 先读 old 并检查版本，较新
  版本到达注入点后延迟 500ms，再 CAS；并发写入可令 CAS 失败，由心跳调用方重试。
  `server/cluster/cluster_test.go::TestConcurrentReportBucket` 的 v3 延迟、禁用、
  v2 插入、join 后 v3 断言仅作本轮只读源码依据，未运行其注入路径。
  Rust `core_task_34_competing_snapshots` 实际通过：两个线程共享 old，屏障后竞争
  `update_buckets`，恰好一个成功；后续版本更新收敛到 u64::MAX。
  `compare_and_set_report_buckets` 使用独占借用并由调用方锁串行化，未复现 Go
  乐观 CAS 的中途失败；完整心跳重试装配仍属后续服务主题，不冒充运行差分。
- `UpdateSubTree`（region.go:1419）：root 更新后、子树锁前，仅 origin==nil
  延迟 1s。Go `TestSetRegionConcurrence` 本轮通过，但没有注入生成，不能作为
  延迟交错覆盖。Rust `core_task_48_eventual_repair_with_barrier` 实际通过：
  暂停旧事件、应用新 root/子树、恢复旧事件，再重放校正并检查 pending 与引用计数；
  配合 task 47 角色/overlap 测试验证真实状态，没有以 sleep 或 mock 次数替代。

已有 Go 测试执行覆盖、源码推导 fixture 和 Rust 行为测试分别作为证据。
本轮没有新的全场景跨语言运行差分；禁止新增 Go oracle 本身不构成阻塞。
未声称迁移完整调度、网络心跳路由、全部指标宿主装配、全局 Runner 或服务重试管线。

## 关闭结论

在用户明确的只读 Go 验证规则下，主题内真实领域 API、DTO、适配边界、
当前回归、依赖树与基准后差异复核均满足验收。来源主题任务可标记完成并删除，
批次 69 随完成报告删除。两个 plan.md 和其他会话的任务改动保持原样，不纳入提交。
