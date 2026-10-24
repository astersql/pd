# 任务33：增量历史批次

依赖 task-32.md 已完成；实现起点 `0a51556ca2bfd4f7c796e770e17550e0ec73809d`。
只读 Go 基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`；server.go/server_test.go
相对该基准无差异。已审查根规则、两份只读计划、ADR/移植指南、依赖基线，目标无更深 AGENTS。

## Go oracle（实施前运行）

入口 [oracle.sh](task33/oracle.sh)，输出 [oracle.out](task33/oracle.out)，退出0，3 passed。
显式编译既有 server/client/history/metrics 及 server/history 测试文件，排除 client_test.go
的 TestMain 与 failpoint 依赖测试；GOFLAGS=-mod=readonly、CGO_ENABLED=0、GOPROXY=off。
选定路径不运行 RunServer/client streaming；full-sync 迭代未取消，不需要 noFastExitSync。
未执行注入、生成器、make、新增或修改 Go 文件。Linux 测试二进制通过 Docker Alpine 运行。

既有测试通过断言（不是额外打印的运行快照）：

- TestSyncHistoryRecordsSplitBatches：起点10，ID0..1000，共1001条；帧(10,1000)、(1010,1)。
  第一帧后关闭发送流则第二帧返回 Unavailable，实际仅发送一帧。
- TestSyncHistoryRegionStopsAtSyncStartIndex：reset10，写ID1，捕获end11，再写ID2；
  请求10只发送ID1，帧start10。
- TestSyncFallsBackToFullSyncWhenHistoryMissing：reset100，请求1，进入全量，
  阻塞期间写ID2；全量帧(start0,ID1)、追赶(start100,ID2)、完成(start101,empty)。
  本任务只承担进入全量的决策；全量发送/追赶由后续任务实现，不冒充已完成。

源码字段契约：半开区间[start,end)，history index不是MetaStore revision；start0或start>end
需全量；start=end发送空帧确认；缺失或不完整区间需全量；非零落后请求先观察窗口。
每帧包含同序RegionInfo，保留meta/stat/leader/buckets，由server适配层转换协议字段。
空keepalive使用已确认发送索引，不能使用history.next而越过尚未发送记录。
这些附加边界为源码推导与Rust验证，区别于上述Go运行断言。

## 测试驱动与实现

先登记最小可编译接口，执行 `cargo fmt --all` 后运行聚焦命令，见[red.out](task33/red.out)：
退出101，1 passed / 5 failed / 0 ignored；真实断言为需要Incremental却返回FullSync。
实现后相同6项测试通过，见[validation-1.out](task33/validation-1.out)。

新增 `history_replay.rs`，导出 `HistoryReplay`、`SyncFrame`、`MAX_SYNC_REGION_BATCH_SIZE`；
现有 `HistoryBuffer::plan_replay` 复用真实记录、观察窗口与区间查询，没有平行history。
批次非零且封顶1000，完整性检查后构建独立Arc快照；无新依赖、feature或Cargo.lock变更。
本次测试复现Go的1001分帧、捕获end之后追加以及缺失窗口决策；另外验证自定义批次、
部分区间缺失、零/未来起点、相等起点确认、keepalive、覆盖后快照、u64末端与窗口增长。
使用Arc指针同一性确认保留完整RegionInfo，不重建并丢失统计/leader/buckets。

本模块是同步规划，无I/O和后台任务，不需要自身cancel/join或跨后端测试。
发送失败不在规划期间推进任何下游状态；Go关闭发送流的运行断言作为后续发送接线约束保留，
不声称已实现Rust发送循环。包级回归包括既有MemoryStore/EtcdMetaStore持久化与故障测试。
复核proto/build.rs仅在OUT_DIR解压校验归档并生成Rust；etcd夹具仅go build已有源码，
使用readonly/off/CGO=0环境，Drop关闭stdin并wait，超时kill再wait，正常/失败路径释放资源。

## 最终验证与共享资源

所有Cargo阶段使用[slot.sh](task33/slot.sh)逐个原子尝试动态领取槽位，实际均为1；
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁内记录计划/任务/会话/PID；包装器在正常、失败和中断路径等待子进程后释放自己的锁，保留缓存。
[资源核验](task33/resources.out)确认锁释放、缓存保留以及无槽位二进制进程残留。

正式入口 `bash docs/rust/state/task33/slot.sh python3 docs/rust/state/task33/validate.py`。
完整命令、退出码与测试数见[validation.out](task33/validation.out)及[summary.json](task33/summary.json)。

| 命令 | 退出码 | 有效测试数 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 非测试 |
| `cargo test -p astersql-pd-syncer --locked --lib state_task_33_history_replay -- --nocapture` | 0 | 6 passed / 0 failed / 0 ignored |
| `cargo test -p astersql-pd-syncer --locked` | 0 | 33 passed / 0 failed / 0 ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 | 非测试 |
| `cargo fmt --all -- --check` | 0 | 非测试 |
| `cargo check --workspace --locked` | 0 | 非测试 |
| `cargo test --workspace --locked` | 0 | 1172 passed / 0 failed / 1 ignored |

既有ignored为tso_processes；check仅有既有cpu_usage弃用警告。
[只读核验](task33/readonly.out)确认1101个Go/模块文件集合、SHA256及Git状态均与
[起点](task33/go-before.json)一致；aggregate SHA256=
`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
所有记录的plan.md与[起点清单](task33/plans-before.json)一致；证据链接及git diff --check通过。

状态：已完成。只验收增量历史规划与帧，不代表全量发送、追赶或整个同步主题完成。
