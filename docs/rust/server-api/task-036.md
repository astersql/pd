# 任务036：classic TSO 流与最小时间戳

状态：已完成，待回归。基准 `87cce7d5f6a038fe16087ea01e613959c91ceeef`；依赖任务35与任务1–4的清单、主题6领域准入和只读 oracle 入口已核对。协议固定 kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`，默认 features，无依赖变更。

## 行为与来源

只读 Go 契约：[Tso 与 GetMinTS](../../../server/grpc_service.go)、[Allocator.GenerateTSO](../../../pkg/tso/allocator.go)、[getTS 与逻辑溢出](../../../pkg/tso/tso.go)、[错误常量](../../../pkg/errs/errno.go)。classic Tso 按消息先检查集群 ID，再调用分配器；分配器先检查领导权，再检查 count。失败使用 Unknown。GetMinTS 先执行 unary 角色校验，再从同一分配器取一个时间戳。classic Go 的此分支忽略 internalErr，返回分配器给出的 timestamp；Rust 在没有成功分配值时保留零 timestamp 和无错误 header 的形状。deprecated dc_location 在当前 Go classic 分支被忽略。

[最小时间戳适配](../../../pkg/server/grpc_tso.rs)、[Grpc](../../../pkg/server/grpc.rs) 与实际同端口 [SyncGrpc](../../../pkg/server/sync_transport.rs) 接线复用 host-owned Oracle。流仍由响应流拥有全部工作，按需读取下一条输入；分配等待也受宿主取消控制，无独立排队 worker。[宿主分配入口](../../../pkg/runtime/tso.rs) 复用 canonical Oracle 的 fenced allocate/update，在持久窗口用尽后续期重试，保留领导权优先于 count 的顺序及 Go 错误文本。[角色服务](../../../pkg/runtime/member_role_worker.rs) 不再把 TSO 的分配器错误转换成普通 unary 的角色错误。没有复制领域算法、增加私有 Tokio runtime 或引入协议到领域层。

覆盖 [source-inventory.md](source-inventory.md) 的任务36、grpc_service 的 Tso/GetMinTS 和共享 tso allocator/oracle 来源行，以及 [wire-inventory.md](wire-inventory.md) 的 Tso/GetMinTS 行。共享 tso 目录的 MCS 转发、keyspace group 最小值聚合仍依赖主题8；ResetTS/管理接口由75/76等原 owner负责，不计为本次完成。[surface-inventory.md](surface-inventory.md) 中指标、限流、配置、审计由原 owner验收。HTTP 观察为真实同端口 ping，没有增加产品 HTTP 接口。

## 行为证据

[真实宿主 TCP 测试](../../../pkg/server/grpc_tso_test.rs) 在同一 MetaStore/Oracle 上执行 [共享场景](../../../pkg/server/grpc_tso_scenarios_test.rs)：未 bootstrap 的 GetMinTS、Tso/GetMinTS 交错严格单调、批量范围无重叠、count=1/2/100/2^18−1、零/2^18/u32::MAX 错误、错误集群 ID、旧 dc_location、多次同流请求、EOF 和客户端取消后继续分配。响应 header、count、suffix_bits 都明确断言。初始测试可编译，因 GetMinTS 返回 Unimplemented 而行为断言失败，退出101，1失败。

[可控时钟与真实 etcd 测试](../../../pkg/runtime/grpc_tso_clock_test.rs) 在同一真实 TCP router 上装配 canonical Oracle 和生产宿主分配入口：时钟1000ms、3ms保存窗口，验证逻辑溢出到1001ms、窗口续期到1002ms，以及时钟回退至500ms后仍推进到1003ms。读取实际持久窗口，并改写 etcd timestamp 值触发真实 compare/fencing 失败；GetMinTS 返回零 timestamp，Tso 返回 Unknown，恢复原窗口后可继续。重建 Oracle 放弃旧持久窗口，时间戳不重叠；真实 resign 后 Tso 返回精确领导权错误，GetMinTS 返回 Unavailable。持续发送而不消费响应时，比较两次 GetMinTS，证明 Oracle 停止处理该流；丢弃流、取消宿主后服务退出且 join 完成。此测试没有以 mock 调用次数代替状态。

背压探针使用65,535字节接收窗口和1ms发送间隔；最初无间隔洪泛在 HTTP/2 层发生 connection reset/broken pipe，不能当作背压成功。增加终止诊断后，最终要求流保持存活、Oracle 停止推进才通过。不宣称已验证任意请求洪泛下的连接保活或吞吐性能。

[真实双实现差分](../../../pkg/runtime/grpc_tso_differential_test.rs) 通过 [只读监督入口](../../../scripts/pd-compat-readonly.py) 复用已有 Go PD 二进制。两端运行同一成功、count边界、多批次流、客户端取消场景，比较精确状态码/错误文本；双节点 follower 验证零count/正常count/超大count均优先返回领导权错误，GetMinTS 的角色错误优先于缺失header。两端检查实际 HTTP ping；真实 Go 重启后时间戳继续单调。独立时钟与随机集群ID不逐值相等，分别校验完整 header、批次区间及跨 RPC 顺序；不比较内部重试耗时和未返回的时间戳。Go 内部存储故障未注入；该分支由源码映射与 Rust 的真实事务失败验证覆盖，不称作双端故障差分。

初轮实现删除了 select 后多余的 await 后才编译通过。差分先修正 harness 对首响应头错误的处理，再由实际 Go 输出修正领导权错误中的“is”；新增 duplex 场景曾在等待 Go 响应头前未发送首请求，经监督入口 SIGTERM 清理（退出130），调整为先发送首请求后通过。包级回归还发现任务18的旧断言把 TSO follower 错误写成 Unavailable，现按真实 Go 证据更新为 Unknown。集成测试的域请求计数也从2更新为4：零count与超大count现在进入带领导权校验的分配器，随后正常请求和关闭行为验证通过。

## 验证记录

所有阶段从槽位1–10依次原子 mkdir 动态领取，实际槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。锁记录计划、任务、会话和持有 PID；全部 Cargo 和子进程继承此绝对路径，正常阶段结束释放自己的锁；本任务没有删除槽位缓存。末次全局回归期间外部进程删除了缓存及占用记录，详见下文。最终验证执行器同时设置 `PD_COMPAT_MODE=dual`、`PD_COMPAT_GO_BIN=$PWD/bin/pd-server`、`PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server`，工作区中读取这些变量的既有健康差分也使用真实二进制。完整命令、日志、退出码、Go首尾清单和监督报告归档于本任务压缩证据。


| 确切命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_036_ -- --nocapture` | 101 | 1行为断言失败，非编译错误 |
| 最终同一聚焦命令 | 0 | 1通过，0忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_036_ -- --nocapture` | 0 | 1可控时钟/事务/背压/取消测试通过，0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 148通过（147单测、1集成），0忽略 |
| `cargo test --locked -p astersql-pd-server --test server -- --nocapture` | 0 | 1集成测试通过 |
| `cargo fmt --all`；`cargo fmt --all -- --check` | 0 | 静态 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN="$PWD/bin/pd-server" PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server" bash scripts/run-pd-compat-readonly.sh --case classic-tso` | 0 | 1真实双节点差分通过，0忽略 |

