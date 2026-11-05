# 任务046：维护全局GC屏障

状态：已完成，2026-11-05，基准9f92311c9。
[task-046-evidence.json.gz](task-046-evidence.json.gz)保存行为失败/通过、两内核Go逐例和在线13组Set/5组Delete/WAL差分。

SetGlobalGCBarrier复用GC manager写锁和revision CAS，先校验TTL再校验非空ID；gc_worker可作为global ID。
采用45的纳秒TTL、向上整秒及MaxInt64无限期限规则，读取最大txn point后才缓冲全局barrier写入。
DeleteGlobalGCBarrier在同一revision验证边界读取旧值并删除，缺失返回None，仍保留GC revision写入行为。
所有失败不返回barrier或发布partial状态，不修改safe-point cache。

MaxTxnSafePoint按既有Go Iterator的100项批次扫描全部metadata，只考虑Enabled，但不按gc_management_type过滤；
每个Enabled空间的原始持久txn point均参与，包括unified记录。Disabled/Archived/Tombstone/未知state跳过，孤立point无metadata不参与。
同值保留最先扫描的metadata ID，最后才比较NullKeyspace；全零返回Go原有0/空name/ID0。
扫描、point读取和写入共同由GC revision CAS验证，未声称metadata本身已实现48的固定revision快照。
达到最大metadata ID时仍执行Go的lastID+1最终扫描，保留其16777216越界错误。
非前进identity返回显式NonAdvancingKeyspaceScan；此非法identity的保护分支未计入Go行为等价运行证据。
公开get_max_txn_safe_point同样通过真实只读GC事务验证，供47的native_br兼容边界复用。

诊断name使用Go %q（保留可打印Unicode），44已有barrier诊断仍使用%+q（ASCII转义），二者共用同一byte-safe formatter。
来源gc_state_manager.go:1294–1438、keyspace.go:1597–1673；实际调用server/gc_service.go的global Set/Delete。

验证：

- red真实etcd：统一txn point20时错误接受global TS19，行为断言失败exit101；实现后通过。
- 专项6项、GC包31项通过。109个metadata跨100项分页，统一记录参与、非Enabled不参与，
  metadata/null同值优先级、Unicode/quote/NUL name完整错误描述，均验证实际最大值和持久结果。
- 有限TTL跨秒与无限期限、global gc_worker、empty/非正TTL的次序、TS递减但不低于最大txn point、缺失删除均通过。
- malformed protobuf或txn数字报错，不部分保存barrier；最大valid ID触发最终越界，未把短页当扫描结束。
- prepared set后独立GC事务改写point，CAS冲突不保存barrier且保留旧point cache；
  prepared delete后外部替换barrier，冲突保留新TS30。阶段无伪造store响应。
- set与delete分别在实际停止etcd的提交边界失败；同WAL fresh manager分别看到未创建/未删除，重试完成。
- 同源Go Classic/NextGen各运行TestGlobalGCBarriers、TestGetAllKeyspacesMaxTxnSafePoint，两叶例均通过，无failpoint。
- 在线Go真实RPC与Rust领域调用：超过100项metadata，包含Enabled统一/独立及各非Enabled state和孤立MAX point，
  13组Set覆盖跨页最大值、推进/回退、Null最大、metadata/null同值、统一记录旧point、有限TTL、global reserved名、
  TTL/empty次序、损坏metadata和最大ID最终越界。成功完整barrier/TTL与持久bytes一致；稳定领域错误全文一致。
- 初始oracle错误地假设range protobuf错误带proto包装；实际source LoadRangeKeyspace直接返回gogo错误，
  与point load的RFC包装不同。修正为断言实际Go unexpected EOF，并保留失败日志；Rust保留typed Keyspace decode错误。
  另一次仅测试import缺失的编译失败保留，不计行为失败证据。
- 5组Delete覆盖普通/有限期限/global gc_worker/缺失/empty。ID/TS和absolute expiration匹配，
  有限Delete TTL约束于真实请求前后wall-clock范围，∞精确为MaxInt64。全部失败保持原bytes/create/mod revision。
  双方同WAL重启后完整全局GC持久bytes一致，真实key和revision全量记录。
- 有限Set now采用45已声明的等价舍入区间显式输入，精确纳秒边界由原生测试验证；不删除时间字段。
- metadata、fmt、workspace check及sa_002动态架构守卫通过；完整workspace test按外层24统一回归。
  Cargo阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1，结束释放自有锁。
- 1101项Go/模块文件存在性/SHA及Git状态不变；未改Go、模块、failpoint、plan.md或来源史诗。

继续47的GC兼容推进和完整legacy service映射，之后才可接通外层RPC。
