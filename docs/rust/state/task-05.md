# 任务 5：桶范围覆盖与碎片

实现前记录：源码基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，开始 HEAD
`332454ff30add4145fe64d3d60b0001dca55ed54`；桶 Go 源码与基准无差异。
批次1–4完成证据见 [任务4](task-04.md) 及其依赖。已读根规则、Copilot、ADR、
移植/依赖指南、两个只读计划；pkg/docs/tests无更深AGENTS.md。
复用canonical rangetree/keyutil和批次4值对象，不引入协议、后台任务或存储依赖。

## 实现前 Go oracle

命令：`GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics/buckets -run '^(TestPutItem|TestGetBucketsByKeyRange|TestBucketTreeItemClone)$' -count=1 -v`。
退出0，3 passed，见 [真实输出](task05/go-oracle.out)。复用既有源码/测试，未新增Go。
审查 hot_bucket_cache.go、bucket_stat_informer.go、hot_bucket_task.go、对应测试及
rangetree/range_tree.go：put/get/debris/clone均无failpoint、生成或I/O。
TestMain只做goleak检查；NewBucketsCache的队列在ctx取消后退出，goleak验证收敛。
TestGetBucketsByKeyRange会调用checkBucketsFlow→inherit/calculateHotDegree，均无注入。
core仅UpdateBuckets/UpdateSubTree存在注入，所选调用链不经过它们；初始化仅阈值、日志、指标。
此具体测试入口无需failpoint；标准go test不运行generate，readonly/off阻止模块改写与下载。

真实运行断言契约（共享输入/断言，不声称Go输出过未打印的字段级快照）：
TestPutItem按序插入 `(id,version,keys)`：`(1,0,["",""])`、
`(1,2,[10,20,30])`、`(2,0,[15,20])`、`(1,3,[20,""])`、
`(3,0,["",20])`、`(4,0,["",""])`。各步region/tree数量分别
`1/1,1/3,1/5,2/4,2/2,1/1`；每步新region可查，查询[10,∞)非空。
所有keys是上述文字的ASCII字节，周期1000ms，六维flow=10，版本不决定范围相等。
TestGetBucketsByKeyRange插入 `["",015),[015,020),[020,∞)` 三项；
查询 `["",100),[030,100),[010,030),[015,020),[001,∞)` 数量为3/1/3/1/3。
TestBucketTreeItemClone原桶[010,020),[020,100)，六组边界及数量在Rust复现。
没有随机/时间字段归一化。

源码补充契约：范围查询半开，空结束为∞；双侧碎片Archive，不进region索引；
只删除被覆盖Alive项的region索引，覆盖Archive不得误删同region的新Alive项。
同region同边界直接替换全部字段，不改变树分片；Rust以独占借用更新两个自有快照索引。
clone逐stat使用普通max_key/min_key，仅当新end为空时取stat.end，严格left<right。
因此Go的空结束stat可能不进入碎片，即使范围树仍保留该碎片；保持此现有行为。
完整覆盖无穷项的工厂可能生成["",""]，canonical RangeTree过滤此无效碎片。
以上字段、裁剪和独立快照断言来自源码，区别于上面的已运行Go断言。

## 实现与测试驱动证据

新增 [bucket_range.rs](../../../pkg/statistics/buckets/bucket_range.rs) 的
`BucketRangeIndex`、`bucket_debris_factory`、`BucketTreeItem::clone_by_range` 和
`RangeItem`实现；入口显式注册独立测试。只扩展既有canonical crate，新增生产依赖为
workspace已锁定的rangetree/keyutil；Cargo.lock只增加这两条本地依赖，无版本变更。
同范围路径同步替换两个自有值，旧查询快照保持独立；不同范围路径由真实RangeTree
计算重叠和碎片，再维护region索引。没有mock范围/索引结果，无新增features。

最小可编译接口返回空索引/默认克隆后运行
`cargo test -p astersql-pd-statistics-buckets --locked --lib state_task_05_bucket_range -- --nocapture`：
[红灯](task05/red.out)退出101，0 passed/5 failed/0 ignored，实际region数量0期望1、
查询ID空期望[1,2,3]、碎片数量0期望2、克隆Alive期望Archive。
完整实现后[绿灯](task05/green.out)退出0，聚焦5 passed，包级11 passed。
没有以编译失败、零测试或人为破坏正确实现充当红灯。

5项测试包含Go原始6步put计数/5组query断言和6组clone裁剪，并增加源码契约验证：
相邻不重叠、边界相等查询、有限范围外空查询、左右碎片完整字段/热度、归档覆盖时
同region新报告保留、同边界版本/桶数/热度替换且旧快照不变、全覆盖和重复无穷范围、
无穷末桶的源码裁剪特例、深复制负载/键。公共查询不暴露可变内部索引。
这些额外字段断言明确属于源码推导，Go运行证据只主张既有测试实际检查的内容。
本行为无错误返回/持久化/后台任务，不要求新增cancel/join或后端故障场景。

## 槽位和基线边界

全部Cargo阶段通过[槽位包装器](task05/slot.sh)原子mkdir动态领取槽位1；
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
记录计划/任务/会话/PID；finally处理信号/失败，先终止并wait本阶段进程组，再释放自己
的owner与锁，保留缓存。没有额外target目录或target-dir覆盖。
构建继承GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off。
再次审查proto/build.rs，仅读取固定模块ZIP并在槽位OUT_DIR生成；
etcd_fixture_test.rs仅build既有Go fixture到槽位，写入WAL/ready等均位于槽位，
EOF请求关闭并wait、必要时kill/wait。无需generate或failpoint，不改Go源码/模块。
完整workspace验证使用既有真实MemoryStore/EtcdMetaStore，未新增平行存储实现。

## 最终验证

[验证脚本](task05/validate.sh)、[完整输出](task05/validation.out)、
[metadata](task05/metadata.out)保存确切命令及退出码。

| 命令 | 退出码 / 有效结果 |
| --- | --- |
| `cargo fmt --all` | 0，无其它任务Rust文件差异 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics-buckets --locked --lib state_task_05_bucket_range -- --nocapture` | 0，5 passed/0 failed/0 ignored |
| `cargo test -p astersql-pd-statistics-buckets --locked` | 0，11 passed/0 failed/0 ignored |
| `cargo check --workspace --locked` | 0，仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0，994 passed/0 failed/1既有ignored |
| `python3 docs/rust/state/task05/readonly.py` | 0，1101个Go/模块文件集合、SHA256及Git状态不变，文档链接/diff通过 |

workspace既有ignored为tso_processes；进程环境不足时提前返回的既有健康测试也不计入
本任务证据。聚焦5项均真实执行行为断言、没有忽略。无额外Go fixture或调试容器。
[调用链审查](task05/call-review.out)、[起点哈希](task05/go-before.json)、
[只读输出](task05/readonly.out)、[资源输出](task05/resources.out)保存核对记录。
本阶段Cargo/etcd子进程均已退出，自己的槽位1锁已释放，缓存保留。
所有日志仅清理行尾空白及文件尾多余空行，不改变结果。
两个指定plan.md和来源史诗未修改，用户原有计划变更未纳入提交。

状态：已完成。本证据仅验收桶范围覆盖与碎片，无待回归项，不代表整个热点桶或统计主题完成。
