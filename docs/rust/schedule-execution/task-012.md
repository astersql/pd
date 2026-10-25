# 任务 012：无协议依赖的心跳操作命令

状态：已完成。依赖批次11提交 `b8d2b4759` 的生产 API、10组 Go fixture、5项聚焦测试、
27项包测试及1353项 workspace 测试证据已核实，见 [task-011](task-011.md)。
本任务串行实现，只接入 canonical `astersql-pd-schedule-hbstream`，两级 plan 和原主题只读。

## 生产边界

[operation.rs](../../../pkg/schedule/hbstream/operation.rs) 提供 `Operation` 七个独立可选
动作及 `HeartbeatCommand::for_region`，后者从真实 core RegionInfo 取得 leader、ID、
epoch。无 leader 返回 None；空动作、存在但为空的批次、false 的 ChangeSplit、多个
动作并存均原样保留。ChangeType/SplitPolicy 保留未知 i32；Peer/RegionMeta 复用 core。

TransferLeader 的单 peer 与候选 peers 独立；ChangePeerV2 和 SwitchWitnesses 保留
次序和缺省条目；split keys 保留空、重复与二进制内容。Merge 保留正常 canonical
region membership 的全部字段，包括 encryption/flashback。命令拥有 payload，clone
和输入变更不会共享可变数据。没有协议、operator、server、runtime 反向依赖，没有
网络/后台任务或存储路径；本任务的故障边界是无 leader 时抑制发送。
[领域说明](../../../pkg/schedule/hbstream/operation.rs.md) 记录 API 和转换边界。

## 实际 Go oracle

来源 commit `836fdc7cf3ef34f7eabc9d051418995bb97efabe`，相关 Go 文件相对来源没有
差异。[provenance](logs/task_012/provenance.json) 固定既有 cluster.test/dlv、来源源码和
[fixture](fixtures/task_012.json) 的 SHA256。
[freeze.py](logs/task_012/freeze.py) 在 TestSetOfflineStore 的现有安全 setup 完成处停止，
仅构造领域输入，调用真实 NewTestHeartbeatStreams/SendMsg，并对真实 msgCh 中的响应
调用 encoding/json.Marshal。调试器打印动态响应类型，分别确认 pdpb/schedulingpb。
[原始交互](logs/task_012/oracle.json)、[命令](logs/task_012/oracle.commands) 保存输入、
输出及进程退出0。未声称修改输入后的整个 Go 测试通过。

每种 mode 各17组，总34组：七类单动作、空动作、全部动作并存、全部存在但为空、
未知枚举、关闭auto split、缺省epoch、零epoch、零peer、候选含nil、无leader。
32组实际响应加2组无leader时队列长度0；每种mode均覆盖所有上述场景。
嵌套数据还覆盖u64最大ID/confver、PeerRole未知值、witness、二进制key/IV、
加密key ID、flashback字段、重复split key、批次nil条目与空key。

归一化只恢复 Go JSON 省略的默认标量，并将 nil/empty byte slice 转为相同空字节串。
不忽略业务字段、错误、未知枚举、消息存在性、批次次序或缺省消息条目。Go Merge
输入使用 core 可表达的真实非nil成员。没有从 Rust 反推期望结果。

沿用任务11已审计的 setup 链；实际 SendMsg/Drain/Close 无必需 failpoint 注入。
只读挂载既有二进制；不编译 Go、不写 Go/模块文件、不启停 failpoints。为保留调试器
分配的输入，在这个隔离进程关闭 GC，未替换生产函数或注入返回值。

## TDD 与故障验证

先注册独立 [task_012_test.rs](../../../pkg/schedule/hbstream/task_012_test.rs)，取得
[缺少实现编译失败](logs/task_012/red-compile.out)，退出101，此项不作为行为断言证据。
[first-pass](logs/task_012/first-pass.out) 记录测试读取 Go nil byte slice 的诊断失败，
修正仅为把 nil byte slice 读为空字节序列。

[mutation.py](logs/task_012/mutation.py) 逐一注入并恢复12种错误：丢弃七种动作、
丢弃epoch、错误leader store、错误cluster ID、强制关闭split、重排split keys。
每次修改先 cargo fmt --all；[结果](logs/task_012/mutation.json) 全部退出101，分别
1/1/1/2/1/1/1/1/1/2/1/2项真实测试因行为断言失败，无编译失败冒充行为证据。
finally 恢复源码并格式化，最终验证基于恢复后的实现。

2项有效测试：34组冻结Go案例（34通过、0失败）；独立验证完整i32枚举往返、
u64最大寻址、payload深复制、无leader抑制、零peer可发送、缺省epoch保留。
无I/O的值类型没有额外网络/持久化故障；未引入mock或伪造此类差分。

## 最终验证

[validation.py](logs/task_012/validation.py)、[结果](logs/task_012/validation.json) 保存
确切 argv、stdout/stderr、退出码与有效测试数。

| 命令 | 退出码 | 有效测试 |
| --- | ---: | ---: |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test -p astersql-pd-schedule-hbstream --locked --lib task_012 -- --nocapture` | 0 | 2通过，0失败/忽略；34组fixture |
| `cargo test -p astersql-pd-schedule-hbstream --locked` | 0 | 2通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --workspace --locked` | 0 | workspace编译 |
| `cargo test --workspace --locked` | 0 | 1355通过，0失败，1既有忽略 |

[slot-run.py](logs/task_012/slot-run.py) 每阶段原子mkdir从1–10动态领取，记录
plan/task/session/PID；所有Cargo及子进程继承绝对路径。各阶段实际槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
正常、失败与信号退出路径等待子进程后释放自己的锁，保留缓存。
[最终阶段](logs/task_012/validation-stage.out) 记录领取和结束。

[verify.py](logs/task_012/verify.py) 从原始 Go Marshal 输出重新解析并逐组核对fixture，
核对动态响应类型、依赖提交、二进制/源码哈希、失败断言、测试数、锁清理和只读边界。
[审计结果](logs/task_012/verification.json) 保存实际数量。
Cargo.lock仅新增本地包及既有依赖边，离线生成带来的pulldown-cmark版本漂移已恢复，
无远程依赖变化。Go/模块和全部plan.md的起止哈希一致，含未跟踪Go文件检查；
`git diff --check`通过。提交仅包含任务12实现、接线、测试和证据，保留初始任务10/11
编号文件删除。任务12完成后按要求删除其编号任务文件，不改写计划或主题。
