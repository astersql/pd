# 任务014：心跳流保活与停止

状态：已完成。依赖批次13提交 `e064eee5c` 的 canonical 路由接口、9组 Go
运行 fixture、3项聚焦测试和1358项 workspace 测试证据已核实，见
[task-013](task-013.md)。本任务串行执行，两级 plan 和原始主题均保持只读。

## 生产路径

[stream_lifecycle.rs](../../../pkg/schedule/hbstream/stream_lifecycle.rs) 实现
`HeartbeatStreams::start/stop/join`、`HeartbeatMessage` 和 `HeartbeatMetrics`；
[routing.rs](../../../pkg/schedule/hbstream/routing.rs) 复用真实路由器，新增可取消
的队列等待、`HeartbeatStream::keep_alive` 和完整领域传输 `MessageStream`。
[领域说明](../../../pkg/schedule/hbstream/stream_lifecycle.rs.md) 记录接口、错误、
指标清理及外部 I/O 边界。无私有 runtime、全局 registry 或 detached send task。

一分后保活，header-only 响应不制造 target peer。取消立即唤醒满队列生产者，
消费者停止取新命令；stop 重复安全，join 等待当前发送并释放 ticker/队列。
传输必须最终返回；等待正在执行的外部 I/O 与 Go Close 一致，不把强制 abort 当关闭。
宿主移除 store 时调用 metrics 删除 API；发送后重读 canonical store 防止重建已删指标。

## Go运行oracle与输入

源码基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`。
[provenance](logs/task_014/provenance.json) 固定来源、二进制与 fixture SHA256；
四个采用的 Go 文件逐字节匹配来源 commit。复用前任务审计过的原生 `cluster.test`
和 dlv，只读 Docker 挂载；在 `TestSetOfflineStore` 第300行暂停后调用生产
`NewTestHeartbeatStreams`、`SendMsg`、`BindStream`、`Close`。已有 mockhbstream
仅为外部传输 channel 边界，nil channel 触发真实一秒发送超时。接口 itab 输入
与构造方法沿用任务13，无替换函数、注入返回值、Go改写、编译或failpoint启停。

[freeze.py](logs/task_014/freeze.py) 保留全部固定输入：cluster ID123、region55、
peer1/store1；先无绑定发送，再绑定正常 channel 发送，再绑定 nil channel 发送。
随后 store1正常、store2超时、store99不存在、store3 Removed；使用已有 Recv 的
10ms真实等待反复推进进程，实际观察到一分钟 ticker 的响应和错误。
[原始交互](logs/task_014/oracle.json.gz)、[命令](logs/task_014/oracle.commands.gz)
和[stdout](logs/task_014/freeze.out.gz) 均无损gzip，实际进程退出0。
关闭生产后台消费者两次后观察状态。另建全新 no-run 实例，1024次真实 SendMsg
填满队列，重复 Close，再 SendMsg 返回且队列保持1024。原 Go 测试被隔离调试后
退出；不声称修改输入后的原始完整测试通过。

[fixture](fixtures/task_014.json) 保存5组连续观察：push skip、push ok、push err、
keepalive ok/err、closed twice；每组保留全部 Prometheus labels/值、绑定ID和
交付数，另保存完整 header-only JSON、饱和关闭与 descriptor。
只对 map 顺序排序，调试器地址和观察时刻不作领域比较；不忽略错误、指标或消息字段。
这是 classic 实际运行；协议独立逻辑共用，不冒充新增 nextgen wire 运行。
Go oracle 的运行结果仅覆盖上述输入；并发阻塞和暂停时钟精确边界另按 Go 源码设计
Rust 故障/时序测试，不冒充额外 Go 运行差分。没有本任务范围的持久化操作。

## 红绿测试和故障

[red-compile](logs/task_014/red-compile.out) 记录注册测试后的缺失 lifecycle API，
退出101；不以编译失败替代行为证据。
[mutation.py](logs/task_014/mutation.py) 对最终 canonical 实现逐一注入提前保活、
错误cluster ID、失败保活不删绑定、bury后重建指标、遗漏skip、取消错误返回成功、
join直接abort发送，共7种错误；每次先fmt，同一批实际测试均因行为断言退出101，
[结果](logs/task_014/mutation.json) 保留具体断言日志索引对应文件。
finally恢复源码；没有保留测试专用的生产开关。

[task_014_test.rs](../../../pkg/schedule/hbstream/task_014_test.rs) 有5项有效测试：

- `task_014_behavior_cases`：对比5组 Go 指标、响应、真实 descriptor/type；暂停时间
  59秒无保活、60秒保活；重复stop/join后再推进180秒无发送；删除/reset清理指标。
- `task_014_router_oracle_bindings`：4组路由/保活观察逐字段对照 Go 的绑定、交付和指标。
- `task_014_saturated_cancel_and_join`：真实1024槽队列、满绑定队列、等待入队生产者
  均在取消后返回错误；join等待正在阻塞的发送；取消后不排空积压；bury不重建指标。
- `task_014_drop_and_keepalive_failure`：保活故障删除绑定后不重试，Drop请求取消。
- `task_014_keepalive_join_and_metric_registry`：保活阻塞时join等待，丢弃join future
  后仍可重新join；bury不重建指标；重复注册错误与独立宿主registry隔离。

## 最终验证

[validation.py](logs/task_014/validation.py) 和 [结果](logs/task_014/validation.json)
保存确切命令、stdout/stderr、退出码及有效测试数。

| 命令 | 退出码 | 有效测试 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test -p astersql-pd-schedule-hbstream --locked --lib task_014 -- --nocapture` | 0 | 5通过，0失败/忽略；5组Go观察及饱和关闭 |
| `cargo test -p astersql-pd-schedule-hbstream --locked` | 0 | 10通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | metadata |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --workspace --locked` | 0 | workspace编译 |
| `cargo test --workspace --locked` | 0 | 1363通过，0失败，1既有忽略 |

[slot-run.py](logs/task_014/slot-run.py) 每阶段动态原子mkdir领取槽位，记录
plan/task/session/PID；全部 Cargo 命令和子进程继承绝对路径。
实际槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
正常、失败和信号路径均等待子进程结束后释放自己的锁；保留编译缓存。
[最终阶段输出](logs/task_014/validation-stage.out) 保存领取、路径和退出码。

[verify.py](logs/task_014/verify.py) 从原始 Go 交互重新解析5组观察与保活响应，
核验饱和队列、来源hash、依赖提交、变异断言、验证计数、只读文件和槽位释放。
[审计结果](logs/task_014/verification.json) 保存实际统计。
Go/模块与plan文件起止哈希一致（含未跟踪文件检查）；未产生任何Go差异。
Cargo.lock只增加既有prometheus/tokio-util的本地依赖边，无远程版本变化。
提交仅包含任务14实现、测试、接线和证据，排除已有任务10–13文件删除。
完成后删除本编号任务文件，不改写plan或来源主题。
