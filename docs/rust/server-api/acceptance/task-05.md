# 任务05：事务与GC推进验收

2026-11-06，基准 `5feb52d91`。AdvanceTxnSafePoint、AdvanceGCSafePoint 验收完成。
[原始回执](task-05.json.gz)保存本轮命令、失败及通过日志、输入指纹、RPC观察、实际物理记录与revision、WAL恢复及Go守卫。
对应 coverage.json 的 `wire:2dd7e6ead8c402ab91f9bc3f`、`wire:6ae6e64de1b8c1b3ba80dac6`，来源 wire-inventory.md:50–51、server/gc_service.go:542–614；旧清单作为历史输入保持原样。

复用[44](../../control-plane-mcs/task-044.md)、[47](../../control-plane-mcs/task-047.md)、[132](../../control-plane-mcs/task-132.md)的真实领域manager与任期存储，及[批次4](task-04.md)的当前服务接线。
仅增加 Grpc/SyncGrpc 的现代推进适配：事务推进传入实际系统时间，领域失败放UNKNOWN响应header，成功保留old/new及完整blocker description。
未在transport重写事务、屏障或CAS；两个废弃watch继续复用task037精确Unimplemented契约。

本轮真实TCP/tonic双进程矩阵每实现42项观察、30组原始持久状态证据：

| 方法 | 成功、参数与屏障 | 角色、取消与事务故障 | 耐久性 |
| --- | --- | --- | --- |
| AdvanceTxnSafePoint | 全局/独立空间、相同目标仍写revision、下降拒绝、Null及超范围别名、u64最大值；local/global屏障、过旧TiDB min-start不回退、过期local清理、完整阻塞描述；统一GC及缺失空间拒绝；空/V3 scope按Go getter映射0 | 未bootstrap、精确错cluster、实际第二节点follower；真实etcd空但存在revision引发CAS冲突且完整entries不变；扣留真实TCP响应，独立etcd读取确认110已提交，再观察客户端Cancelled/Timeout expired | 三个scope完整entries从同etcd WAL重启恢复；恢复后继续推进 |
| AdvanceGCSafePoint | 相同目标、合法推进、下降及超txn拒绝、独立空间隔离、Null别名、u64最大值及MAX-1错误；统一GC/缺失空间/空和V3 scope错误header | 相同四类入场边界与CAS冲突；扣留响应期间独立确认60已提交；暂停进程期间非法下降请求超时，恢复后持久entries不变 | 相同三个scope从同WAL恢复，非MemoryStore |

成功写入逐项验证GC revision counter恰增1，point和counter的mod_revision相同；失败和恢复逐项比较bytes/create/mod revision。
跨实现只归零各自独立cluster ID；原ID在回执保留，可以恢复原始header，物理key及所有revision不删除。
持久bytes、完整错误header/code/message及blocker description跨实现精确比较；两个写RPC的取消不能被解读为事务回滚。

夹具为双方提供相同键空间metadata（含DEFAULT/0），与任务4一致；不把Go自动初始化DEFAULT而Rust夹具未初始化的不同输入当成GC差分。
默认空间自动bootstrap属于键空间宿主验收，本项不声称交付它。44的有限时区/原始owner和提交故障域证据仍归44，本项没有伪称重跑全部领域矩阵。

验证（所有命令默认features）：

- red：`cargo test --locked -p astersql-pd-compat-tests --lib sa_005_ -- --nocapture`，0 passed/1 failed，exit101，真实RPC返回Unimplemented/Not yet implemented。
- `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server`，exit0。
- 最终同一聚焦命令，2 passed/0 failed/0 ignored，exit0。
- `cargo test --locked -p astersql-pd-server --lib task_078_ -- --nocapture`，2 passed/1既有ignored，exit0；忽略项不算证据。
- `cargo test --locked -p astersql-pd-server --lib task_037_ -- --nocapture`，2 passed/0 ignored，exit0。
- `cargo test --locked -p astersql-pd-runtime --lib mcs_132_ -- --nocapture`，2 passed/0 ignored，exit0。
- `cargo test --locked -p astersql-pd-compat-tests --lib sa_004_ -- --nocapture`，2 passed/0 ignored，exit0。
- metadata、workspace check、fmt check、Python AST与git diff --check均exit0；完整workspace test仍集中外层任务24。
- 中间测试失败完整保留：响应闸门通知可能早于事务提交，改为独立轮询实际持久值；合法请求在客户端超时后仍可能推进，取消无写断言改用下降target；DEFAULT元数据输入不一致导致完整错误消息不等，改为双方一致fixture。上述失败均不冒充生产实现red或一次全绿。

Cargo各阶段动态原子领取槽位1或2，绝对CARGO_TARGET_DIR分别为 `/Users/xiangmin/worker/code/pd/target/rust-slot-1`、`/Users/xiangmin/worker/code/pd/target/rust-slot-2`。
确切阶段/槽位/退出码在回执；Cargo及产物子进程结束后释放自有锁并保留缓存。
固定Go oracle、kvproto f8ad079、官方etcd3.5.15/etcdctl、PROTOC与SDK均重新核验摘要；SDK二进制与已核验归档成员一致。
1101项Go及模块文件首尾存在性/SHA一致，未运行Go构建/测试、make、failpoint或生成/整理。plan.md、领域plan与史诗保持只读。
