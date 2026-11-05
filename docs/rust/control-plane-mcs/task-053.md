# 任务053：枚举GC状态并导出屏障指标

2026-11-06，基准1aecad976。[完整回执](task-053-evidence.json.gz)保存真实red/green、全部命令及退出码、双内核Go原例、在线列表差分、指纹和Go守卫。

列表复用同一GC manager、MetaStore、keyspace codec和代际缓存。排除屏障时克隆调用开始后的缓存，扫描100条一页的启用metadata，统一GC空间返回自身ID的零状态，Null单列；不排除屏障时重新读事务。移除失效缓存，换代取消中止整次枚举。两种选项使用独立有序singleflight，新调用不复用开始前的执行，最后等待者取消终止读取。最大合法ID后的扫描错误保留Go行为。现代列表RPC由外层任务8负责，本任务只提供真实领域入口。

宿主Registry注册一个弱引用转发collector，GC term绑定该句柄；失败注册回滚，不使用全局registry。成功推进事务安全点才发现超过24小时的有效屏障，本地gc_worker除外，全局同名保留。名称刷新保持身份和10分钟告警间隔；写入只更新已发现样本，删除、到期、空间移除和任期退役清理样本。epoch阻止删除之前的观察复活标签；旧cleanup不能卸载新任期。采集只快照内存，释放锁后交给Prometheus，无存储I/O。cache计数器预绑定hit/slow_hit/miss；Null兼容回退GC更新的gauge保留Go请求目标值语义。

验证：

- 首个列表red返回空集合，257项真实metadata断言失败；指标red已真实写屏障并推进事务，但采集为0而非1。分别修复后通过。编译问题及测试预期错误单独保留，不算功能red。
- `cargo test --locked -p astersql-pd-gc --lib mcs_053_ -- --nocapture`：8 passed、0 ignored。覆盖两页metadata及Null、缓存与新鲜读取、失效缓存剔除、损坏持久值报错、调用取消、真实响应闸门与CAS冲突后读取新值、年龄与告警/名称/到期策略、真实生产写入/删除/空间移除/换代、缓存计数与兼容gauge。
- 失败事务由真实etcd revision竞争触发；样本保持，真实停服etcd后仍可采集。响应闸门实验旧读取因真实并发提交返回Conflict，后发调用读新值60，最后等待者取消后shutdown成功join。
- `cargo test --locked -p astersql-pd-runtime --lib mcs_053_ -- --nocapture`：1 passed。实际节点宿主任期manager产生屏障，通过TCP HTTP `/metrics`核对导出，停服后样本清除，即使外部仍保留manager。
- `cargo test --locked -p astersql-pd-compat-tests --lib mcs_053_list_go_differential -- --ignored --nocapture`：1 passed。124项metadata跨页，归档过滤、统一GC零状态、Null及无穷屏障，在三轮true/false/true中逐字段对比Go RPC与Rust领域输出；两侧读取前后bytes/create/mod revision不变。只排序无序列表，不归一化状态值。
- Go原有8个屏障指标测试在Classic和NextGen各通过（各14个含子例节点）；来源metrics_test.go，无failpoint依赖，使用已核验现有二进制与只读sandbox。原例覆盖同一年龄/续租/名称/注册换代/并发/告警/采集释放锁语义，原始日志保留；不将源码推导冒充运行输出。
- GC整包71 passed、0 failed、0 ignored；宿主注册回滚、metadata、fmt check、workspace check通过；完整workspace test按外层24集中执行。
- 所有Cargo阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；子进程退出后释放锁，保留缓存。
- Go及模块1101项存在性和SHA首尾一致；未运行make、failpoint注入、生成/整理或Go写入。所有plan.md和来源史诗只读。
