# 任务037：废弃RPC与明确不支持的流

实现基准 `0c6cf87e6f5d3aa04dac05b268afe7d93a21f260`。状态：已完成。本任务仅验收四个固定契约，不解除其余GC领域准入。

## Go契约与最小实现

任务标题中的“SyncMaxTS/GetDCLocationInfo不支持”不符合固定Go源码。实际 [SyncMaxTS/GetDCLocationInfo](../../../server/grpc_service.go) 返回成功、当前集群ResponseHeader及其余字段零值，忽略请求header、旧DC和max_ts参数。它们不校验leader/bootstrap，也不分配或同步时间戳。[WrapHeader](../../../pkg/utils/grpcutil/cluster.go) 在cluster ID为零时返回NOT_BOOTSTRAPPED及精确文本 `cluster id is not ready`，这是响应头错误，不是gRPC错误。

真正固定返回Unimplemented的是 [WatchGCSafePointV2/WatchGCStates](../../../server/gc_service.go)：分别为 `WatchGCSafePointV2 is obsolete. Poll GetAllKeyspacesGCStates instead if necessary` 和 `WatchGCStates is not supported yet`。这两个方法不读取GC领域状态，不创建watch worker，无需虚构主题8领域能力。任务78保留GC领域归属，本任务只完成其固定拒绝契约。其他仍未实现的GC方法没有改成成功。

[固定契约辅助函数](../../../pkg/server/grpc_unsupported.rs)、[Grpc适配](../../../pkg/server/grpc.rs) 与实际监听器使用的 [SyncGrpc委托](../../../pkg/server/sync_transport.rs) 复用宿主identity；没有新增领域算法、runtime或持久化实现。零cluster ID分支有精确单测；已ready的真实Go进程始终已有cluster ID，因此该分支仅有源码映射与Rust单测，不宣称运行差分覆盖。

## 来源与归属

已读取只读plan.md、任务1–4清单和任务36证据。任务36专项与包级已通过，历史工作区回归因外部删除target缓存待重跑，本次保留其任务文件和历史记录。

覆盖 [source-inventory.md](source-inventory.md) 的37行、`pkg/proto/lib.rs` descriptor行及共享`server/grpc_service.go`声明；补充`server/gc_service.go`两个固定错误方法。[wire-inventory.md](wire-inventory.md) 的SyncMaxTS、GetDCLocationInfo行及两个GC watch的固定错误证据已更新。其余GC字段/事务/领域操作仍归78，IsSnapshotRecovering归74。[surface-inventory.md](surface-inventory.md) 无任务37独占配置、指标或命令行项，共享观测/安全行为保留原owner。

