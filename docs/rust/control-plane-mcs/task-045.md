# 任务045：维护键空间GC屏障

状态：已完成，2026-11-05，基准2304f53e3。
[task-045-evidence.json.gz](task-045-evidence.json.gz)保存red/green、原生及双内核原例、在线RPC/HTTP和WAL证据。

SetGCBarrier先校验正TTL，再执行用户API的metadata重定向，最后在同一个GC manager写锁中校验reserved/empty ID。
高于24-bit的别名、统一GC空间落到NullKeyspace；metadata不存在优先于reserved ID错误。
拒绝barrierTS低于当前txn safe point，允许等于，以及在这个下限之上的barrierTS回退/延长期限。
正TTL保留纳秒精度，now+ttl后向上舍入整秒；i64::MAX纳秒表示永不过期，MAX-1仍是有限期限。
过期时间写入legacy SSP metadata，不附加etcd lease；实际删除由44确认提交的推进事务完成。

DeleteGCBarrier保留同一重定向及ID约束，在revision CAS中加载旧barrier、删除并返回它；不存在返回None但仍更新GC revision。
ForceDeleteServiceGCSafePoint复用同一manager锁，保留legacy HTTP绕过reserved/empty及旧记录decode的语义。
Set/Delete不修改safe-point cache，即使报错也保持Go与44推进错误失效缓存的区别。
PreparedGCBarrierChange给宿主真实事务边界，确认提交才返回barrier结果；没有假storage/后台TTL worker。
BarrierTime只增加有符号纳秒相加，复用既有舍入、物理表示及strict IsExpired。
来源gc_state_manager.go:662–802；调用server/gc_service.go的Set/Delete及server/api/service_gc_safepoint.go的force delete。
CompatibleUpdateServiceGCSafePoint的native_br全局分支与完整legacy映射需要46/47，未在此任务或transport里伪造完成。

验证：

- 可运行red 1项：txn point为10却接受barrierTS9，实际状态断言失败exit101；实现后通过。
- 8项专项真实etcd通过，GC整包25项通过（保留43/44全部17项）；既有GC存储21项相邻回归通过。
- 显式精确时钟验证1ns、跨秒进位、刚好整数秒、MAX-1有限期限与MAX无限期限；
  负epoch的立即返回值保留负expiration，而legacy物理重载将非正expiration解释为无限，匹配既有Go codec。
  实际barrier key无lease，持久SSP最大expiry为9223372036854775807。
- TTL错误优先于missing metadata，missing优先于reserved；保留gc_worker、empty、负TTL的typed和原文错误。
  native_br普通屏障名及含slash名称照原路径语义保存，统一/超范围别名正确重定向。
  barrierTS30→40→25→20均可，低于txn20被拒绝。
- Set及GC revision两个watch事件同revision；删除不存在仍使revision计数+1。
  真实外部txn point改写后，屏障set失败/成功/delete均保留原暖point cache，full读取仍能看到实际新值。
- prepared set之后外部GC事务推进导致CAS冲突，没有barrier落盘；prepared delete之后外部更新barrier，失败批次保留新值40。
- set及delete分别在准备后实际停止etcd，commit失败；同WAL fresh manager分别看到未创建/未删除状态，再重试完成。
- 普通删除拒绝reserved/empty；force delete可删除这两种名字的损坏JSON，无需反序列化，也可重复删除不存在项。
- 从真实Set创建的expiry102开始，推进在102秒仍被阻塞，102秒+1ns才原子删除/推进，过期屏障不再阻止GC。
- 同源Classic与NextGen各实际运行TestGCBarriers、TestCompatibleServiceGCSafePointRoundingTTL两叶例通过；
  setup及方法无failpoint依赖，原test2json、只读sandbox命令及二进制SHA保留。
- 在线20组Go SetGCBarrier RPC对比同输入Rust，含1s/60s/最大有限秒/饱和无限TTL、非法TTL/ID、过旧TS、
  独立/统一/缺失/别名、Unicode、native_br、slash及同一barrierTS下降更新。
  成功barrier完整domain值和persisted SSP bytes一致，稳定错误全文一致；所有失败前后物理bytes/create/mod revision不变。
  成功路径继续真实Delete RPC，返回ID/TS及absolute expiration一致，缺失再删均返回None，双方GC revision与全状态bytes一致。
- Go RPC不暴露now：有限TTL用其真实持久expiry和输入整数秒TTL确定的等价舍入区间中点作为Rust显式时钟，
  不声称提取了Go纳秒时钟。Set返回TTL与输入饱和值精确比较；Delete响应TTL验证在调用前后实际wall-clock区间内，
  不删字段或固定成宽松常量。精确纳秒舍入/严格过期边界由上述原生测试独立验证。
- 真实legacy HTTP force delete删除损坏gc_worker记录，HTTP200与Rust结果、实际持久状态一致；
  双方同WAL重启后完整状态bytes仍一致。物理key、原始JSON bytes、header/create/mod revision全部记录。
- fmt、metadata、workspace check及sa_002架构守卫通过；完整workspace test按外层24统一回归。
  各Cargo阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1，结束释放自有锁。
- 1101项Go/模块文件存在性和SHA完全不变；未改Go、模块、failpoint、plan.md或来源史诗。

继续必要领域依赖，尚未宣称外层旧版GC RPC验收完成。
