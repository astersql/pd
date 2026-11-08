# 第 21 项配置、指标与命令覆盖审计

审计基准 `ab03aa33858110882818c0df586e3af515a47ee1`。
[批次 20](task-20.md)提供当前协议注册、同输入 HTTP 和存储证据。
[批次 3–18 的回执](handoff.md)参与当前来源指纹核验。原
[表面清单](../surface-inventory.md)、[102 历史回执](../task-102-resume-evidence.json.gz)
及 [coverage.json](coverage.json) 的冻结数据保留原值。
`surface_audit_21` 指向[机器回执](task-21.json.gz)。

## 来源与准入

- 解析原 Markdown manifest 的路径链接、SHA-256 和 owner，核对任务 1 冻结集合及审计前原文。
  原 267 条路径全部存在，230 条字节一致、37 条漂移。另核验 193 条当前来源，
  涵盖配置、CLI、指标、MCS 接线、测试、运行依赖及前置回执。回执逐路径记录原值、
  当前原始字节摘要、变动归因和新增原因。
- 审计 2394 条有 owner 的表面声明和调用行，按行保留原责任、候选来源、
  运行场景或缺少的具体证据。18 行关联本轮探针。其余逐行保留未准入原因，
  领域、未复验命令作用域和安全边界由原任务验收。
- 表面清单由
  `a64666b05bd794e4e32303a16fd62f361f97e255fd97b84e2c3e0708b6c677e4`
  更新为
  `8ddc01727e37af6704b5d7c46fa2fcf59c126cf9b628fed654bb319c1964c724`。
  原 manifest 集合摘要仍为
  `ce6e64c8cf56aced73e1bb88d6c4eaf14796cfa411d813d9ede578ace3e4843e`。
  第 19、20 项按相邻交接核验原输入和本次变更字节，两个历史回执保留原样。

## 当前运行

使用固定 kvproto 的 Go PD 和当前 Rust PD。classic 与 API 两种模式均启动真实
进程，读取配置 HTTP 和指定日志文件，确认 CLI 覆盖 TOML 名称、日志级别和文件。
首轮 Rust 已向指定文件写日志，配置 HTTP 的文件名仍为空。修复后，运行时将
已经生效的启动日志配置投影到配置 HTTP。独立探针也覆盖 `-V`、`-h`、
`--config-check`、无效配置优先于版本输出、未知参数和退出通道。
帮助和错误文案存在实现差异，原始输出单独保留。

- 大小写混合的 `PD_SERVICE_MODE=ApI` 启用 API 模式。无 TSO 进程时，
  `GET /pd/api/v2/ms/members/tso` 在 classic 返回 404 和拒绝字符串，
  在 API 返回 200 和 JSON null。两边都执行相同探针。
- 每个进程实际执行 AllocID，回执保留请求及响应的 protobuf 字节和真实 etcd
  键值、修订号。`pd_cluster_id{type="idalloc"}` 的 HELP、TYPE 及采样均与
  实际分配窗口对应。四次调用均返回 1001，etcd 窗口及指标均为 2000。Go 额外暴露 `keyspace-idAlloc` 零值，本场景未触发该分配器。
- classic 两边都暴露 `pd_tso_role{group="0"} 1`。API 模式下 Go 暴露角色零值
  和事件标签，Rust 无对应系列。这些是指标差异，完整生命周期和 HTTP 矩阵归第 23 项。
- 既有只读脚本复验配置校验、CLI 退出、信号和 17 组日志管理 HTTP。
  启动配置重启探针保留同输入 HTTP 及两次真实 etcd 读回的空配置记录。
  日志管理的热过滤与配置 HTTP 的启动日志字段分别记录。
  配置回执还保留 Go 的日志轮转默认大小 300 和 Rust 的 0。

## 验证

机器回执保存确切命令、退出码、有效测试计数、二进制及源输入指纹、失败日志和
原始 HTTP/RPC/etcd 观测。负向对照在真实清单副本上删除、重复一条记录，模拟
当前输入字节变化，以及把已通过的业务回执替换为仅指标回执。

使用共享槽位 1，`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
预检逐项核对 Go PD、Go SDK、固定 kvproto ZIP、protoc 和官方 etcd 3.5.15/etcdctl。

首次 `sa_021_` 执行退出 101，1 项缺少当前审计回执。首次真实探针
退出 101，1 项因配置 HTTP 返回空日志路径失败。两次均实际执行断言，
原日志随回执保存。一次运行外层脚本在子测试结束后退出 127，随后独立验证
运行输入首尾摘要一致。子测试本身的通过和忽略计数按原日志记录。


相邻批次 19 的首次运行因四个业务回执共享的运行时输入漂移退出 101，
1 通过、1 失败。复跑旧版 GC、原生 WAL、SearchLog 和同架构 ServerInfo，
每项退出 0、1 通过、0 忽略。真实 WAL 在 SIGKILL 后保留 515032 字节，
重启读回 MetaStorage 修订号 11，AllocID 从 1 前进到 1001，TSO 前进。
Native 集成测试重建了 arm64 调试产物，各场景按实际二进制摘要登记。
ServerInfo 临时切换到 x86_64 产物，并在退出后恢复 Native 集成阶段的 arm64 产物。
复跑前后重新核验批次 19 的 3034 个运行输入。

| 检查 | 退出码和结果 |
| --- | --- |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_021_ -- --nocapture` | 0，2 通过、0 失败、0 忽略 |
| `cargo test --locked -p astersql-pd-compat-tests --lib task_102_source_ -- --nocapture` | 0，2 通过、0 失败、0 忽略 |
| 同包 `sa_019_`、`sa_020_` | 均为 0，各 2 通过、0 失败、0 忽略 |
| `surface_21_live` 显式执行 | 0，1 通过、0 失败、0 忽略 |
| 启动配置重启探针和相邻 HTTP 配置测试 | 均为 0，各 1 通过。HTTP 配置套件另有 1 项既有默认忽略 |
| 现有 CLI 配置、退出和日志只读脚本 | 均为 0，分别复验 9 组配置、18 组命令及额外信号场景、17 组日志管理请求 |
| `cargo metadata --format-version 1 --no-deps`、`cargo check --workspace --locked`、`cargo fmt --all -- --check`、`git diff --check` | 均为 0 |

复核压缩回执后，`sa_021_` 和 `sa_019_` 各 2 项通过。测试及子进程均已退出，
槽位锁已释放，保留缓存。1101 个 Go 源码及模块路径的原始字节和 Git 状态首尾一致。
守卫摘要为 `e44b662b8157050be6c84d41925cd8b9d63450ca5e35637726a403c248fdfc64`，主计划原摘要核验通过。
