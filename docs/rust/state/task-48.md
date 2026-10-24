# 任务48：区域心跳协议适配

依赖 task-47 已完成，累计依赖沿其记录；已读两份只读plan、AGENTS、Copilot、ADR、依赖基线与移植指南。无更深AGENTS。
起点commit、1101项Go/模块逐项哈希及状态、两份plan哈希见 [before.json](task48/before.json)。

## 实施前运行oracle

Go基准69ba16e6d47dfe59a21498c21641e1c1eaa726e0；只读审查 server/grpc_service.go:1357–1427、
server/cluster/cluster_worker.go、pkg/schedule/hbstream/heartbeat_streams.go:284及core转换。
复用任务47真实native Go服务二进制，未重新构建/注入Go；proto/build.rs仅下载校验固定ZIP并在OUT_DIR生成Rust。
[oracle.py](task48/oracle.py)启动隔离服务器，Rust tonic probe发送真实双向流：[oracle.out](task48/oracle.out)。
输入为region1/peer2/store1，epoch2/2、term7、interval10–20秒、size2097152字节、query get11/put13、CPU unified_read17/scheduler999。
接受、重复均无成功回包（500ms超时），GetRegionByID确证epoch2/2及leader2；过期version1得到UNKNOWN流内错误，
region_id=0/epoch=None/target_peer=leader；invalid region0得到UNKNOWN；nil leader在membership阶段返回gRPC Unknown、
`invalid store ID 0, not found`。拒绝后的快照仍epoch2/2。Go服务TERM退出0并wait，数据删除。
query/CPU的领域运行语义复用task41/46及core adapter既有测试；此probe的GetRegion不暴露这两项，不冒称读取到其值。
错误message比较稳定前缀，Go文本protobuf请求dump及Rust领域Debug元数据格式不要求逐字一致；错误类型、目标peer、状态变更必须一致。
不接调度命令、hbstream跨流绑定或keepalive；成功不产生人为ack。

## 失败证据与实现

[red.out](task48/red.out)实际运行1项行为测试，tonic默认RegionHeartbeat返回Unimplemented，
业务断言失败、退出101；listener已先cancel/join。最初一次编译发现core无read_cpu_usage访问器，
修正测试为公开loads()[6]后才取得上述红灯，未将编译错误当失败验收。
[green.out](task48/green.out)同测试首次通过；[expanded.out](task48/expanded.out)4项通过。

新增RegionHeartbeatService与RegionHeartbeatError、PdServices默认可选入口；Grpc::region_heartbeat
复用region_from_heartbeat转换并调用宿主真实状态接受服务。流内错误保持UNKNOWN和target_peer，
not-bootstrap返回错误头并终止，wrong cluster为FailedPrecondition、未知store为Unknown，Removed忽略。
接受及重复不返回ack、不复制cluster算法。错误诊断仅省略Go请求dump，过期错误保留稳定前缀。
领域不依赖协议、transport或运行时；canonical cluster入口及其副作用仍由宿主装配。
本适配不拥有存储或持久化；不将通用Etcd测试冒称本路由新增持久化能力。

5项行为测试均真实tonic HTTP/2 loopback：epoch/confver/leader/term/interval/query/CPU、重复、
过期version/confver/term与最终缓存；None默认；nil leader/未知store、wrong cluster、无region/无peer/无down-peer；
Removed/空流正常关闭、缺服务、未bootstrap；在途调用取消与客户端断开。
CPU统一读取loads()[6]=17、scheduler999不进入load，GC999不计read_query，size由既有core归一化。
取消及断开均以活跃future计数归零、reports为空和listener join验证，没有脱离流的worker。
时钟、调度、转发、跨流绑定、keepalive及管理API不在本任务范围。

## 最终验证与退出

统一入口：`bash docs/rust/state/task48/slot.sh python3 docs/rust/state/task48/validate.py`。
[summary.json](task48/summary.json)记录确切命令、退出码与有效测试数量，
[validation.out](task48/validation.out)和validation-0.out至validation-6.out保留全部输出。

| 命令 | 退出码/结果 |
| --- | --- |
| `cargo fmt --all` | 0 |
| `cargo test -p astersql-pd-server --locked --lib state_task_48_region_heartbeat_adapter -- --nocapture` | 0；5通过/0失败/0忽略 |
| `cargo test -p astersql-pd-server --locked` | 0；25通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0 |
| `cargo test --workspace --locked` | 0；1279通过/0失败/1既有忽略 |

既有ignored为tso_processes，本任务无忽略；workspace覆盖真实MemoryStore/EtcdMetaStore。
Etcd夹具仅`go build`既有scripts/rust-etcd-fixture，不含生成/注入入口，继承CGO_ENABLED=0、
GOFLAGS=-mod=readonly、GOPROXY=off；本任务未运行make/failpoint/Go格式化或模块整理。
未改变Cargo依赖与锁文件；workspace fmt未产生范围外Rust改动。

所有阶段动态领取槽位2：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
[resources.json](task48/resources.json)：全部槽位产物子进程结束后释放自有锁、保留缓存；
Go oracle服务退出0并wait，隔离数据目录删除；Rust listener取消/join，
在途future在host取消及client断开后活跃数均归零。未创建私有runtime或额外worker。
复用Go二进制SHA256：446a9ce18ed5ac513d4cc66306cfe276db44cd2198db2d9e4e53d20520040523。
首次探针因Go延迟响应头而等待，已中断并回收进程；修正为先入队再调用RPC，最终oracle独立重跑退出0。

[readonly.json](task48/readonly.json)确认1101项Go/模块文件集合、逐项内容哈希和Git状态与起点相同；
聚合SHA256 ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4。
两份指定plan.md哈希相同。`git diff --check`与证据链接检查通过；用户既有计划变更排除提交。

状态：已完成。仅声明区域心跳状态协议适配完成。
