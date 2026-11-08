# 生产任务 40 的本地嵌入进程回执

下文为此前测试夹具的历史回执。服务端验收 18 已另行记录正式二进制的显式
本地原生入口，生产任务仍按原计划验收。
[本地拓扑交接](../server-api/acceptance/native-topology.md)。

基准 `7b60522a95454ca8e90a9cde2bba0ad3ab87a517`。新登记的
`theme10_native_single_binary_embedded_wal` 启动同一个 Rust 测试可执行文件的
两个先后子进程。各子进程作为宿主，调用现有 `PdNodeBuilder`，在调用方 Tokio
执行器上注入现有单 voter OpenRaft/SQLite 存储和调用方 Registry。子进程环境
清空，PATH 不含可执行文件。两次提供服务时均检查进程树，宿主无子进程。
[本地嵌入测试](../../../cmd/pd-server/native_single_binary_test.rs)。

第一进程通过真实 TCP 执行 HTTP health、GetMembers、AllocID、Tso。确认
`metadata.sqlite-wal` 有内容后，由父进程发送 SIGKILL，退出信号为 9，
监听随进程终止。第二进程复用同一地址和磁盘目录，直接打开原生存储并重新获取
服务任期。复跑读回同一 cluster ID `1794143975318268000`，ID 从 1 到 1001，
TSO 从 `(1794143975538, 1)` 到 `(1794143980734, 1)`。两次均在服务期间用
SIGUSR1 触发宿主处理器并看到宿主 subscriber 日志。第二次发送 SIGTERM，
进程正常退出，宿主显式回收并关闭后端，监听可立即重绑。

| 命令 | 结果 |
| --- | --- |
| `cargo test --locked -p astersql-pd-cmd-pd-server --lib theme10_native_single_binary -- --nocapture` | 首轮 101，测试误用了候选存储的 WAL 文件名。改为原生文件名后两轮均退出 0，每轮 1 通过、0 失败、0 忽略、21 过滤 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 |
| `cargo test --locked -p astersql-pd-cmd-pd-server --lib task_097_ -- --nocapture` | 0，外部 etcd 过渡路径 2 通过、0 失败、1 默认忽略 |
| `cargo fmt --all -- --check` | 0 |
| `env -i PATH=/nonexistent "$CARGO_TARGET_DIR/debug/pd-server"` | 1，stderr 为 `pd-server: --etcd-endpoints is required for the transitional backend` |
| 同一命令加 `--etcd-endpoints=http://127.0.0.1:1 --data-dir="$work/rejected-native"` | 1，startup/runtime failed，原生目录未创建 |

各阶段均动态领取槽位 1，`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`，
待被测进程和夹具退出后已释放槽位。Go 只读保护按任务 18 首尾快照执行。

本回执覆盖测试二进制内的嵌入宿主。独立 `pd-server` 的原生后端选择须由
生产任务 28 接入，生产 40 仍须在同一正式二进制上复验独立和嵌入拓扑。
认证共识传输、受控成员管理以及服务/后端切换是原拥有任务的交付条件。
[生产准入 ADR](adr.md)。

后续已让 `pd-server` 的二进制测试目标复用正式配置解析和装配，通过测试
注入重开同一原生 WAL。此证据与嵌入宿主互补，正式发行进程的后端选择按
生产 28 的原有门槛验收。
[独立测试目标回执](task-28.md)。
