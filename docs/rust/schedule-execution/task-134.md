# 任务134：操作心跳协议转换

依赖133已核实：提交 `b63a04f20`、[task-133](task-133.md) 的81案例与2项测试。
直接生产依赖为已验证 hbstream 的 `HeartbeatCommand`/`Operation`，以及既有
server core peer/region 转换。生产路径为
[schedule_wire.rs](../../../pkg/server/schedule_wire.rs)，公开 `to_pdpb`、
`to_scheduling`、`error_pdpb`、`keepalive_pdpb`、`keepalive_scheduling` 和
`WireError`。详细边界见[模块说明](../../../pkg/server/schedule_wire.rs.md)。
本任务只完成转换；网络流装配、闭环和性能仍由后续任务验收。

采用用户后续明确授权的**只读Go源码/测试推导 + Rust真实行为与故障验证**。
没有新增或运行Go oracle，没有宣称新的Go/Rust运行差分。准入 oracle-index
没有覆盖本故事全部wire字段，因此没有把已有测试名字/输出当作此处运行证据。
[provenance](logs/task_134/provenance.json)记录来源HEAD、依赖提交、Go领域源码、
固定kvproto协议/生成Go marshaler、准入索引及fixture的SHA-256。
协议commit为 `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

推导依据：`heartbeat_streams.go` 的 `SendMsg` 逐字段复制七种独立动作，
`run` 构造两种header-only keepalive，`SendErr` 始终构造PD错误响应；
pdpb/schedulingpb响应的字段1及7–11相同，但2–6不同，不能跨类型原样转码。
这些消息没有oneof，present-empty动作与缺失动作不同。Go生成的repeated
message marshaler会解引用nil，Rust对其返回明确字段/索引错误而不抛panic、
不丢弃元素、不将nil悄悄替换为默认消息。此错误处理属于适配边界，不改变调度决策。

[固定fixture](fixtures/task_134.json)由[独立冻结脚本](logs/task_134/freeze.py)
按上述proto字段号生成，两者均标为源码推导，不调用待测Rust实现或Go工具。
复用task012固定输入但重新冻结wire期望，不复用其输出作为本次运行oracle。
184案例包含165个可编码输入、17个nil列表错误和2个无leader抑制输入。
其中128案例穷举七动作presence组合，6案例保留原输入中的非默认批次/并发动作
并去掉不可编码nil，7案例覆盖负数/未知/极值枚举及重复二进制split keys。
每个可编码输入分别比较PD和scheduling的完整字节（330次比较），再真实prost解码
比较完整消息；检查命令没有修改。epoch/merge完整metadata通过既有canonical转换。
无时间、随机字段，无归一化，不忽略错误或字段差异。

显式注册2项真实测试：`task_134_behavior_cases`执行上述184案例；
`task_134_headers_errors_and_decode_faults`覆盖54种cluster/error/target组合、
6种keepalive、6次畸形消息解码拒绝，以及错误头和present-zero头的固定字节断言。
错误值包含未知和负数，message包含NUL/Unicode；target覆盖absent/present-default/
unknown role/witness。没有mock、忽略项或新增feature；纯转换不触及MetaStore/I/O。

失败—通过：

- [missing.json](logs/task_134/missing.json)：首先缺锁文件依赖边，退出101；
  [registration.json](logs/task_134/registration.json)记录离线生成锁后缺少实现模块，
  聚焦命令退出101。离线生成意外升级的pulldown-cmark已恢复；最终Cargo.lock
  仅新增server到两个既有workspace依赖的边，无远程依赖版本变化。
- [green.json](logs/task_134/green.json)：初始178案例、2项测试通过。
- [red.json](logs/task_134/red.json)：[变异脚本](logs/task_134/mutation.py)临时清空
  region_id；同一测试执行2项，1通过/1失败，退出101。PD classic_empty的完整字节
  比较准确报出缺失字段4。finally恢复正确实现，未提交变异。
- [final.json](logs/task_134/final.json)：恢复后最终184案例、聚焦和完整验证。
  各命令保存合并stdout/stderr的压缩日志、退出码和日志SHA-256，没有裁剪故障输出。

所有Cargo阶段通过[槽位脚本](logs/task_134/verify.py)动态原子mkdir领取共享槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner含计划、任务、会话标识及持有PID；所有子进程继承该环境及GOFLAGS=-mod=readonly。
正常、失败、中断先等待/终止子进程组，然后释放自己的锁；保留缓存。

| 最终命令 | 退出码/有效测试 |
| --- | --- |
| `cargo fmt --all` | 0 |
| `cargo test -p astersql-pd-server --locked --lib task_134 -- --nocapture` | 0；2通过/0失败/0忽略，184案例 |
| `cargo test -p astersql-pd-server --locked` | 0；51通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0 |
| `cargo test --workspace --locked` | 0；1845通过/0失败/1既有忽略 |

workspace包含2项既有compile-fail文档测试，无新增忽略；仅既有协议deprecated字段警告。
所有子进程已退出，槽位1锁已释放，缓存保留。
[最终审计](logs/task_134/audit.json)确认1101份Go/模块文件集合及内容哈希一致，
无新Go文件，Go Git状态为空；整体与包级plan.md均与HEAD一致。
格式化未改动无关Rust文件，Cargo.lock无远程版本漂移；Markdown相对链接、
`git diff --check`通过。既有其他任务文件删除不纳入提交。
状态：已完成；完成门槛按用户授权的源码推导与Rust行为证据执行。
