# 任务 6：桶热度跨范围继承

实现前记录：源码基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，开始 HEAD
`86b67ac0fa7c15f0cc106f002209dfd3dfce5d65`。桶 Go 文件与基准无差异。
批次1–5已完成，见[任务5](task-05.md)及其依赖证据。已审查根规则、Copilot、
ADR、依赖/移植指南和两个只读计划；无更深规则。复用值对象、范围树及范围克隆。

## 实现前 Go oracle

只读命令：`GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics/buckets -run '^TestInherit$' -count=1 -v`。
退出0，1 passed，见[真实输出](task06/go-oracle.out)。选中测试仅调用既有
newTestBuckets→convertToBucketTreeItem→inherit，无I/O、生成、failpoint或后台任务。
TestMain仅goleak检查；初始化为阈值、日志和指标。生产core中注入点不在此调用链。
标准go test不执行generate，readonly/off阻止模块改写/下载，不需要failpoint注入。

真实运行共享断言契约：旧桶keys=["",20,50,""]，hot=[3,2,10]；五个新报告分别
["",20,30,40,50]→[3,2,2,2]、[20,45,50]→[2,2]、[00,05]→[3]、
[80,""]→[10]、["",""]→[10]。keys为ASCII字节，period=1000ms，六维flows=0，
region/version=1；每个报告转换后从0热度开始。此为已有Go测试实际执行断言，
不是Go逐字段打印快照，无随机/时间归一化。

源码补充契约（区别于运行差分）：有正热度取最大正值，否则取最负值，0不冲掉冷度；
保留新桶已有更热值，冷值不覆盖正值；仅修改hot_degree。origins及各自stats按范围
有序、不重叠，来源可Alive或Archive。flatten后双游标扫描；半开边界相邻不继承。
注意源码使用普通MinKey及字节比较：右界为空判作相交，游标也使用普通结束键比较，
且新item.end < origins[0].start直接返回（包括空结束键）。这些兼容特例不能替换为
理想化无穷比较。调用方checkBucketsFlow在查询重叠/同范围旧值后继承，再计算冷热；
本任务只实现继承，不提前引入下一批冷热计算或协议接线。

## 实现与红绿证据

新增 `BucketTreeItem::inherit`，显式注册独立测试；既有clone_by_range无需改动。
无新依赖、feature或manifest/lock变更。按有序来源迭代与新桶游标线性推进，
额外空间O(1)，不复制旧stats，不mock统计/范围索引。仅修改接收者热度；
独占借用维护内存安全，不增加协议、调度、全局状态、后台任务或存储。

最小可编译空实现后执行聚焦命令（先cargo fmt --all），[红灯](task06/red.out)
退出101，0 passed/5 failed/0 ignored：Go矩阵实际[0,0,0,0]期望[3,2,2,2]，
冷桶实际0期望-8，split实际[0,0]期望[-4,-4]，部分交集实际全0。
这是真实行为断言失败，不是缺符号或零测试。

5项测试覆盖Go全部5组键/热度输入与断言，以及多来源热/冷/零合并、已有新桶热度参与、
有限相邻不继承、前后无交集、部分交集、空来源/空stats、新旧字段不变、来源不变；
真实BucketRangeIndex的归档碎片→split→merge闭环检查最终索引和热度。
补充空结束键源码特例。[首次实现验证](task06/validation-initial.out)4 passed/1 failed：
测试错误地将有限结束键与旧空结束键视为无穷比较，期望[9,9,9]，实际[9,0,0]；
重新核对Go字节比较与游标分支后，只修正测试期望及注释，保留实现。
Rust helper另用version=7、loads=[region_id;6]检查非热度字段保留，继承不读取这些字段。
这些扩展断言明确属于源码契约，不声称Go TestInherit运行过未包含的输入。

## 构建边界

所有Cargo调用均经[槽位包装器](task06/slot.sh)原子mkdir动态领取，
实际槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录plan/task/session/PID；finally在正常、失败及信号退出时等待或终止并wait
本阶段子进程组后释放自己的owner/锁，保留缓存。未使用target-dir覆盖。
构建继承GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off。
重新审查proto/build.rs：只读固定模块ZIP，校验SHA256，生成在槽位OUT_DIR；
etcd_fixture_test.rs仅go build既有scripts/rust-etcd-fixture到槽位，fixture的WAL/ready
均写槽位，EOF关闭，Drop有wait及必要时kill/wait；无generate/failpoint。
本任务为纯内存继承，无需新增异步cancel/join或后端故障；workspace回归复用真实
MemoryStore/EtcdMetaStore，不把既有后端测试当作继承行为证据。

## 最终验证

[完整输出](task06/validation.out)、[验证脚本](task06/validate.sh)和
[metadata](task06/metadata.out)保存确切命令与退出码。

| 命令 | 退出码 / 有效结果 |
| --- | --- |
| `cargo fmt --all` | 0，无其它任务Rust文件差异 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics-buckets --locked --lib state_task_06_bucket_inherit -- --nocapture` | 0，5 passed/0 failed/0 ignored |
| `cargo test -p astersql-pd-statistics-buckets --locked` | 0，16 passed/0 failed/0 ignored |
| `cargo check --workspace --locked` | 0，仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0，999 passed/0 failed/1既有ignored |
| `python3 docs/rust/state/task06/readonly.py` | 0，1101个Go/模块文件集合、SHA256及Git状态不变，文档链接及diff检查通过 |

workspace既有ignored为tso_processes，进程环境不足时直接返回的既有健康测试也不计入
本任务证据；本任务5项聚焦测试均实际执行断言，没有忽略。
[调用链审查](task06/call-review.out)、[只读检查脚本](task06/readonly.py)、
[起点哈希](task06/go-before.json)、[核对输出](task06/readonly.out)、
[资源检查](task06/resources.out)保存最终核对证据。
所有Cargo及etcd子进程已退出，自己的锁已释放，缓存保留。没有新增Go源码或模块差异，
没有make、failpoint启停或生成工具。日志仅清理行尾空白及文件尾多余空行，不改变结果。
两个指定plan.md与来源史诗未修改，原有计划变更未纳入本任务提交。

状态：已完成，无待回归项。仅验收桶热度跨范围继承，不代表整个热点桶或统计主题完成。
