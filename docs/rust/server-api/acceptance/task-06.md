# 任务06：键空间GC屏障验收

2026-11-06，基准 `567d7f7cf`。SetGCBarrier、DeleteGCBarrier 验收完成。
[本轮原始回执](task-06.json.gz)保存命令、失败/通过日志、输入摘要、原始物理key与bytes/create/mod revision、RPC观察及Go守卫。
覆盖 coverage.json 的 `wire:560faf2e9f467d581d10d0b2`、`wire:fc7f5963338202722d5964e7`，来源 wire-inventory.md:52–53、server/gc_service.go:616–693。
历史来源清单保持原样。

复用[45](../../control-plane-mcs/task-045.md)的 canonical 屏障领域实现、[132](../../control-plane-mcs/task-132.md)的真实任期 manager，以及[批次5](task-05.md)的推进RPC。
仅增加 Grpc/SyncGrpc 适配及测试登记；TTL通过既有饱和转换传入manager，按Go在调用开始取时钟并返回剩余TTL，领域失败保留UNKNOWN响应header。
未复制领域事务、metadata重定向、ID约束或过期算法。task037待实现清单移除这两个已实现RPC；废弃watch继续保留精确Unimplemented契约。

本轮两个真实进程分别使用自身etcd，真实tonic与独立MetaStore读取验证：

- 独立空间2/3同名屏障隔离；新增、TS上调/下调/下限相等，低于txn point拒绝；统一空间4重定向Null，Null/超范围别名一致；Unicode/slash及u64最大TS。
- reserved/empty ID、缺失metadata、零/负TTL及验证优先级；Delete返回原屏障，不存在返回None但仍推进GC revision。
- 每次成功恰增一个GC revision；Set屏障与counter同mod_revision；失败前后完整entries不变。
- 有限TTL 1、60、9223372036秒及饱和无限边界，原始持久expiry均保留，按实际调用前后wall-clock严格验证向上舍入区间；Set响应TTL精确等于输入有限秒或饱和MAX。
  Delete有限TTL按原始expiry和调用前后时间验证floor/max，不抹去TTL字段。独立进程时钟不同的有限expiry不作相同bytes的虚假比较。
- 30→3600→7200秒续期和TS30→40→25在同一记录生效；有限续期记录及其它三个scope完整entries经同WAL重启保留，包括create/mod revision。
- 通过本组Set建立1秒屏障，实际推进先阻塞，过期后再次推进原子删除屏障；精确纳秒与刚好到期边界由本轮8项mcs_045测试验证。
- 未bootstrap、精确错cluster、真实第二节点follower；真实etcd空但存在的revision造成CAS冲突，两RPC均返回错误且无副作用。
- 对Set/Delete分别扣留真实TCP响应：独立etcd读取确认创建/删除已提交，再观察Cancelled/Timeout expired。客户端取消不能推断事务回滚。
  暂停服务期间非法Set和reserved Delete超时，恢复后完整entries不变。
- 除独立cluster ID外，无限TTL矩阵的完整header/code/message、barrier字段及持久bytes跨实现精确比较；原始cluster ID与所有revision在回执保留。
  双方默认空间metadata输入相同；自动DEFAULT bootstrap仍归键空间宿主验收。

验证（默认features，详细参数、退出码、计数及槽位见回执）：

- red：`cargo test --locked -p astersql-pd-compat-tests --lib sa_006_ -- --nocapture`，0 passed/1 failed，exit101，真实RPC Unimplemented/Not yet implemented。
- 最终同一聚焦命令：2 passed/0 failed/0 ignored，exit0。
- `cargo test --locked -p astersql-pd-gc --lib mcs_045_ -- --nocapture`：8 passed/0 ignored，exit0；含真实提交故障、同WAL恢复重试和严格过期边界。
- `cargo test --locked -p astersql-pd-server --lib task_078_ -- --nocapture`：2 passed/1既有ignored，exit0，忽略项不计通过证据。
- `cargo test --locked -p astersql-pd-server --lib task_037_ -- --nocapture`：更新待实现清单后2 passed/0 ignored，exit0；此前1 passed/1 failed完整保留。
- `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server`、metadata、workspace check、fmt check与git diff --check通过。
- 中间一次额外聚焦运行因动态槽位2尚无pd-server而失败，不计行为red；最终在同一锁定槽位先构建再测试，杜绝跨槽位依赖产物。
  完整workspace test继续集中任务24，无关领域无重复全量回归。

Cargo阶段原子领取槽位1、2、3；绝对CARGO_TARGET_DIR分别为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`、`/Users/xiangmin/worker/code/pd/target/rust-slot-2`、`/Users/xiangmin/worker/code/pd/target/rust-slot-3`。
确切阶段与槽位见回执；全部Cargo及产物子进程结束后释放自有锁，保留缓存。
固定Go oracle、kvproto f8ad079、官方etcd3.5.15/etcdctl、PROTOC与Go SDK归档/成员摘要均重新核验。
1101项Go/模块文件首尾存在性和SHA一致；未运行Go构建/测试、make、failpoint或生成/整理。
保留用户已有删除与其它计划文件；本计划plan.md、领域plan和来源史诗保持只读。