最终监督报告 `compat-readonly-1793285947561862000.json`：exit=0，readonly_unchanged、inputs_unchanged、work_directory_cleaned 和 commands[0].process_group_cleaned 均 true。该报告产生后曾再次逐文件核对 inputs 哈希，结果一致。SIGTERM取消的报告 `compat-readonly-1793285644486549000.json` 单独保留，不算行为通过。

Go oracle 为既有 `bin/pd-server`，go1.26.8 构建，固定 kvproto 模块 `v0.0.0-20261016101550-f8ad079c8e98`；SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`。build info 无 VCS revision，不声称来自当前 HEAD；监督报告被外部缓存清理移除后，已重新只读采集 Go build info 和二进制哈希，保存在压缩证据中。


`cargo check --workspace --locked` 退出0。`cargo test --workspace --locked -- --test-threads=1` 未通过：已执行350项通过、2项失败、1项忽略；失败均在既有 election watch_reset 测试启动 fixture 时发生 ENOENT，未到行为断言。单独重跑 `cargo test --locked -p astersql-pd-election --lib -- --test-threads=1` 又在编译阶段因刚生成的 liblog rmeta 不存在退出101（0行为测试）；此时观察到非本任务的 `rm -rf ./target` 进程仍在删除共享缓存。再次工作区重建也受同一清理影响，主动 SIGTERM 本任务 runner，外层退出130；自有 Cargo 已结束，自己的槽位锁已释放，没有停止外部进程。

环境异常也删除了之前的5份监督报告原文件及 Rust 二进制缓存；此前报告的成功与守卫结果已实际读取核实，原始命令输出仍完整保留，但不能声称原始 JSON 仍在归档里。压缩证据明确标注缺失，并记录删除前观察到的最终报告字段。缓存清理结束后必须重新领取槽位，重建 pd-server、运行 `--case classic-tso` 并立即归档新监督报告，再重跑工作区测试。已有专项、148项包级及静态证据有效；本任务按用户授权标记“已完成，待回归”，保留编号任务文件，不宣称整个工作区验收完成。


开始/结束1983个 Go/模块文件（含 ignored/untracked）的路径、SHA-256 和 Git状态完全一致；plan.md 与初始快照逐字节一致。未运行 make、failpoint 启停、Go测试、Go格式化或模块整理。Python AST、本文链接与 `git diff --check` 通过。其他会话新增的三个2026-10-29计划目录原样保留，不纳入本次提交。

完整命令日志、退出码、槽位执行器、首尾Go清单、最终源码SHA-256及报告缺失说明见 [压缩证据](task-036-evidence.json.gz)，SHA-256：`d1937691f4922a433f36bfeb05c21d07e7f690b20be44f129b2b753070b069a4`。工作区回归仍待环境恢复，编号任务文件保留。
