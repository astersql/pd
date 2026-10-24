# 任务36：下游独立发送与背压

起点 `79598a83b`；依赖[任务35](task-35.md)验收通过。读取两个只读计划、
根规则、Copilot、ADR、移植指南及依赖基线；没有更深 AGENTS。
Go 基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，server.go/server_test.go 无漂移。

## 实施前 Go oracle

[只读入口](task36/oracle.sh)、[运行输出](task36/oracle.out)：5 个既有测试通过，退出0。
显式编译既有文件排除 client_test.go 的 failpoint TestMain；所选测试不启用 failpoint，
不运行 RunServer 或取消全量迭代；发送失败使用既有 testServerStream.sendErr。
InjectCall 未启用，因此测试的错误来自真实 Send 返回路径，无需注入改写。
GOFLAGS=-mod=readonly、CGO_ENABLED=0、GOPROXY=off，既有 Go 测试编译到槽位，
Linux 二进制由 Alpine 运行。不新增 Go 工具，不调用生成/格式化/注入命令。

以下为已执行测试的断言契约，而非程序额外输出的字段快照：
- IndependentDownstreamIndexes：reset10，ID1/2；pd2=10、pd3=11，发送
  (10,[1,2])、(11,[2])，二者结束12，历史不变。
- RemovesOnlyFailedDownstream：reset10，ID1；pd2 Send错误，仅移除pd2，
  它的索引仍10，pd3收到(10,[1])并推进11。
- AppendHistoryRecordsDoesNotWaitForBusyDownstream：sendMu锁住，已有ID1/2，
  追加ID3/4/5完成，历史5条，忙流索引10。
- DrainDownstreamSendsPendingRecordsSerially：先(10,[1,2])推进12，追加ID3并通知，
  后(12,[3])推进13。
- KeepAliveDoesNotPrecedePendingRecords：先请求keepalive但有待发记录，只有
  (10,[1,2])；下次空闲请求才发(12,[])。

源码补充：每流单独notify通道和发送任务，history读取在发送前完成；最多1000条/帧。
索引仅在成功发送后推进，overflow/future-index返回错误。取消与重绑按流身份隔离。
领域使用HistoryIndex与完整Arc<RegionInfo>，不使用MetaStore revision或协议类型。

## TDD 与实现

[红灯](task36/red.out)：最小可编译drain实际执行5项，退出101，0通过/5失败。
具体断言：实际空frames而期望(10,[1,2])，失败流仍留在[pd2,pd3]而非[pd3]，
阻塞发送器从未进入Send。没有以编译错误或零测试代替红灯。

`downstream.rs` / `downstream_test.rs` / 相邻行为说明与lib显式登记：
- `Downstreams` 管理真实HistoryBuffer、短锁和各流索引；append不等待Send，
  先观察最慢索引窗口、合并Notify唤醒，记录不在流内重复排队。
- `DownstreamSender` 是可取消future的I/O边界；每流一个宿主runtime任务独占发送器。
  drain每批至多1000条，锁外发送，只有成功后推进本流索引；发送后再检查history。
- `DownstreamTask` 提供cancel与join，重绑取消旧任务，Drop按Arc身份清理注册。
  失败不误删其他或新绑定；取消中断阻塞Send且不确认未完成帧。
- 新增SyncFrame crate内部构造器，复用完整Arc记录；Tokio从已有dev依赖提升为
  workspace生产依赖，Cargo.lock/root manifest不变，无新增feature或外部依赖版本。

最终10项行为覆盖5个已运行Go契约，并追加1000条分帧、Send期间追加、
idle keepalive期间无通知追加、第二帧失败保留首帧索引、历史覆盖/未来索引错误、
替换流及显式shutdown取消阻塞I/O、join后active Send计数为0与注册表清空。
没有mock历史、区域字段或索引结果；所有成功路径检查最终索引和发送内容。
本任务无新MetaStore边界；包级持久化回归使用MemoryStore与真实EtcdMetaStore。

## 最终验证

所有构建阶段由[动态槽位入口](task36/slot.sh)原子领取，实际oracle、红灯、初次绿灯和
最终验证均为槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[验证入口](task36/validate.py)、[总日志](task36/validation.out)、[命令/退出码/计数](task36/summary.json)。
依次执行cargo fmt --all、指定聚焦测试、包级回归、metadata、fmt check、workspace check、
workspace test。最终全部退出0：聚焦10通过/0忽略，包级53通过/0忽略，workspace
1192通过/0失败/1既有ignored（tso_processes要求独立进程二进制，与任务35基线相同）。
metadata/fmt/check均退出0。聚焦、回归与字段快照均与上述Go oracle断言一致；
Rust额外故障测试保持各自错误与最后成功索引，不扩大Go运行证据的涵义。

Go构建子进程继承readonly/off设置；已审查proto build.rs仅写OUT_DIR、
etcd夹具仅写槽位二进制与数据目录，无Go生成、模块整理或failpoint注入。
[退出证据](task36/resources.out)：进程等待完成，自己的slot-1.lock已释放，编译缓存保留。
[只读核对](task36/readonly.out)：1101个Go/模块文件清单、SHA256、Git状态前后相同，
29份plan.md哈希相同，Markdown链接与git diff检查通过。未改动其他任务已有变更。

状态：已完成。
