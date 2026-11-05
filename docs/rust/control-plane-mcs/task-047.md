# 任务047：推进GC安全点及旧版服务映射

状态：已完成，2026-11-05。

AdvanceGCSafePoint复用同一GC manager写锁和revision CAS，先读GC再读txn；严格入口拒绝回退及超过txn，
兼容入口回退返回原GC并执行只读CAS。相等目标仍验证txn并写入；旧GC已超过txn时兼容回退仍返回原值，保留Go行为。
局部、全局及TiDB屏障通过44的txn推进约束GC上界，不在transport重复实现。
成功才发布缓存pair，失败、取消或提交结果不确定使该pair失效。prepared事务保留可验证的真实提交边界。

CompatibleUpdateServiceGCSafePoint在同一manager写锁下分支复用44–46领域操作：
gc_worker只接受MaxInt64秒并推进txn；普通服务执行local Set/Delete再读取txn；NULL空间native_br执行global Set/Delete再读取所有Enabled空间最大txn。
只有对应的behind错误被忽略。后续观察与首次提交是两个真实存储步骤，后读失败不撤销已提交的barrier。
旧服务ID保留protobuf原始bytes作为物理key；JSON逐字节匹配Go encoding/json的非法UTF-8替换和HTML转义。
TTL采用饱和秒→纳秒及既有向上整秒规则。接口返回绝对expired_at，RPC应以真实TSO时间转换剩余TTL。

来源：gc_state_manager.go:314–421、1130–1255及相邻既有测试；server/gc_service.go。
共享typeutil保存Go strconv显式base ParseUint的溢出顺序、NumError全文与%q/%+q编码；
数字损坏、GC revision语法、真实CAS冲突的稳定错误消息已通过在线Go全文比较。

验证证据存于[task-047-evidence.json.gz](task-047-evidence.json.gz)：

- 最初真实etcd行为失败：txn10时错误接受GC11；随后实现通过。
- 10项专项、GC包41项、相邻storage GC21项通过；typeutil完整包及工作区检查另见命令回执。
- 覆盖严格/兼容/相等/旧状态不一致、u64边界、管理类型、取消、真实CAS及缓存失效、同WAL重试。
- 真实网络gate停在barrier提交响应；独立watch证实已提交后停止etcd，随后LoadTxn实际Unavailable；同WAL保留barrier。
- 同源只读Go Classic/NextGen分别执行6个既有GC叶例，全部通过，无failpoint注入。
- 在线真实Go RPC与Rust领域：12组GC推进包括语法/溢出/revision错误及present-empty revision导致真实CAS冲突；13组服务操作包括非法UTF-8 ID、worker最小值、原始服务回退/忽略behind、native_br、有限及饱和TTL。
  成功返回及错误全文、完整持久GC bytes一致；失败保持原始create/mod revision。双方同WAL重启恢复一致。
- 服务响应TTL反推出该次Go真实TSO整秒；有限存储期限选取相同整秒舍入区间的显式Rust输入，保留时间字段，精确纳秒边界由原生测试验证。
- Cargo动态槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1，命令退出后释放锁。
- 1101项Go/模块文件存在性与SHA保持不变，未运行生成器、Go格式化或failpoint命令。
- 完整workspace test按外层任务24集中执行；本任务执行workspace check、metadata、fmt和sa_002架构守卫。

这一步不声称宿主132或外层legacy RPC验收已完成。
