# 生产任务 28 的独立测试目标回执

下文为此前测试夹具的历史回执。服务端验收 18 已另行记录正式二进制的显式
本地原生入口，生产任务仍按原计划验收。
[本地拓扑交接](../server-api/acceptance/native-topology.md)。

`cargo test --bin pd-server` 的 Unix 测试目标编译正式配置解析和经典进程
装配。测试子进程显式注入现有隔离 SQLite/OpenRaft 后端。正式入口的连接器
继续使用外部 etcd，测试夹具仅由 `cfg(test)` 模块注册。抽出的公共装配
保留原参数校验顺序、监听、身份初始化、就绪与关闭流程。
[正式装配](../../../cmd/pd-server/classic_process.rs)、
[二进制测试登记](../../../cmd/pd-server/bin_main.rs)、
[独立测试](../../../cmd/pd-server/native_standalone_test.rs)。

子进程清空环境、使用不存在的 PATH，服务时采样无子进程。通过真实
GetMembers、AllocID、Tso 和 HTTP health 观察服务。SIGKILL 后用同一
地址和 WAL 重开，PD 身份保留，ID 从 1 到 1001，TSO 增长。再发送
SIGTERM，正式信号处理完成，测试宿主关闭后端，端口立即可重绑。

| 命令 | 结果 |
| --- | --- |
| `cargo test --locked -p astersql-pd-cmd-pd-server --bin pd-server theme10_native_cli -- --nocapture` | 首轮 101，直接构造默认结构缺少正式解析设置的副本默认值。改用正式配置解析后退出 0，1 通过、0 失败、0 忽略、1 过滤 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 |
| `cargo test --locked -p astersql-pd-cmd-pd-server -- --test-threads=1 --nocapture` | 0，合计 23 通过、0 失败、3 默认忽略 |
| 发行二进制带测试环境变量启动且不提供 etcd 端点 | 1，仍输出外部端点必需提示，测试目录未创建 |

相邻并行回归曾暴露重复编译的 etcd 测试夹具生成同名目录，以及动态端口
争用。命令包内复用一份夹具实例定义。既有独立生命周期测试也改由正式
参数解析取得默认值，保留原有生命周期事件断言。

槽位 1，`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
正式独立原生选择继续受生产准入约束。测试目标提供真实进程和共享装配证据。
