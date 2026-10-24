# 任务47：存储心跳协议适配

依赖 task-46 已验收，累计前置沿其证据；已读两份只读plan、根规则、Copilot、ADR、移植指南、依赖基线，无更深AGENTS。
起点commit、Go/模块逐文件哈希及Git状态、指定plan哈希见 [before.json](task47/before.json)。

## 实施前 oracle

基准69ba16e6d47dfe59a21498c21641e1c1eaa726e0；来源 server/grpc_service.go:812–825、927–1015，
server/cluster/cluster.go:1160起与 core_store_adapter.rs。领域运行复用任务41/42的原测试、CPU/query调试快照；
peer query=get+scan+coprocessor，GC不计；CPU unified_read乘interval秒，scheduler不计；字段在边界不归一化。
缺interval保留None，缺query/CPU保留None，空peer列表为空。

协议真实运行在实现前取得：[oracle.out](task47/oracle.out)。现有bin/pd-server为x86且--version无响应，
已发送TERM/KILL；该进程处于macOS内核U态（不参与槽位验证），另只读构建原有cmd/pd-server：
`GOOS=darwin GOARCH=arm64 go build -tags without_dashboard -o "$CARGO_TARGET_DIR/pd-oracle47" ./cmd/pd-server`，退出0。
审查main和入口依赖，不调用生成器/工具或注入；GOFLAGS=-mod=readonly、CGO_ENABLED=0、GOPROXY=off。
产物仅在槽位。proto/build.rs只读固定模块ZIP校验摘要、在OUT_DIR生成Rust；etcd夹具只构建现有Go工具，无注入。
[oracle.py](task47/oracle.py)启动隔离Go服务器，Rust tonic客户端发送真实protobuf请求并读取响应：
缺stats→gRPC Unknown（invalid store heartbeat command前缀）；Bootstrap注册store1；
未知store99→响应头UNKNOWN/store 99 not found；store1空stats→无错误/Preparing；
PutStore注册Tombstone/Removed store3后心跳→STORE_TOMBSTONE/store is tombstone。
领域Removed成功no-op与协议先行拒绝有意分层。服务器TERM退出0并wait，数据删除。
运行探针只用临时端口/数据，未写Go源码、模块、failpoint。协议其余调度/恢复字段不在任务范围。

## 红灯与实现

[red.out](task47/red.out)：1项运行、1项业务断言失败，已注册store1得到StoreNotFound，退出101。
[red-wire.out](task47/red-wire.out)：5项运行、5项失败，默认tonic StoreHeartbeat为Unimplemented，
包含真实HTTP/2调用；listener取消/join和HotStat.shutdown在结果断言之前完成。
不是缺符号编译错误或零测试。[首次绿灯](task47/green-fixed.out)5通过。
两个局部类型拼写/命名空间错误分别保留在green.out、expanded-compile.out，修正后重新完整验证。

StoreHeartbeatService为宿主显式注入、仅使用core StoreStats/NodeState/类型化错误；
PdServices默认返回None，已有宿主无需修改。apply_store_heartbeat只做协议前置membership检查并复用
canonical astersql-pd-cluster，不复制统计/热点算法，不持有运行时或新后台任务、不持久化迟到快照。
CPU/query/peer/interval用现有core_store_adapter无损转换，领域仍不依赖protobuf/tonic。
响应保留NodeState（包括未知整数）、UNKNOWN/STORE_TOMBSTONE/NOT_BOOTSTRAPPED。
缺stats返回Unknown且保留错误前缀；明确不复刻Go错误中的请求全文dump。
恢复/DR请求因控制器不在范围而返回Unimplemented且不更新stats；其余控制器输出保留协议默认值。
不声明完整RaftCluster、管理面或调度服务已迁移。

7项最终行为覆盖：缺stats/未知store/Removed/wrong cluster；已有store、原始raw报告和时间；
默认None及显式零值嵌套消息、peer列表、再次空报告重置；未注入与未bootstrap；
全部NodeState和未知整数、宿主错误；恢复/DR拒绝且raw未变；真实HTTP/2下两region各3次读报告。
原始protobuf统计逐项相等，真实HotStat两peer的去噪速率均[1000000,1000000,1000000,7]，
CPU scheduler=999不进入read CPU，GC=100不计read query；精确累积/小数速率沿用task41实测。
listener cancel+join、HotStat shutdown等待worker全部退出；无私有runtime。该状态适配不拥有存储，
无新增持久化语义；workspace另外执行真实MemoryStore/EtcdMetaStore契约，不将通用后端测试冒充本路由持久化。

## 最终验证与资源

入口 `bash docs/rust/state/task47/slot.sh python3 docs/rust/state/task47/validate.py`。
[summary.json](task47/summary.json)记录逐命令退出码及有效测试数；[validation.out](task47/validation.out)
为阶段摘要，validation-0.out至validation-6.out保留完整输出。所有命令退出0：

| 命令 | 结果 |
| --- | --- |
| `cargo fmt --all` | 成功 |
| `cargo test -p astersql-pd-server --locked --lib state_task_47_store_heartbeat_adapter -- --nocapture` | 7通过/0失败/0忽略 |
| `cargo test -p astersql-pd-server --locked` | 20通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 成功 |
| `cargo fmt --all -- --check` | 成功 |
| `cargo check --workspace --locked` | 成功 |
| `cargo test --workspace --locked` | 1274通过/0失败/1忽略 |

既有ignored为tso_processes，本任务无忽略。所有阶段动态领取槽位2，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
新依赖只增加workspace既有crate/prost引用；首次oracle `cargo run`维护锁文件，后续验证均--locked。
[resources.json](task47/resources.json)：自有槽位锁已释放、缓存保留，所有槽位产物子进程已结束；
实际native Go oracle服务退出0并wait、目录清理，真实HTTP/2 listener与HotStat workers cancel/join。
唯一系统限制是最初运行既有x86 `bin/pd-server --version`的PID39902处于内核U态，
TERM/KILL均未使其退出。该可选诊断不占Cargo槽位、未请求监听器或数据目录；
不把它冒称已回收，不影响已完成的native oracle与Rust验证。无需重跑本任务验收。
Go oracle新二进制SHA256：446a9ce18ed5ac513d4cc66306cfe276db44cd2198db2d9e4e53d20520040523。

[readonly.json](task47/readonly.json)：1101项Go/模块集合、逐项内容哈希和Git状态与起点相同；
聚合SHA256 ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4。
两份指定plan.md未改，未运行make/failpoint/Go格式化/模块整理。workspace fmt无范围外Rust差异。
`git diff --check`和证据链接检查通过；用户既有计划变更不纳入提交。

状态：已完成。范围仅为StoreHeartbeat状态协议适配。
