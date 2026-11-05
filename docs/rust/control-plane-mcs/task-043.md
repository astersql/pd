# 任务043：读取键空间GC状态

状态：已完成，2026-11-05，基准caf4df5fa。
[task-043-evidence.json.gz](task-043-evidence.json.gz)保存失败/通过命令、双内核Go逐例、真实RPC差分与WAL结果。

新canonical `astersql-pd-gc`接受宿主StorageEndpoint、ClusterPaths、KeyspaceManager和Tokio Handle，
领域类型不依赖proto或网络框架。复用既有GC存储算法，六个中立模块由storage-domain唯一编译，
wire StorageEndpoint通过既有Deref和类型重导出保持兼容；无双份生产算法。
GCStateWriteBatch增加显式begin/commit事务状态，原AsyncFnOnce入口委托同一实现，避免跨任务的generic async callback限制。
协议/JSON原值、GC revision比较、写入revision计数及无重试语义保持；原21项GC存储测试继续验证。

GetGCState将高于24-bit的所有ID映射NullKeyspace，存在但非keyspace_level的空间在用户读取中重定向统一GC；
缺失metadata直接返回KeyspaceNotFound。内部legacy读对非keyspace_level空间返回ErrGCOnInvalidKeyspace。
读metadata不按生命周期过滤，因此单点读取Archived仍可成功。
无active generation时不使用已填缓存；active代际的exclude=true读共享原子safe-point pair，包含barrier的读始终验证GC revision。
非排除读取不替换已有点缓存；失败读取不发布部分/零值。过滤gc_worker，保留expired barrier供调用层表达TTL。
全局barrier包含标志始终执行同一revision验证边界，普通读取不读取global行；任一失败不返回部分状态。

GCLeadership由宿主持有，retire先禁用缓存、取消代际，shutdown等待TaskTracker任务并清理退役缓存；
新代际取消/等待旧任务再清空缓存、发布唯一代际。旧cleanup不能禁用新代际。
共享读取只在传入Handle运行，任务受代际token及tracker所有，单个等待者取消不取消共享IO。
缓存被中间writer失效后最多共享重试两次，再直接返回真实原子pair；代际/请求取消均可终止直接IO。
此任务未标记48–51的固定revision metadata分页/watch/后台批量预热/retry队列完成，未安装这些后台服务。
43的代际只负责前台读取安全；真实PD任期装配由132负责。

来源：gc_state_manager.go:252–307、803–977，gc_state_loader.go与gc_state_lifecycle.go；
同时读取enabled_keyspace_index/loader、warmup/retry及相邻测试，区分48–51责任。
实际Go调用为server/gc_service.go的GetGCState，以及apiv2/handlers/safe_point.go。

验证：

- 可运行red 1项真实etcd：统一GC空间返回自身ID而非NullKeyspace，exit101；实现后通过。
- GC包8项实际测试全部通过。覆盖统一/独立/保留零ID/超范围别名/缺失/Archived、legacy内部限制，
  passive读不取缓存、active缓存/full读差异、换代清空及延迟旧cleanup，解析失败及修复、reserved/expired barrier。
- 实际TCP响应闸门持有已发生的etcd响应：等待者取消，另一个请求仍取得正确pair；
  代际替换取消并join旧读取，旧请求NotLeader，新代际重新读取正确pair。无mock存储/计数替代状态。
- 真实停止etcd后，统一GC暖缓存仍可读；键空间级暖点缓存仍先做metadata重定向，因此返回IO错误，
  full/冷读取均报错。同WAL恢复、fresh SDK和manager后原状态及Archived空间值恢复。
  初次错误地假设metadata也被缓存，测试失败；按Go实际调用链修正断言并保留失败记录。
- 全局+局部状态准备完成后，通过独立真实GC事务改变revision，最终读提交返回Conflict；
  新读取获得更新值。损坏global JSON不返回部分结果，普通读取不受global行影响。
- 同源Go Classic/NextGen各运行GetGCState、全局读取损坏无部分结果、leadership入场gate和旧cleanup隔离，
  各4个叶例通过；均不需要failpoint。依赖缓存、SDK、源码/模块只读sandbox与构建来源沿用39/41。
- 在线真实Go gRPC与Rust领域路径20组输入：null/独立/统一/Archived/超范围别名 × exclude × global include，
  完整state proto、local/global barrier及永不过期/过期TTL表达一致；gc_worker确实被排除。
  缺失空间的完整error文本一致。损坏txn的双方原始错误保留，比较uint syntax错误类别；
  Go带strconv/RFC上下文，既有Rust存储ParseUint为typed简短文本，不宣称两者字符串相等。
  修复后双方full读取取得2001，同WAL重启后的完整state一致。
  全部读取前后逐项比较metadata/name、两类safe point、local/global barrier及GC revision物理bytes/create/mod revision，双方各自完全不变。
- 扩大存储包回归首次103通过/1失败：endpoint_codec_test与region_encryption_test分别include同一夹具，
  模块内静态NEXT生成同PID相同目录，导致另例清理WAL。改为同一fixture别名，共享计数器；重跑全包通过。
  该问题不是GC事务失败；保留原始WAL错误，不用盲目重试隐藏。
- fmt、metadata、workspace check、keyspace整包107项及sa_002架构守卫通过；storage-endpoint整包104项通过。
  完整workspace test按外层24统一回归，0 doc-tests不计行为证据。
  架构守卫实际79包702生产源码、5负向控制通过。
  Cargo阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1，结束释放自有锁。
- 1101项Go/模块文件存在性、SHA与原始guard完全相同，未改Go、模块、failpoint、plan.md或来源史诗。

继续44事务安全点推进。
