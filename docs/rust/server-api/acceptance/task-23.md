# 第 23 项 HTTP 矩阵与指标生命周期

已完成。起始 HEAD 为 `d6c50a7bbdca9ef66efba45b39237632fae92b72`。
批次 22 交接固定协议和 Classic PD。本项核对了批次 8–13 的 HTTP 公共接口、Go 来源和真实
WAL 回执，并重新执行对应入口。回放矩阵列出 32 个 HTTP 来源 ID 和维护指标 M-037。
目录保存 759 个来源行和原拥有者的原文。
[批次 22](task-22.md)、[责任交接](handoff.md)、[回放矩阵](http-regression.md)。

原始回执保存精确命令和退出码、源码及产物摘要、Go 与 Rust 原始 HTTP/RPC、真实 etcd
持久值与 revision、指标抓取、失败日志、只读守卫和槽位清理。
SHA-256 为 `ae602ca2493cf45ef0752872d1080af28e16e050dc5447c43154d0b6e78931bd`。[原始回执](task-23.json.gz)。

## 回归结果

首次预跑在更新监督器期间触发输入指纹守卫。当前子进程退出后停止该轮，未用作验收。
正式 `task_101_` 执行全部 59 个监督器，测试结果为 14 通过、1 失败、0 忽略，退出 101。
失败为配置夹具等待不可能到达的调度阶段，以及 M-107 观察到 Go 有
`admin-add-peer/not-found` 而 Rust 缺少该样本。

- 三个未 bootstrap 的配置夹具改为等待 `Preparing`，保留完整配置比较、
  事务故障、取消、并发及读取不修改持久状态的断言。配置查询的双实现回放和
  相邻配置故障用例通过。
- 指标专门测试在修复前复现 `not-found` 增量 0、预期 1。
  Rust 现有 dispatch 在查不到 region 时记录该指标。真实 HTTP 删除缓存后的
  Go 和 Rust `not-found`、`disappear` 增量均为 1。
- 按 case 替换配置、service-metrics、缓存删除和 operator 写入四份报告，保留首轮原文。
  本轮 59 个业务报告最终均退出 0，7 组新增领域执行 18 项测试。
  显式 ignored 复核读取上述精确集合并比较 54 个已到达的 shared family。
  缺少领域回放、缺失报告、重复报告、陌生 case 或非零业务退出均被拒绝。
- 维护任务的 11 个双实现请求逐次保存 HTTP 和指标原文。创建置 1，
  冲突保留原值，删除置 0，宿主关闭后 Registry 为空。
  GC 屏障标签在宿主关闭后消失，旧 store 快照无法复活两个已删除指标序列。
  事务计数在真实 etcd 停止和取消后为成功 3、失败 2，WAL 恢复后仍能读取原提交。

## 命令和有效测试

| 命令 | 退出码 | 通过 / 失败 / 忽略 |
| --- | --- | --- |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `cargo test --locked -p astersql-pd-compat-tests --lib task_101_ -- --nocapture` | 101 | 14 / 1 / 0。全矩阵首次运行 |
| `python3 scripts/http-regression-oracle.py --case <case>`（四个受影响入口） | 均为 0 | 各入口及内部测试计数见原始回执 |
| `cargo test --locked -p astersql-pd-compat-tests --lib task_101_ -- --nocapture --skip task_101_behavior` | 0 | 14 / 0 / 0 |
| `cargo test --locked -p astersql-pd-compat-tests --lib task_102_ -- --ignored --nocapture` | 0 | 2 / 0 / 0 |
| `cargo test --locked -p astersql-pd-server --lib task_047_ -- --nocapture` | 0 | 2 / 0 / 1。ignored 差分由配置监督器显式执行 |
| `cargo test --locked -p astersql-pd-server --lib task_048_ -- --nocapture` | 0 | 2 / 0 / 1。ignored 差分由调度配置监督器显式执行 |
| `cargo test --locked -p astersql-pd-server --lib task_078_ -- --nocapture` | 0 | 2 / 0 / 1。ignored 差分由 v2 监督器显式执行 |
| `cargo test --locked -p astersql-pd-server --lib task_102_maintenance_etcd_wal_restart -- --nocapture` | 0 | 1 / 0 / 0 |
| `cargo test --locked -p astersql-pd-schedule-operator --lib task_036_ -- --nocapture` | 0 | 11 / 0 / 0 |
| `cargo test --locked -p astersql-pd-gc --lib mcs_053_ -- --nocapture` | 0 | 8 / 0 / 0 |
| `cargo test --locked -p astersql-pd-runtime --lib mcs_053_host_metrics_lifetime -- --nocapture` | 0 | 1 / 0 / 0 |
| `cargo test --locked -p astersql-pd-runtime --lib task_101_transactions_success_fault_cancel_and_recovery -- --nocapture` | 0 | 1 / 0 / 0 |
| `cargo fmt --all`、`cargo fmt --all -- --check`、`cargo metadata --format-version 1 --no-deps`、`cargo check --workspace --locked`、`git diff --check` | 均为 0 | 静态 |

正式矩阵之后只重跑受影响的监督器，随后校验全部报告。配置夹具和 dispatch 指标接线
的源文件摘要在回执中按阶段保留。完整工作区行为回归归批次 24。

各阶段动态领取槽位 1，
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
Go 基准为准入的固定 kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`、
`without_dashboard` amd64 产物。SDK、协议 ZIP、PROTOC、官方 etcd 3.5.15 和
etcdctl 均按准入摘要复核。Go 源码及模块 1101 个路径首尾字节和 Git 状态一致，
40 个计划摘要一致。Go 命令仅作只读版本查询，差分运行现成 Go 二进制。
验证子进程退出后释放自有锁并保留缓存。
