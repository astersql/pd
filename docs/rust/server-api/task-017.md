# 任务017：接通调度协调器主循环

状态：已完成。

实现基准 HEAD：`50cc43a92d43a7c846a9e83a983eb8a5bb7c26c3`。已核查任务1–4的三份清单、主题6准入说明及任务16交付。原任务1–16删除保持原样；只读 plan.md、Go/模块文件和来源史诗均未修改。

## 来源与实现

- [source-inventory.md](source-inventory.md) 任务17整行及 Coordinator/scheduler_restore 来源映射由本任务覆盖。只读 Go `pkg/schedule/coordinator.go` 的 Run/InitSchedulers/RunUntilStop 定义准备门、配置恢复、推送和退出行为；Rust 复用既有 Coordinator、SchedulerController、ScheduleController、ControllerAdmission、ControllerDispatch 与 StreamRouter。
- [regions_shared.rs](../../../pkg/core/regions_shared.rs) 将既有索引算法留在私有 RegionState，RegionsInfo/StoresInfo 克隆只共享索引句柄。根、ID、角色索引的原子操作仍在同一写锁内，role_tree 返回持有读锁的视图。保留公开 RegionsInfo 名称及原模块路径。没有复制心跳树，也没有 unsafe；核心既有索引/引用计数测试纳入 workspace 回归。
- [schedule_worker.rs](../../../pkg/runtime/schedule_worker.rs) 每任期持有一个既有 Coordinator。通过其 sticky prepare gate 后恢复、prepare 并驱动注册的同一实例；[schedule_policy.rs](../../../pkg/runtime/schedule_policy.rs) 为内建具体策略提供既有 typed context，ScheduleController 负责重试、标签门和退避。预留 ID 使用同一节点 allocator；merge 成对入队；成功记录、等待队列、推送期限及 fast influence 继续使用 canonical 控制器。500 ms 推送 tick 与分钟级 raft-kv2 slow-trend 接线由宿主驱动。
- 本任务所需 local policy future 由传入 Tokio executor 的 blocking task 在 LocalSet 上轮询；没有创建私有 runtime/thread、进程信号或全局订阅器。宿主 resources admission gate 串行化策略/rule 与心跳操作；索引锁不跨异步 I/O。Store preparation 改为在同一异步 admission gate 内读取，避免单线程宿主被同步 state 锁阻塞。
- 调度恢复在准备门通过后、持有 admission gate 时重新读取最新 PersistedConfig，而非使用 worker 构造时的旧文档；分钟级策略持久化同样读取当前文档。新增 dynamic-only snapshot 复用既有配置持久化选择规则，保留 TiKV StoreConfig。最终配置写失败仍完成初始化并暴露 schedule_error，保持既有恢复契约；执行错误可观察并在之后 tick 重试。
- [heartbeat_stream_adapter.rs](../../../pkg/server/heartbeat_stream_adapter.rs) 把 domain command 和 keepalive 接入真实 tonic 输出；原输入状态、错误 header 与 cluster-ID 检查保留。输出与输入并行轮询，合法 store 绑定可由新流替换；通道有界、发送有期限，EOF/drop 与任期取消释放背压。shutdown 先等待策略/cleanup，再 join router，最后释放集群资源。用户 Registry 无残留。
- [wire-inventory.md](wire-inventory.md) 的 RegionHeartbeat 输入仍归9；本次新增命令输出证据。调度管理 HTTP 归64，[surface-inventory.md](surface-inventory.md) C-082/C-099 的通用配置 API 仍归48。没有将 task18 的 patrol/checker 接线、完整热调度 wire 差分、主题8/9领域或主题10持久化拓扑记为本次完成；后续必须复用此同一 Coordinator 的控制器，不能再建平行调度状态。

## 真实行为证据

[独立测试](../../../pkg/runtime/schedule_worker_test.rs) 的7项 task_017_ 测试覆盖：

1. 真实 etcd 恢复 region/config，准备门前不初始化、不 prepare scheduler；真实 TCP 心跳使门通过，恢复的 evict-leader 生成 transfer-leader 并送达同一流。
2. 新双向流重绑后接收同一 operator 的后续命令；stale heartbeat 返回错误且 canonical epoch 不回退；region 与 scheduler 配置可从同一 MetaStore 读取。
3. 包含默认策略的真实装配执行，使用当前真实统计快照，不以固定空策略冒充实现。
4. 可控 Tokio 时钟按真实 worker 的 watch phase/tick publication 同步推进（不依赖固定 yield 次数）：经公开宿主入口接纳心跳，保留的共享 cluster handle 立即观察相同 leader；收到真实 operator 后更新 leader，再取消并 join，prepare 的 store control 被释放。
5. 准备门前取消，以及实际 etcd 停机期间取消：不启动策略；回收 worker/Registry/存储引用。
6. 在真实 MetaStore 的 `/config` 事务边界注入 Unavailable，保留真实 etcd 后端；恢复最终写失败仍可发送实际命令，错误保持可观察。不使用 mock 调用次数断言。
7. 用真实 HTTP TiKV `/config` 在准备门等待期间先提交 256MiB/raft-kv2，再发送心跳启动恢复；验证最终持久文档和宿主仍为256/v2，避免旧 snapshot 覆盖并发已提交配置。