固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`；默认features。[独立测试](../../../pkg/server/grpc_unsupported_test.rs) 从真实FileDescriptorSet验证全部56个PD方法有且仅有一个owner，和清单逐行一致；已实施owner必须在SyncGrpc有显式接线。真实TCP遍历剩余16个未实施方法，要求Unimplemented且非成功；这只是防止假成功的迁移缺口检查，不能算作这些方法与Go兼容。两个GC watch另有精确错误差分。prost-types和tonic-prost仅作为已有workspace依赖加入测试，Cargo.lock仅新增server依赖边，没有新版本或生产依赖。

## 行为证据范围

[共享场景](../../../pkg/server/grpc_unsupported_scenarios_test.rs) 在真实TCP服务上比较完整protobuf响应：缺失/正确/错误cluster header，skip_check两种值，最大physical/logical/suffix_bits，以及空/global/未知含NUL的DC。GC流方法比较Unimplemented code与完整message，兼容错误出现在初始headers或stream terminal的两种客户端观察位置。客户端任务abort后允许取消与即时响应之间的竞态，后续请求必须正常；不声称这个瞬时RPC具有可控的服务端暂停或证明每次abort均已发送RST。

Rust测试读取真实etcd `/pd/3600/timestamp` 的完整entries，确认这些请求前后持久TSO窗口未变化，并检查同端口HTTP ping和宿主shutdown/join。无状态RPC无业务写入事务，不人为注入不存在的事务故障。[差分](../../../pkg/runtime/grpc_unsupported_differential_test.rs) 在Rust与既有Go各自leader/follower上运行同一场景，并通过follower GetMinTS拒绝确认角色；leader随后仍能分配远小于注入极值的TSO。Go内部持久存储未直接读取，不把此运行性质写成Go磁盘逐字节差分。

失败验证均先编译成功：第一轮真实SyncMaxTS返回默认tonic Unimplemented而非成功，退出101、1失败；补充GC watch测试后得到默认 `Not yet implemented` 而非Go原文，退出101、1通过/1失败。没有用编译失败冒充行为失败。新增测试依赖后两次`--locked`拒绝更新依赖边，随后`cargo check --offline -p astersql-pd-server --tests`更新锁并通过；这两次退出101仅是锁文件前置检查，不是行为证据。

## 验证记录

所有阶段依次用原子mkdir领取槽位，实际均为槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。锁记录任务、计划、会话与持有PID，所有Cargo和监督子进程继承此绝对路径。Rust修改后先执行`cargo fmt --all`。第一次聚合阶段在包级150项全部通过且Cargo退出后主动停止外层脚本，补入新发现的GC watch契约；该外层退出-15不计为完整验证通过，锁在子进程结束后释放。最终阶段重新执行全部命令。

| 最终命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 静态 |
| `cargo test --locked -p astersql-pd-server --lib task_037_ -- --nocapture` | 0 | 2通过，0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 150通过（149单测+1集成），0忽略 |
| `cargo fmt --all -- --check` | 0 | 静态 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `bash scripts/run-pd-compat-readonly.sh --case deprecated-rpc --case classic-tso` | 0 | 2个真实双节点差分各1通过，0忽略 |

差分执行环境为`PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN="$PWD/bin/pd-server" PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server"`。监督入口实际执行`cargo test --locked -p astersql-pd-runtime --lib readonly_deprecated_rpc_differential -- --ignored --nocapture`与`cargo test --locked -p astersql-pd-runtime --lib readonly_classic_tso_differential -- --ignored --nocapture`，两条均退出0且各运行1项。显式执行的ignored测试算本次真实通过，工作区默认忽略的其他差分不算通过。

监督报告`compat-readonly-1793287432635360000.json`：exit=0；readonly_unchanged、inputs_unchanged、work_directory_cleaned和两个process_group_cleaned均true。原始报告及两份原始日志均在缓存之外即时归档，归档前再次核对全部inputs哈希一致。Go二进制SHA-256为`b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`，go1.26.8及固定kvproto模块；build info无VCS revision，不声称来自当前HEAD。

`cargo test --workspace --locked -- --test-threads=1`退出0：2025项通过（2023项单元/集成测试、2项compile-fail doctest），0失败、32忽略；忽略项不计通过。任务36的时钟/背压/fencing/重启测试也在本轮通过，其classic TSO差分已重跑并完整归档；没有修改任务36文件。

开始/结束1101个Go/模块文件（包含ignored/untracked）的路径、SHA-256和Git状态完全一致；plan.md逐字节不变。没有新增或修改Go文件，没有运行make、failpoint启停、Go测试、生成器或模块整理。真实etcd测试复用既有`etcd_fixture_test.rs`入口：只读构建既有fixture，`-mod=readonly`、GOWORK=off、GOPROXY=off、GOTOOLCHAIN=local，产物仅写已领取槽位；不生成Go源码。生产oracle仍为既有Go PD二进制。

最终确认槽位1无占用锁，无本任务槽位二进制/Go PD残留进程，缓存保留。Python AST、证据文档链接、`git diff --check`通过。其他会话的三个新增2026-10-29计划目录保持原样，不纳入提交。

完整命令、退出码、原始日志、首尾Go清单、源码哈希、监督报告及槽位清理记录见 [压缩证据](task-037-evidence.json.gz)，SHA-256：`d811dbf61ee7e4e82c3911431d303f592f4d6346d7a11b0e945b0eeab2a39ab0`。所有记录均为本次实际运行，未将历史日志或零测试构建当作行为通过。
