# 任务048：固定revision加载启用键空间

2026-11-05，基准02fa7d24d。[完整回执](task-048-evidence.json.gz)保存原始失败/通过日志、确切命令/退出码、源码摘要、Go原例和只读守卫。

canonical GC的EnabledSnapshotLoader接收宿主KeyspaceStorage及正并发数，首事务原子读取256条metadata和allocator hint。
RawTxnResponseItem现在保留真实range more；etcd与memory adapter均保留它，相邻契约验证限定读取和非范围项。
codec通过KeyspaceStorage.decode_persisted复用，GC不引用协议或具体etcd引擎。
分页始终固定首事务revision；allocator仅划分工作区间，缺失/损坏/越界hint不截断尾部。
每页验证完成后才向回调暴露独立值；只在所有workers成功并join后返回完整快照，取消/故障不返回部分map。
不建立第二套manager、不预先实现49的watch或50–52的后台预热/任期装配。

证据：

- 可运行red：真实258条metadata中257条启用，空快照触发0!=257断言，1 failed、exit101。一次格式解析错误单独保留，不计行为red。
- `cargo test --locked -p astersql-pd-gc --lib mcs_048_ -- --nocapture`：4 passed、0 failed、0 ignored，exit0。
  覆盖空/256/跨页、状态/管理类型过滤，缺失/短/零/有效/最大/越界allocator hint及稀疏尾部，protobuf损坏、metadata ID错配、非法key。
- 真实TCP闸门持有首事务已发生的响应，随后删除257、插入300；初始快照仍为首revision的257条，下一次读取才反映替换。
  同WAL停止/重启后fresh SDK取得完整同值快照。取消真实响应等待，以及首事务后真实compact，均丢弃整个尝试；后续干净读取成功。
- 同源Go Classic与NextGen运行9个顶层原例，各31个pass节点；叶例数量及名称见回执。
  使用现有已核验gc测试二进制，sandbox禁止写仓库及源码/模块缓存；无failpoint注入或Go源码/模块变更。
  固定revision场景与Rust同为IDs1..257、首事务后delete257/insert300 unified，双方原例断言相同初始及更新集合。
  Go原例通过断言证明内部输出；不声称Go打印了内部snapshot或提供了额外RPC。
- GC整包45 passed、0 failed、0 ignored；raw_range_两后端2 passed，raw_get_两后端2 passed，gc_batch_read_response_validation 1 passed。
- metadata、fmt check、workspace check和git diff --check通过。完整workspace test仍由外层任务24集中执行。
- 每个Cargo阶段原子领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；所有子进程退出后释放自有锁，缓存保留。
- Go/模块1101项首尾存在性及SHA一致；plan.md和来源史诗不变。未使用make、生成/整理或failpoint注入命令。
