# 任务026：有序键空间缓存及分割边界

状态：已阻塞。Rust实现、聚焦及相邻测试通过；必需Go逐用例运行证据缺失。
任务102未完成，未推进任务027。

实际实现RwLock/BTreeMap共享缓存，Save完整替换name/state，Get返回拥有的快照；
删除立即可见。Tombstone不参与存在性及降序范围，但仍保留在升序遍历中。
范围[start,end]包含端点，降序、正limit截断、非正limit不限量，反向范围为空。

跨空间判断保留Go早退顺序：无法识别的边界保守处理，raw→txn必跨模式，
精确右边界不跨空间，无穷上界按起点存在性判断，最大txn的相同ID早退优先；
中间存在的空间不会改变仅检查端点的结果。分割按当前模式取最高十个有效ID，
复用keyutil严格内部边界、codec的最大ID fencepost，再排序去重。没有私有ID计数器、
后台任务、protobuf或具体存储依赖。

## 当前验证

[所有命令、退出、原始日志、来源SHA及证据搜索记录](task-026-evidence.json.gz)。
编译后的red在Save后实际存在性仍不可见的断言失败；当前Rust验证全部通过。

| 命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-keyspace | 0 | 10通过：6本项+4边界相邻 |
| cargo test --locked -p astersql-pd-keyutil | 0 | 10相邻通过 |
| cargo test --locked -p astersql-pd-codec | 0 | 7相邻通过 |
| cargo test --locked -p astersql-pd-schedule-labeler | 0 | 38相邻通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 78 packages |

去重6聚焦+59相邻有效Rust测试通过；其中一项重放Go既有14组span及12组split固定断言，
使用实际Rust Cache而非调用计数替身；另含四线程实际并发一致性。
这些固定期望来自Go测试源码，未冒称当前Go执行输出。0 doctests不计行为通过。
动态槽位1，绝对CARGO_TARGET_DIR为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`；自有进程退出释放锁，保留缓存。
Go及模块1101项存在性、SHA及Git状态一致。全兼容包及workspace测试依用户授权统一执行。

## 必需差分缺口

Go生产仓库当前没有GetKeyspaceSplitKeys/RegionSpansMultipleKeyspaces的生产调用方，
现有PD公开协议不能直接执行它们。只读扫描本地49500个缓存文件、临时产物及历史路径，
未找到相应既有Go测试二进制/测试对象。没有运行会生成_testmain.go或需要failpoint
注入的Go测试构建，保持生成文件和临时Go工具同样只读。

找到[上游Unit Test(1)历史成功回执](https://github.com/tikv/pd/actions/runs/35825512970/job/107074162700?pr=11210)
及另外两个成功分片。CI提交247b117与源码提交902ff4f的util.go/util_test.go均与本机SHA256
完全一致。工作流分片1包含pkg/keyspace属于静态覆盖推断；公开页面只给汇总状态，
逐用例日志要求登录，因此未把该汇总推断当作所需的逐例运行差分。

GitHub连接器相关接口返回Unknown tool；本机gh不存在；公开API限流，未取得标准
非交互GitHub HTTPS凭据。声明的Chrome技能文件在路径和备用技能目录均缺失，
没有执行未经技能指导的浏览器操作。已向用户询问现有产物路径。

解除条件：提供本机可执行的既有pkg/keyspace测试二进制，或同源逐例运行日志/JUnit，
能核验TestKeyspaceExists、TestGetKeyspaceIDInRange、TestRegionSpansMultipleKeyspaces、
TestGetKeyspaceSplitKeys的实际执行与断言结果。核验来源后补差分证据并重新准入下一任务。
