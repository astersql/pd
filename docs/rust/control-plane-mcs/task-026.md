# 任务026：有序键空间缓存及分割边界

状态：已完成，待统一工作区回归。2026-11-05已取得同源Go逐例JUnit，解除差分证据阻塞。
任务027可继续按真实直接依赖核验；任务102整体领域兼容门禁仍未完成。

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

## 历史差分缺口（已于2026-11-05解除）

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


## 2026-11-05：取得逐例回执并解除阻塞

本轮复用[原始CI分片](https://github.com/tikv/pd/actions/runs/35825512970/job/107074162700?pr=11210)
的 View raw logs，通过已登录Chrome保存完整文本。连接器仍返回Unknown tool，
但浏览器已可取得逐例JUnit，不再把“没有逐例日志”当成当前事实。
下载整次运行归档只包含其他分片，未采用该归档作为本任务通过证据。

CI实际checkout为 `24c9d275e3dddf3b5976208bf159a41ceb8b8591`（PR合并提交），
不是页面标题的 `247b117`。在内存读取该checkout的util.go、util_test.go、
codec.go、keyutil/util.go、keyutil/keyrange.go和keyspace常量；六文件SHA-256均与
当前仓库一致。没有保存新的Go文件，没有本地Go构建、测试、failpoint或模块写入。

原始JUnit的keyspace suite有31项、0失败；本任务只认证以下四项：

| 实际执行的Go测试 | JUnit耗时（秒） | 当前Rust对应验证 |
| --- | --- | --- |
| TestKeyspaceExists | 1.241651 | mcs_026_behavior：三种存活状态及tombstone |
| TestGetKeyspaceIDInRange | 1.226208 | ranges测试：完整条目、升序遍历、降序范围、limit、删除后可见性 |
| TestRegionSpansMultipleKeyspaces | 1.229742 | 14组相同边界输入和布尔断言 |
| TestGetKeyspaceSplitKeys | 1.335020 | 12组相同边界输入和分割键字节断言 |

这四项有独立testcase、非零耗时，无failure/error/skipped子节点；不是从CI总状态推断。
14/12组固定断言来自经过该次Go执行的同源测试；本轮运行实际Rust Cache并保存每组输出。
Go的all-exist checker在这些有限输入上由覆盖所用ID的真实Cache承接；不把这个有限范围
扩张为任意checker兼容。nil分割列表对应Rust空Vec，其余键字节和顺序不归一化。
缓存无持久化、网络或取消入口，所验证副作用为Save/Delete后的可见条目和遍历结果。
其余并发、未知状态、最大ID及top-ten等Rust测试属于附加验证，不冒称新增Go差分。
新增了原有ranges测试中遗漏的条目ID/state及删除后的升序扫描断言，无需改变生产实现。

持久原始日志、来源全文及摘要、Rust输入摘要、确切命令及完整输出保存在
[本轮证据包](task-026-resume-evidence.json.gz)；历史red/green保留于原证据包，
本轮没有人为破坏已正确的实现来制造red。

离线重验：`python3 scripts/verify-keyspace-cache-evidence.py --self-test`，退出0。
校验四项JUnit、实际checkout/Go版本、六份Go源码及45项Rust/Cargo输入摘要、
六项聚焦运行与14/12组完整输出；四个负向控制分别拒绝缺项、skipped、Rust输入漂移、
Rust执行失败。脚本不会运行Go、写Go文件或访问网络。

| 本轮命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 格式化，仅本任务测试产生差异 |
| cargo test --locked -p astersql-pd-keyspace --lib mcs_026_ -- --nocapture | 0 | 6通过、0失败、0忽略 |
| cargo test --locked -p astersql-pd-keyspace -p astersql-pd-keyutil -p astersql-pd-codec -p astersql-pd-schedule-labeler | 0 | 65通过：10+10+7+38；0 doctests不计 |
| cargo check --locked -p astersql-pd-keyspace | 0 | 通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过 |

聚焦6项包含在包级65项中，不重复累计。动态槽位1，
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`；
确切GOMODCACHE、PROTOC、官方etcd fixture环境保存于证据包。全部自有验证进程结束后
释放槽位并保留缓存。Go/模块1101项首尾存在性、SHA及Git状态一致，Go Git状态为空。
完整workspace test仍按既有统一回归安排留待实现批次收尾，本轮未执行，故保留任务文件
为“已完成，待统一工作区回归”；它不再阻塞后续领域任务的直接依赖核验。