初始可编译验收命令实际运行1项测试，退出101：`restored scheduler must deliver an operator over the real heartbeat stream: Elapsed(())`，0通过/1失败。失败发生在行为断言，不是缺接口编译失败或零测试。

### Go 差分

只读监督入口 `--case schedule-coordinator` 启动已有 Go PD 二进制；Rust 使用同一测试中的真实 TCP 宿主与真实 etcd。双方均使用 Serving stores 1/2、两个相同 peer、默认策略加 evict-leader、目标 region7 `[a,b)`、epoch(version=2, conf_ver=2)。Go 先通过真实 Bootstrap/PutStore/StoreHeartbeat/HTTP 创建策略，再重启同一数据目录验证恢复，随后接纳 RegionHeartbeat。

两端实际收到的完整固定 kvproto RegionHeartbeatResponse 逐字段相等：transfer-leader peer2/store2、候选 peers、target peer1、region/epoch/header及所有其他 operation 字段均比较。各自先核对真实 header.cluster_id，仅将此独立身份字段置0后比较，未丢弃其他字段。Go/Rust 同样验证 stale 输入错误；Go HTTP 检查恢复后的默认策略加 evict-leader 列表。Rust 持久 region/config、重绑和资源退出另由上述真实测试核验。

首次 Go 流握手等待未发送输入，已取消受监督进程并修正为先发送/并发轮询；随后统一初始 Serving 状态。覆盖全键空间的旧 fixture 还会触发 Go 默认 keyspace labeler 的 split operator，与本任务 eviction 争用同一 region；Go 原始日志已保留。最终使用双方相同的合法 `[a,b)` 目标和正确 epoch，保留所有默认策略，未通过关闭 Go patrol 或修改 Go 绕过。此处只验收 eviction 调度链，不声称后续 keyspace/patrol 能力等价。

协议仍固定 kvproto commit `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。已有 Go 二进制 build info 没有 VCS revision，不声称来自当前 HEAD；实际二进制、fixture及调用链源码哈希随监督报告保存。不把外部 etcd 称为 Go embedded-etcd 拓扑等价。

## 验证记录

动态原子 mkdir 领取共享槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。所有 Cargo、兼容脚本及子进程继承该绝对路径，无 target-dir 覆盖；默认 features。缓存保留，结束时释放自己的锁。

| 确切命令 | 退出码 | 实际测试数 |
| --- | --- | --- |
| 初始 `cargo test --locked -p astersql-pd-runtime --lib task_017_ -- --nocapture` | 101 | 1失败/0通过/0忽略 |
| 最终相同聚焦命令 | 0 | 7通过/0忽略 |
| `cargo fmt --all`（Rust修改后） | 0 | 格式化，不计行为测试 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态/构建 |
| `cargo fmt --all -- --check` | 0 | 静态/构建 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 静态/构建 |
| `cargo check --workspace --locked` | 0 | 静态/构建 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 28通过/11忽略 |
| `cargo test --workspace --locked` | 0 | 1973通过/12忽略 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 静态/构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case schedule-coordinator` | 0 | 1通过/0忽略 |

诊断阶段曾复现 StoreConfig 144/256 的真实覆盖竞态，已修复并新增确定性网络回归；测试读取持久 StoreConfig 时依既有契约执行 adjust。早期受控时钟输入曾误用 Storage source，改为真实 Heartbeat source 后验证准备门；全量负载又暴露固定 yield 次数不能同步 blocking-pool actor，最终改用真实 worker 进度发布后再推进时钟，断言仍为实际命令和 canonical 状态。另一次交错诊断运行出现旧依赖产物的 rustdoc 加载错误，不计为源码通过证据；最终验收使用固定源码、完全串行命令，并校验输入哈希不变。未修改的 syncutil 随机测试曾在要求首次 poll 必须 Pending 的断言失败，单独重跑通过；最终全量结果以表中记录为准。

完整命令日志、失败断言、监督报告、源码/二进制与 Go 守卫原始记录见 [压缩证据](task-017-evidence.json.gz)。忽略的测试不计通过；本任务 Go 差分由监督入口显式运行且必须恰好1通过/0忽略。静态/构建命令的0测试不算行为验收。

开始/结束含 ignored/untracked 的Go/模块文件共1983个，路径及每文件原字节SHA-256完全相同；排序JSON集合摘要 `300a4b0fdad52dd6b67f5daa919a1720ca47c97ecc7a81d364f6896f12d094a2`，Go/模块Git状态均无差异。最终串行验收输入哈希全部保持不变，监督报告只读/输入/进程组/工作目录清理均通过。压缩证据SHA-256 `1c84a43226643866d13252cc046a581b78f81b63c57ef5defada6f89deffd0a8`。最终工作区检查只保留本任务实现/证据及既有计划删除；仅提交本任务变更，完成后删除编号任务文件。

重复 Go 守卫快照按 `snapshot_ref` 在压缩证据的 `guard_snapshots` 中去重；命令日志原文完整保留。Markdown 本地链接7924项、Python AST、Bash语法和 `git diff --check` 均通过。
