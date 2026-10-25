# 任务018：转移领导者步骤

状态：已完成。前置任务17的提交 `791d987ea`、审计修订 `f0821eccc`、
[task-017](task-017.md) 及真实 `OpInfluence` API 已核实；此次包级回归包含其测试。

用户授权解除 Go 运行 oracle/运行差分准入要求。本任务依据只读 Go 源码与
测试契约冻结期望，`evidence_kind=source_assertion`、`go_execution=false`。
没有执行 Go operator，没有将源码推导声称为 Go/Rust 运行差分。
已审查 `oracle-index.json`：其中 controller 并发 oracle 不覆盖本任务目标，
没有将其它任务的运行输出当作 TransferLeader 证据。

## 实现与场景

生产路径 [step_leader.rs](../../../pkg/schedule/operator/step_leader.rs)，
[领域说明](../../../pkg/schedule/operator/step_leader.rs.md)。公开
`TransferLeader::{is_finish,conf_ver_changed,check_in_progress,influence,timeout,get_cmd}`，
类型化错误 `LeaderTransferError` / `LeaderCandidateError` / `LeaderTargetError`。
复用真实 core `RegionInfo`、`BasicCluster`、配置 `SharedConfigProvider`、
hbstream `Operation` 和前置 `OpInfluence`。没有 protobuf、mock、持久化或 I/O。
时钟是显式 Unix 纳秒边界；正常领域行为不涉及 classic/nextgen 分支或新 feature。

[fixture](fixtures/task_018.json) 固定来源 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、四份只读 Go 源码 SHA256、原始输入、
源码推导期望和明确的30分钟配置。无随机/时间字段归一化，缺失 peer 保留 null。
来源 `step_test.go` 的 `TestTransferLeader` 覆盖旧 leader、主目标、其它 leader、
Down、多目标主 peer 缺失和候选无 peer/主目标 unhealthy 的组合；
`TestNeedStoreHealthCheck`（相关源码段）提供 unhealthy 开关契约。
其它边界由 `TransferLeader`、`validateStore`、`fastStepWaitDuration` 和
`store.go` 的只读源码推导。当前字节与来源 commit 的哈希已逐项核对。

独立 [task_018_test.rs](../../../pkg/schedule/operator/task_018_test.rs) 显式注册4项测试：

- `task_018_behavior_cases`：8个完成案例，包括新 heartbeat 候选完成、旧 leader、
  无 leader/零 store；恒定 confver=0、字符串；2种conf-change模式的完整命令、
  重复/缺失候选与缺失主 peer；发送命令后旧 region 不会自行完成。
- `task_018_candidate_health_and_errors`：15个表驱动案例各重复2次，验证候选列表
  不变；缺 peer、learner、缺 store、Down、unhealthy 开关、严格10/30分钟边界、
  未来 heartbeat、候选与主目标不能跨店拼接、重复失败项顺序及主目标回退。
- `task_018_influence_and_timeout`：3组完整 store 影响量/成本快照（候选去重、
  候选含源、源=主目标、无候选），7个超时案例（负值、零、100/101/102 MiB、
  大 region 及 i64 最大值纳秒 wrapping）。仅主目标增加 leader 数量与大小，
  所有不同候选各收费1000，其它资源字段不变。
- `task_018_role_and_signed_boundaries`：5种角色，包括 learner 拒绝、联合/未知
  非 learner 通过；witness/pending/down peer 不额外过滤；2个有符号配置/时钟
  案例、3个零/负/i64最小 size 影响量案例；直接检查错误枚举、store ID 和顺序。

## 红绿证据

首次注册测试时缺少 API，命令退出101，见
[red-missing.out](logs/task_018/red-missing.out)；此编译失败不代替行为红灯。
最小实现后，[red.py](logs/task_018/red.py) 临时注入“遗漏 ToStores 完成判定”，
同一固定行为测试在 leader=3、主目标=2、候选=[3] 时断言 `false != true`，
退出101；另外2项测试通过。脚本 finally 恢复生产实现。
[red.json](logs/task_018/red.json) 和 [输出](logs/task_018/red-behavior.out) 保存命令、
退出码和具体断言。最终完整测试覆盖该断言并通过，未从 Rust 实现生成期望。

## 验证

[validation.json](logs/task_018/validation.json) 和 `validation-*.out` 保存每项完整
stdout/stderr、确切命令、退出码和测试数量：

| 命令 | 退出码 | 结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_018 -- --nocapture` | 0 | 4通过、0失败 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 16通过、0失败 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1379通过、0失败、1既有忽略 |
| `git diff --check` | 0 | 通过 |

[readonly-validation.py](logs/task_018/readonly-validation.py) 与
[reuse-go/go](logs/task_018/reuse-go/go) 沿用任务17验证路线，只复制任务14已有
etcd fixture 二进制，拒绝真正的 Go 编译/其它命令。此二进制仅服务真实 Rust
存储测试的外部 etcd I/O，不是 operator oracle。哈希、原始构建请求与复用记录见
[provenance](logs/task_018/fixture-reuse-provenance.json)、[go-reuse.jsonl](logs/task_018/go-reuse.jsonl)。

全部 Cargo 阶段由 [slot-run.py](logs/task_018/slot-run.py) 原子 mkdir 动态领取，
实际均为槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
等待进程结束后清理自己的锁，保留缓存，信号和失败路径均有 finally 清理。

[verify.py](logs/task_018/verify.py) 对照任务开始时的只读全量文件清单，核对
Go/模块内容、Git状态、两份只读 plan 和 Go 来源 commit/hash；实际摘要保存在
[verification.json](logs/task_018/verification.json)。Cargo.lock 仅新增两个现有
workspace 路径依赖，无远程依赖版本变化。全 workspace 格式化未触及其它任务。

仅提交本任务实现、测试、接线、说明和证据，排除开始时已有任务10–17的删除。
不修改整体/包级 plan 或来源主题，完成后删除任务18编号文件。
