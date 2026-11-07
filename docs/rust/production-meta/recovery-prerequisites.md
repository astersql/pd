# 恢复工具前置交接

基于 `85f650ecf9bf0cf1e8b8bc7b0948b1b5b969086e`。下面的 CLI 预期来自 Go
源码。当前找到的可执行 Go 对照是 `pd-server`，本轮未找到同源 `pd-recover`
二进制。CLI stdout、退出码及副作用仍须通过原任务86的进程差分验证。
[Go CLI](../../../tools/pd-recover/main.go)、[工具说明](../../../tools/pd-recover/README.md)。

| 原拥有任务 | 可固定的输入与断言 |
| --- | --- |
| 运维86 | 无参数和 `-h` 显示 usage，`-V` 在连接存储前返回版本。畸形参数走 `flag.ExitOnError`。TLS 配置失败打印 `failed to connect: err` 后正常返回。新集群缺 `cluster-id` 或 `alloc-id` 打印相应提示并返回，未提交事务。差分应保留 stdout/stderr、退出码和键空间摘要 |
| 运维87 | 新集群分支以 bootstrap key 不存在作比较，单事务写 cluster ID、alloc ID、cluster protobuf 和 bootstrap 时间。重复执行验证比较失败及原值保留 |
| 运维88 | 旧成员分支从现有 cluster ID 建根路径，以 bootstrap key 存在作比较，原子抬高 ID 窗口并删除 store 前缀。测试应覆盖错误身份、分支拒绝及范围外键保留 |
| 运维92 | 备份产物进入同一 recover CLI，重启后读取 cluster ID 和新分配的 ID。实际备份内容、工具退出码和恢复后的状态均需保留 |

现有 Rust 服务端的 ID 和 TSO 管理入口可作为宿主侧验证。运行
`cargo test --offline --locked -p astersql-pd-server --lib http_admin_recover_ -- --skip differential --test-threads=1`，
退出码 0。`task_075_behavior`、`task_075_fault_and_cancel`、`task_076_behavior`
和 `task_076_clock_fault_cancel` 均通过，0 失败、0 忽略，323 过滤。
实际槽位 1，`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
使用已核验的外部 etcd 3.5.15 fixture，结束后已释放槽位。这四个用例涉及管理
操作与故障处理，未启动 `pd-recover` 或 `pd-backup`。
[ID 管理用例](../../../pkg/server/http_admin_recover_id_test.rs)、
[TSO 管理用例](../../../pkg/server/http_admin_recover_tso_test.rs)。

运维任务86的“范围与依赖”要求批次85及全部前序批次交接，并核验
`coverage.md`、`consumers.md`、`runtime-boundary.md`。这三份文档和85回执
在当前工作区缺失。恢复工具 crate 由86首次创建，87/88依赖其入口，92还依赖
91备份 CLI。生产任务30明确复用这些工具。预检向量与现有宿主回归可独立交接，
CLI 注册、写入行为与恢复闭环须在原运维链中完成。
[运维86](../../../.plans/2026-10-29-pd-rust-运维客户端与工具/86-恢复工具参数与预检.md)、
[运维92](../../../.plans/2026-10-29-pd-rust-运维客户端与工具/92-备份恢复闭环.md)、
[生产30](../../../.plans/2026-10-29-pd-rust-生产元数据与发布切换/30-验证备份恢复运维流程.md)。
