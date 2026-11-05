# 任务044：推进事务安全点

状态：已完成，2026-11-05，基准39f141d51。
[task-044-evidence.json.gz](task-044-evidence.json.gz)保存red/green、全部原生/Go命令及18组真实RPC/持久状态/WAL差分。

`advance_txn_safe_point`复用43的真实GC存储、metadata重定向、manager读写锁及缓存。
先加载txn与GC safe point，再拒绝target下降；以GC revision CAS共同提交新txn point、过期local/global屏障删除，
以及已存在gc_worker的兼容rewrite。没有gc_worker时不新造它；存在时忽略其旧TS和过期时间，写回实际new point及无限有效期。
即使target等于旧值也执行Go原有写入/revision行为。

阻塞取最小local barrier、TiDB min-start-ts及global barrier，严格小于才替换，保留同值时原先来源；
实际new=max(old,min)，过旧legacy blocker不会使状态倒退。min-start为0或空owner不阻塞。
TiDB owner保持原始bytes，synthetic service ID保留tidb_min_start_ts_前缀；诊断按Go %+q进行ASCII转义，含非法UTF-8。
诊断中全局优先于min-start，再优先local；描述中min-start值采用实际new point，匹配Go降级兼容路径。
宿主可注入诊断时区formatter，默认UTC；领域过期判定始终使用绝对BarrierTime。

PreparedTxnSafePointAdvance持有真实manager写锁和缓冲事务，允许宿主在实际提交边界取消。
阶段/提交失败均失效该scope缓存，确认成功才发布pair；提交future被丢弃或取消也保守失效，防止未知结果残留暖cache。
来源gc_state_manager.go:421–607、1255–1267；实际调用server/gc_service.go:AdvanceTxnSafePoint。
采用43的公共begin/commit能力，无mock或第二个算法，未在适配层放领域运算。

验证：

- 可运行red：真实存储先写10，再写9错误地成功；行为断言失败exit101；实现后通过。
- 9项专项真实etcd测试通过；GC整包17项通过（含43的8项读取回归）。
- local/min/global阻塞及相等优先级，低于old的blocker、原始0xff owner和Unicode barrier诊断均用实际存储验证。
  严格过期边界100s仍活、100s+1ns才删除，worker rewrite/txn写入/revision与两个lazy delete共5个watch事件同revision。
- 独立manager的预备批次同时CAS，恰好一方成功；随后u64::MAX推进成功，MAX-1被拒绝且持久值不变。
  另在预备后通过真实GC事务改写revision，失败批次不删除过期屏障，不发布旧pair；暖cache失效后重新读出40/35。
- 无效min-start发生在lazy deletion暂存后，整批不落盘；修复后重试完成。
  无效GC point优先于下降target错误；解析失败不发布部分结果。
- 准备后停止真实etcd，commit报错；同WAL fresh manager看到原point和过期屏障，重试才原子推进/清理。
- 实际提交响应被TCP闸门持有，独立etcd watch确认txn point已写30，再取消调用；返回Cancelled、缓存失效，
  放行后读取30，不把未知提交当作回滚。此处没有伪造DeadlineExceeded/commit返回值。
- 把真实共享读future停在未消费完成通知的位置，两次真实下降写入失败分别使cache失效；
  原请求直接读取原子pair并完成。随后停止etcd，新的exclude读取报错，证明fallback没有依赖/填充缓存。
- 只读同源Classic与NextGen各运行AdvanceTxnSafePointBasic、GCBarriers、GlobalGCBarriers、TiDBMinStartTS，
  每内核4叶例通过；这些方法及setup无需failpoint。保留原test2json和二进制摘要。
- 在线18组Go真实AdvanceTxnSafePoint RPC对比同输入Rust领域调用，覆盖推进/同值/下降/local/ties/min/global/
  老blocker/expired清理/有限时区描述/legacy worker与Unicode/u64上界/两个解析错误/独立空间/统一空间拒绝/缺失/Null别名。
  成功的old/new和完整blocker description一致；稳定领域错误完整文本一致，strconv解析错误保留两边原始文本并比较typed类别。
  Go TZ和Rust host formatter同为显式Asia/Shanghai输入，不删改时间或描述字段。
  双方GC revision、txn/GC point、local/global屏障及min-start的完整持久bytes一致；失败逐项保留原create/mod revision及bytes。
  同WAL重启双方仍一致，实际物理key/revision全部写入证据；仅对跨集群的已知路径前缀用固定角色位置进行bytes比较。
- 共享Go启动夹具仅增加显式environment入口；原包装输入不变，32默认组合并原在线回归通过。
- metadata、fmt、workspace check及sa_002动态架构守卫通过；完整workspace test按外层24统一回归。
  各阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1，结束释放自有锁。
- 1101项Go及模块文件首尾存在性/SHA一致。未改Go、模块、failpoint、plan.md或来源史诗。

此任务是领域能力；真实PD任期装配仍由132负责，后续继续45屏障。
