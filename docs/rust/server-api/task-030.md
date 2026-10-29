# 任务030：SplitRegions 请求

状态：已完成。基准 `f2c487f39`；批次29及此前准入已核验。协议仍固定 kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`，默认 features。

覆盖 [source-inventory.md](source-inventory.md) 的任务30行、grpc_service.go 方法清单，以及 [wire-inventory.md](wire-inventory.md) 的 classic SplitRegions 行。surface-inventory 的 HTTP regions/split、独立 schedulingpb 服务、转发指标分别属于67/78/90–92，本次不将其计为完成。

只读 Go oracle：`server/grpc_service.go:2298–2354`；`pkg/schedule/splitter/region_splitter.go:76–143`（inclusive retry、最终退避、100ms观察、60s超时）、分组与边界扫描。Go 返回的是观察到的新 start key 的 region ID；operator admission 不能算成功。空输入返回0；uint64 retry_limit 转换为64位 Go int 后为负值时没有尝试；取消只结束每次观察，外层仍重试并退避。返回百分比为 `100 - remaining * 100 / original`，保留部分结果。

[RPC适配](../../../pkg/server/grpc_split_regions.rs) 经 Grpc、SyncGrpc、RoleServices/Application 进入 [term worker](../../../pkg/runtime/schedule_split.rs)，复用 canonical SplitRetry、replication/rules、ControllerAdmission、operator records、heartbeat routing 和原有调度队列。pending request 不占用等待心跳所需的资源锁。请求丢弃取消观察；任期/服务关闭回收所有 pending，无独立 runtime 或 detached worker。prepare gate 期间只有活动 split 才缩短轮询周期。领域 retry_limit 从i32扩大为i64以保留 wire 转换，没有加入 Go 没有的 retry clamp。新增依赖仅为 runtime 对已有 workspace splitter crate 的引用。

[真实TCP测试](../../../pkg/server/grpc_split_regions_test.rs) 与 [共享场景](../../../pkg/server/grpc_split_regions_scenarios_test.rs) 使用真实etcd、tonic和同端口HTTP ping。验证未bootstrap、错误cluster ID、空输入、未充分复制/已存在边界、负重试值、真实USEKEY命令、目标peer、GetOperator RUNNING、边界心跳、100%及60秒后50%响应和确切ID。查询与心跳在pending期间仍可进行。核对新region的持久metadata；客户端deadline后没有新边界，服务取消打断仍在运行的请求并完成shutdown。

[可控时钟测试](../../../pkg/runtime/grpc_split_regions_clock_test.rs) 使用真实宿主、同一MemoryStore事务、真实心跳pipeline及调度controller，精确推进100ms观察、取消后的50%/ID保留、60秒超时无进展、已取消请求的inclusive retry和最终退避。使用调度tick publication加operator查询同步actor；时间按1秒推进，使真实领导租约renew任务也有运行机会。测试channel仅承载领域心跳命令，60秒keepalive之后重新绑定；不是用调用次数代替结果断言。

SplitRegions自身没有新增持久事务：admission/observation都是既有内存调度状态，持久region来自已有heartbeat pipeline；本次不制造虚假的split持久队列或allocator故障。持久写入故障的pipeline回归由包级/workspace测试覆盖。

[真实Go差分](../../../pkg/runtime/grpc_split_regions_differential_test.rs) 复用任务4只读supervisor和已有Go二进制，运行同一请求与心跳场景，比较错误header/transport、命令内容、完成率、ID和观察到的metadata；还比较真实follower的NOT_BOOTSTRAPPED。随机cluster ID在断言实际值/错误文本后归零；region IDs排序仅规范Go map迭代。Go二进制没有VCS revision，不声称来自当前HEAD，哈希/build info随原始报告记录。

初始测试编译成功，SplitRegions返回Unimplemented，行为断言失败（101）；接线过程中还发现SyncGrpc和RoleServices必须转发该方法，未将默认trait错误算成功。可控时钟测试先发现取消发生在actor完成scan之前、一次跃进59秒会使真实租约到期，以及测试channel在keepalive后需重新绑定；修正同步/时钟推进/测试传输后重跑。一次差分在测试源仍需修正时由supervisor SIGTERM结束（130），不计通过，进程组和临时目录由supervisor清理。

workspace初次检查发现既有handler传入i32，已将SplitRetry构造参数改为Into<i64>，保留源码兼容。首次真实Go差分还发现原测试范围[a,z)跨过Go默认txn keyspace的x前缀label边界，后台checker发出另一组split keys；没有放宽命令断言，而是将共同场景范围收窄到[a,w)，避开该自动边界后重跑。客户端deadline的实际Cancelled/Timeout expired也逐值比较，没有仅比较一个“取消”汇总标签。

## 验证

所有命令从仓库根执行，默认features。动态原子mkdir领取槽位1、2，owner记录计划、任务、会话和PID；CARGO_TARGET_DIR分别为 `/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`、`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。最终聚焦、包级、差分和workspace验收均使用槽位1；槽位2用于早期时钟编译、一次格式化和被主动停止的差分。

| 确切命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初次 `cargo test --locked -p astersql-pd-server --lib task_030_ -- --nocapture` | 101 | 1个预期行为断言失败，非编译错误 |
| 最终同一聚焦命令 | 0 | 1通过，0忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_030_ -- --nocapture` | 0 | 1通过，0忽略 |
| `cargo test --locked -p astersql-pd-schedule-splitter -- --nocapture` | 0 | 13通过，0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 136通过，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all`；`cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN="$PWD/bin/pd-server" PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server" bash scripts/run-pd-compat-readonly.sh --case split-regions` | 0 | 1真实Go差分通过，0忽略，含follower |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 2000通过，25既有/环境项忽略且不计通过 |

最终只读监督报告 `compat-readonly-1793271611871539000.json`：exit=0，readonly_unchanged、inputs_unchanged、work_directory_cleaned、process_group_cleaned均为true。两处自有槽位锁全部释放，缓存保留；未遗留fixture/PD进程。开始/结束含ignored/untracked与target在内的1983个Go/模块文件集合、逐文件SHA-256和受保护Git状态完全一致。plan.md与基准HEAD逐字节一致。Python AST、入口bash语法、证据相对链接及git diff --check通过。原有任务1–29删除保持未暂存；任务30完成后按要求删除，不纳入实现提交。

[压缩原始证据](task-030-evidence.json.gz) 含基准、源码哈希、全部失败/通过日志、精确命令、退出码、测试摘要、Go首尾守卫、三个supervisor报告及其原始差分日志、二进制provenance和槽位执行器。SHA-256：`e468b0a4d47fcbd6743ec44e54555baf032a863af3e6cd308b9008cfe99eaad3`。
