# 任务029：ScatterRegion 与 GetOperator

状态：已完成。基准 `15d9be2b3`，任务28的完成证据及任务1–4准入已核验；协议固定 kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。默认 features，无依赖或 Go 修改。

## 来源与行为

覆盖 [source-inventory.md](source-inventory.md) 的任务29行及 grpc_service.go 方法清单、[wire-inventory.md](wire-inventory.md) 的 ScatterRegion/GetOperator 两行。surface-inventory 的 operator HTTP API、转发指标和配置分别由62/63、90/91/92、47–50负责，不把这些表面计入本任务验收。

只读 Go oracle：`server/grpc_service.go:1975–2083,2105–2175,2395–2415`；`pkg/schedule/scatter/region_scatterer.go:499–672`；`pkg/schedule/operator/operator_controller.go:791–799,987–1000` 与 `status.go:76–99`。

批量 regions_id 优先于弃用的 region_id；单 region 支持请求 metadata/leader 回退。保留未 bootstrap、缺失 operator 的 REGION_NOT_FOUND/Not Found、单缺失 region 的原始 `region %d not found` header、批量缺失/空集的 UNKNOWN transport 错误，以及真实失败 region ID。完成率使用 Go 的 processed/failures 公式，包含其 admission 失败计数语义。GetOperator 从同一 controller 读取 active/retained operator 的 desc、kind 与真实状态，映射 CREATED→5、STARTED→RUNNING、SUCCESS→SUCCESS、CANCELED→CANCEL、REPLACED→REPLACE、EXPIRED/TIMEOUT→TIMEOUT。

[grpc_scatter.rs](../../../pkg/server/grpc_scatter.rs) 负责协议适配；[schedule_scatter.rs](../../../pkg/runtime/schedule_scatter.rs) 通过有界请求通道借用现有 term-owned Coordinator。复用 canonical scatter、admission、records、规则、热区快照、心跳 routing 和调度 worker 的 retry/influence queues，不创建第二份调度状态或私有 runtime。RPC 取消关闭 reply channel；已完成的部分 admission 仍进入执行队列，替换记录被归档，同一 operator identity 不重复排队。主循环优先处理到期 tick，避免连续查询饿死调度。

同步 builder callback 需要 ID 时，从同一个任期围栏 allocator 预留上界 block；不用的 ID 可以留空隙，符合现有 allocator 的 reservation 模型。预留不是 Go 的逐 peer 精确 ID 轨迹，差分不声称分配序列逐值相同。scatter 没有另建持久 operator 队列；持久副作用为已有 region metadata 与必要的 ID 边界。任期结束丢弃内存 operator，与既有调度 owner 一致。

修复必要依赖：关闭 placement rules 时 runtime 原先跳过 RuleManager 初始化，但 canonical scatter 仍需要默认 voter rule，因此真实成功场景报 NoTargetLeader。现在始终初始化同一个 RuleManager；没有在适配层复制规则。Go/Rust 的广泛配置默认值等价性仍由47–50负责；fallback 错误案例用单 voter metadata 固定共同的未充分复制前提，不把不同 placement 配置下的错误推导当兼容通过。

## 当前行为证据

[独立真实服务测试](../../../pkg/server/grpc_scatter_test.rs) 与 [共享场景](../../../pkg/server/grpc_scatter_scenarios_test.rs) 使用真实 TCP/tonic、etcd、同监听器 HTTP ping；验证未 bootstrap、缺失/失败 region、fallback、同 group 重入、不同 group 冲突、混合批次75%/failed=[999]。三个 store、三个完全复制 region 初始 leader 均在 store1，scatter 产生两个 admin,leader operator；真实 heartbeat 收到转移命令并报告新 leader，GetOperator 从 RUNNING 变为保留的 SUCCESS。逐个核验 region ID、desc、kind、status、命令目标与完成状态；最后才按 Go 随机选择的 region ID 排序汇总两个相同类型，不能用汇总掩盖未执行 operator。测试还核对真实持久 region metadata 和失败 region 未被创建。

