# 生产元数据任务1的准入盘点

盘点基于 `0325af629150bdd6626cbf928ceb5a0afedb8d17`。当前主题9交付缺失，
生产实现和发布门保持关闭。可复用的契约和已运行的对照在此固定。

| 接口或错误 | 行为与消费者 | 可复用证据 |
| --- | --- | --- |
| `MetaStore::{consensus_cluster_id, consensus_leader_id, backend_health}` | 后端身份、leader 和端点健康观测，供 runtime 诊断。PD member ID 有独立来源 | [诊断装配](../../../pkg/runtime/etcd_diagnostics_test.rs) |
| `read`, `read_counted` | 线性或可串行化读、revision 固定分页和总数。供存储 endpoint、选主和配置恢复 | [共享契约](../../../pkg/storage/kv/contract_test.rs)、[有界范围](../../../pkg/storage/kv/bounded_range_test.rs) |
| `transact`, `transact_branches`, `transact_raw` | 比较后原子选择分支，保留操作顺序。`RawTxnOp::{Write, Get, GetRange}`，分别返回写入结果占位、点值、范围及 `more`。供 `Base`、持久化 endpoint 和选主 fencing | [分支](../../../pkg/storage/kv/raw_branches_test.rs)、[点读](../../../pkg/storage/kv/raw_get_test.rs)、[范围](../../../pkg/storage/kv/raw_range_test.rs) |
| `watch`, `watch_with_previous`, `watch_range`, `compact` | 按 revision 回放，范围和删除前值可选。供 etcdutil、TSO 和同步器 watch | [共享契约](../../../pkg/storage/kv/contract_test.rs)、[范围 watch](../../../pkg/storage/kv/range_watch.rs) |
| `MetaWatch::{header_revision, previous_value, next, next_with_progress}` | 区分事件游标与响应 header，批次保留删除前值，可取消 pull；可选进度通知 | [契约定义](../../../pkg/storage/kv/meta_store.rs)、[真实 WAL 重开用例](../../../pkg/storage/kv/etcd_store_test.rs) |
| `grant`, `keep_alive`, `revoke`, `ttl` | 租约附着和到期删除，供选主、fencing 和临时记录 | [共享契约](../../../pkg/storage/kv/contract_test.rs)、[选主任期包装](../../../pkg/election/leadership_store.rs) |
| `StoreError::{Cancelled, Unavailable, Compacted, FutureRevision, LeaseNotFound, RequestTooLarge, InvalidArgument, PermissionDenied, Backend}` | 调用方按终止取消、提交未知、重新快照及明确失败分类处理。`Unavailable` 的写入不能盲目重试 | [稳定错误类别](../../../pkg/storage/kv/meta_store.rs)、[提交取消用例](../../../pkg/storage/kv/txn_cancel_test.rs) |

`storage/domain` 保持传输无关。独立进程当前的适配入口是 `EtcdMetaStore`。
共享测试分别登记在内存和真实 etcd 入口，两侧各 11 个共享断言，etcd 入口
另有真实 WAL 重启、watch 续接和 lease 失效场景。
[领域入口](../../../pkg/storage/domain/kv.rs)、[独立进程装配](../../../cmd/pd-server/main.rs)、
[历史契约回执](../theme3-存储键值.md)。

Go 对照为验收16已冻结的 `b53bc9a01594+dirty`、Go `1.26.8`、
`pd-server-without-dashboard-amd64`，二进制 SHA-256 为
`364477a06e13afb29626ba57dbbd0be8245d0e9d68ad30f0dbeeda3dce3a3147`。
etcd 3.5.15 的 SHA-256 为
`ec97cf20f1dc01a4d4b3727784b876a2b5a24afbd05409ea5142ea6dcba55a13`。
两份本机文件均按验收15回执重新计算摘要并匹配。验收16保存的真实
MetaStorage RPC 和 WAL 重启属于外部 etcd 对照，适用于该服务层证据范围。
[验收15环境](../server-api/acceptance/task-15.json.gz)、
[MetaStorage 原始回执](../server-api/acceptance/task-16-metastorage.json.gz)。

原主题9批次86、87、88、92均未交付。仓库中 `tools/pd-recover` 只有 Go
入口和说明，缺 Rust recover crate、参数预检、旧成员身份保护以及备份恢复回执。
现有 Rust runtime 的恢复启动专项和 Go 工具源码是后续实施输入。生产任务30
要求复用主题9恢复工具，所以此处保留对应依赖。
[主题9](../../../.plans/2026-10-22-pd-rust-渐进迁移主题/9-迁移运维客户端与工具.md)、
[主题9恢复闭环](../../../.plans/2026-10-29-pd-rust-运维客户端与工具/92-备份恢复闭环.md)、
[现有恢复启动用例](../../../pkg/runtime/recovery_start_test.rs)。

本轮当前基准运行 `cargo test --offline --locked -p astersql-pd-storage-kv -- --test-threads=2`。
退出码 0，单元目标 36 通过，真实 etcd 契约目标 12 通过，合计 48 通过、
0 失败、0 忽略。doc-test 目标 0 项。实际槽位 1，绝对 `CARGO_TARGET_DIR`
为 `/Users/xiangmin/worker/code/pd/target/rust-slot-1`，运行结束后已释放。
`PD_COMPAT_ETCD_BIN` 选用上述已校验的现成二进制，避开 fixture 的 Go 构建分支。
构建传入已有 `GOMODCACHE` 和 `PROTOC`。本轮没有执行 Go 命令。
