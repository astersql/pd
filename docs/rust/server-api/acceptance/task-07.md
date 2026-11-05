# 任务07：全局GC屏障验收

2026-11-06，基准 `c6b4f1fd1`。SetGlobalGCBarrier、DeleteGlobalGCBarrier 接通同一任期所有的 canonical GC manager。
[本轮原始回执](task-07.json.gz)保留确切命令、输入摘要、失败及通过日志、真实RPC响应、持久key/bytes/create/mod revision与Go守卫。
来源 server/gc_service.go 的 SetGlobalGCBarrier/DeleteGlobalGCBarrier、wire-inventory.md 对应两行；
复用[46](../../control-plane-mcs/task-046.md)的全局屏障算法、[132](../../control-plane-mcs/task-132.md)的任期资源和[批次6](task-06.md)的既有接线模式。
未复制领域事务、metadata遍历、最大txn point及过期算法；两个废弃watch保持原Unimplemented合同。

真实Go/Rust进程分别使用自身真实etcd与WAL，真实tonic客户端和独立MetaStore观察：

- 未bootstrap、错cluster精确码/消息、真实第二节点follower均覆盖Set/Delete；错误不改变GC持久entries。
- 全局新增、TS上调/下调/最大txn point相等及小于最大值拒绝；empty ID、非正TTL及优先级、Unicode/slash、u64最大TS。
  global `gc_worker` 是合法名称；删除缺失仍恰好推进一次GC revision。
- 无限TTL的完整header、barrier与持久bytes跨实现精确比较（仅归一独立cluster ID）；Set屏障和GC counter具有相同mod_revision。
  原始cluster ID与物理revision均保留；每个错误验证完整entries无副作用。
- 有限TTL 1、60、9223372036秒及饱和边界；Set TTL精确比较，持久expiration按调用前后wall-clock验证向上整秒区间。
  Delete TTL按原始expiration和调用前后时间验证floor/max，不去掉时间字段，也不把不同时钟产生的expiration宣称字节相等。
- 真实Set建立的全局屏障约束Null统一scope和两个独立空间的AdvanceTxnSafePoint；过期后推进并原子清理全局记录。
  续期30→3600→7200秒与TS30→40→25在同一全局记录生效，完整持久entries经同WAL重启保留。
- 强制真实etcd revision CAS冲突，两方法均返回错误且无副作用。扣留真实TCP响应，独立etcd先观察Set/Delete已提交，
  再观察客户端Cancelled/Timeout expired；取消不表示回滚。暂停服务期间非法Set/Delete取消后不写状态。
- 本轮6项mcs_046领域验证补充：跨100项metadata分页、Enabled统一记录参与、非Enabled和孤立point排除、Null及同值优先级、
  损坏数据和最大ID越界、prepared CAS、实际停etcd提交失败与同WAL重试。领域来源证据不替代本轮RPC差分。

验证（默认features）：

- 行为red：聚焦sa_007命令0 passed/2 failed/0 ignored，exit101，真实RPC返回Unimplemented/Not yet implemented。
  此前两次缺失测试字段的编译失败单列保存，不计行为red。
  扩展测试曾错误地直接推进unified空间4，Go返回管理类型错误；修正为通过Null统一scope验证，原始失败保留。
- 聚焦 `cargo test --locked -p astersql-pd-compat-tests --lib sa_007_ -- --nocapture`：首轮及最终跨空间矩阵均2 passed/0 failed/0 ignored，exit0。
- `cargo test --locked -p astersql-pd-server --lib task_078_ -- --nocapture`：2 passed/0 failed/1既有ignored，exit0。
- `cargo test --locked -p astersql-pd-server --lib task_037_ -- --nocapture`：2 passed/0 ignored，exit0。
- `cargo test --locked -p astersql-pd-gc --lib mcs_046_ -- --nocapture`：6 passed/0 ignored，exit0。
- build、metadata、workspace check、最终fmt check和git diff --check通过；完整workspace test按计划集中任务24。
  中间fmt check检测到新增跨空间断言的未格式化排版，先cargo fmt --all后复跑聚焦与fmt check，未隐去失败日志。

Cargo阶段动态领取槽位1、2；绝对CARGO_TARGET_DIR为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`、`/Users/xiangmin/worker/code/pd/target/rust-slot-2`。
所有Cargo与产物子进程结束后释放自有锁、保留缓存；最终build和差分在同一锁定槽位运行。
固定Go oracle、kvproto f8ad079、官方etcd3.5.15/etcdctl、PROTOC及SDK归档/成员摘要本轮重新核验。
1101项Go/模块文件首尾存在性及SHA一致，未执行Go构建/测试、make、failpoint或生成/整理。
只提交本项Rust、测试和证据；保留用户已有删除及其它任务文件。plan.md、领域plan和来源史诗保持只读。