[事务故障测试](../../../pkg/runtime/grpc_scatter_fault_test.rs) 复用任务26的真实 IO gate。耗尽当前 allocator window 后，在真实 etcd 事务追加超大写入，响应 UNKNOWN/`unexpected metadata backend response`；持久 ID 边界不变、没有 operator 或超大写入残留。阻塞下一次实际事务后取消服务，请求失败、shutdown/join 完成且边界仍不变。另在服务测试中断言重试请求仍在运行，再取消服务，排除请求早已完成的伪取消证据。

[Go 差分](../../../pkg/runtime/grpc_scatter_differential_test.rs) 经任务4只读 supervisor 运行同一场景，比较未 bootstrap、错误原始 protobuf/文本、真实成功/失败与心跳完成；还比较真实 follower 的两个 RPC 返回 NOT_BOOTSTRAPPED。随机 cluster ID 在断言真实 ID 后归零；随机选择哪些 region 转移只按已逐个验收的同类型 operator 汇总。未忽略错误文本、失败 ID 或状态变化。既有 Go binary 为 go1.26.8 构建且无 VCS revision，不声称它来自当前 HEAD；哈希及 build info 在压缩证据。

初始测试编译成功，真实 RPC 返回 Unimplemented，行为断言失败（退出101）。扩展成功场景发现并修复缺默认 voter rule 的实际接线缺口。首次 Go 流测试等待响应头发生等待，由 supervisor SIGTERM 停止（130），进程组/临时目录已清理；改为先送心跳再等响应头。第一次错误差分还发现未注册 stores 的三 voter fallback 在不同 placement 配置下前提不一致，改用单 voter 固定共同失败前提；保留日志，不删掉错误比较。

## 验证记录

动态按1–10原子 mkdir 领取槽位，实际槽位1和2；绝对 CARGO_TARGET_DIR 分别为 `/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`、`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。每阶段记录计划/任务/会话/PID，所有 Cargo/脚本/子进程继承；结束清理自有锁、保留缓存。精确命令、退出码、有效测试数及最终 Go 守卫在验收完成后补记。


| 确切命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初次 `cargo test --locked -p astersql-pd-server --lib task_029_ -- --nocapture` | 101 | 1个行为断言失败，非编译错误 |
| 最终同一聚焦命令 | 0 | 1通过，0忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_029_ -- --nocapture` | 0 | 1通过，0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 135通过，0忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 36通过，23既有环境项忽略，不计通过 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all`；`cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN="$PWD/bin/pd-server" PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server" bash scripts/run-pd-compat-readonly.sh --case scatter` | 0 | 1真实Go差分通过，0忽略，含follower |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 1998通过，24忽略，不计通过 |

聚焦、包级、构建与差分使用槽位1；最终 metadata/fmt/check/workspace 回归使用槽位2。所有验证阶段已结束，两个自有锁均释放，缓存保留；无遗留 fixture/PD 进程。最终 supervisor 报告 `compat-readonly-1793270075941227000.json` 的 exit=0、readonly_unchanged、inputs_unchanged、work_directory_cleaned 和 process_group_cleaned 均通过。

Go 守卫：开始先冻结源码1101文件；扩展到含 ignored/untracked 与 target 的1983文件，其完整快照与任务28结束的1983文件快照完全相同。结束时完整集合、逐文件 SHA-256 和受保护 Git 状态完全一致，未新增或还原任何 Go/模块文件。plan.md 与 HEAD 逐字节一致。证据链接、Python AST、git diff --check 通过。保留原有任务1–28删除，不暂存其他任务文件。

[压缩原始证据](task-029-evidence.json.gz) 包含基准、源码哈希、失败/通过/包级/workspace日志、初始与最终Go守卫、四次只读 supervisor 报告（130取消、101前提不一致、两次0通过）、原始差分日志与槽位脚本。SHA-256：`1130d1708350f6e5e14120b0ce51c9c67e2c3acecfda5ae2e502a271b5d3403b`。
