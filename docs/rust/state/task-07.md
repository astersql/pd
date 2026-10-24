# 任务7：桶冷热判定与查询

实现前记录：基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，起点HEAD
`14908553d15b7271b255fea40c98a4d624a11f53`。桶Go源码与基准无差异。
批次1–6已验收，见[任务6](task-06.md)及其依赖记录；复用报告值、范围索引和继承。
已读根规则、Copilot、ADR、依赖基线、移植指南和两个只读计划；无更深规则。

## 实现前Go运行证据

`GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics/buckets -run '^(TestCalculateHotDegree|TestColdHot|TestCollectBucketStatsTask)$' -count=1 -v`
退出0，3 passed，见[输出](task07/go-oracle.out)。调用链：既有newTestBuckets→convert、
calculate；NewBucketsCache→schedule→checkBucketsFlow→inherit/calculate→putItem，以及
collect→GetHotBucketStats。均无failpoint/生成/源码写入，TestMain仅goleak，测试defer取消
队列且goleak通过；相关core注入点不在调用链。标准go test不执行generate，readonly/off
阻止模块写入和下载。没有新增或改写Go文件。

真实执行断言（不是未打印的字段快照）：TestCalculateHotDegree以region=1/version=1、
ASCII keys=[010,100]、period=1000ms、六维flow=0开始，calculate后-1；读byte/key
阈值+1后0；写byte/key阈值+1后1。TestColdHot以region1[10,20] flow=0和
region2[20,30] flow=MaxUint64，各重复报告20次，分别断言-1…-20和1…20。
Go归一化为uint64 wrapping flow*1000/1000，MaxUint64仍超过阈值。
TestCollectBucketStatsTask直接插入10个degree=0区域，id=0…9、ASCII十进制
keys=[i*10,(i+1)*10]，min=-100返回10个region，min=1返回空。无随机/时间归一化。

源码补充契约：仅loads[0,1,3,4]任一严格大于阈值才热，QPS不参与；阈值复用
canonical statistics-utils 的MIN_HOT_THRESHOLDS（8192/128/1024/32）。
热时仅degree<20才+1，冷时仅degree>-20才-1，不做无条件clamp；冷热转换逐次跨0。
查询为degree>=min，空region筛选表示全部，只查询当前region映射，忽略未知id，
无符合stat不返回region；重复id不复制stat，桶顺序保留。Rust按任务要求返回深复制快照。
这些额外边界将以源码契约测试验证，不声称Go既有测试执行过全部额外输入。

## 实现与红绿验证

新增 `BucketTreeItem::calculate_hot_degree`、`BucketRangeIndex::get_hot_bucket_stats`
和两个边界常量。range模块仅增加crate内只读iterator供全region筛选。
复用canonical statistics-utils常量；manifest/lock只增加此已存在本地依赖，无新版本或feature。
测试通过真实报告归一化、继承和范围索引组合每次报告，不mock统计结果。

最小可编译空实现后先fmt，再执行聚焦命令：[红灯](task07/red.out)退出101，
0 passed/5 failed/0 ignored，热度实际0期望-1、查询实际0个region期望10，
其它失败为快照空与重复报告无结果。这是行为断言失败，不是缺符号或零测试。
实现后[绿灯](task07/green.out)退出0，5 passed/0 failed/0 ignored。
绿灯阶段先 `cargo update -p astersql-pd-statistics-buckets --offline`（退出0）
维护本地依赖锁，再fmt并使用--locked。

5项测试逐一复现Go三项测试的输入/断言，另覆盖四维各自严格阈值、QPS不参与、
热/冷25次保持边界、反向45次跨零至另一边界、越界i32极值条件步进且无溢出、
空stats、无匹配/未知/重复region筛选、包含相等degree、桶内顺序、全字段与深复制隔离，
以及覆盖产生Archive碎片后查询仅包含current region。查询快照在后续更新后保持原值。
额外输入明确属于源码契约测试，Go实际执行证据仅主张前述既有断言。

本任务纯内存无后台任务/存储依赖，不新增cancel/join或后端故障路径。
基线回归复用真实MemoryStore与EtcdMetaStore，不能代替本任务行为测试。
重新审查proto/build.rs（固定模块ZIP校验、仅槽位OUT_DIR生成）、
etcd_fixture_test.rs与既有scripts/rust-etcd-fixture/main.go调用链：
Go build仅输出既有fixture到槽位，WAL/ready/log均写槽位，无generate/failpoint；
fixture EOF关闭、Drop等待并必要时kill/wait。Cargo继承GOARCH=arm64、CGO_ENABLED=0、
GOFLAGS=-mod=readonly、GOPROXY=off，Go/模块最终哈希核对覆盖整个阶段。

## 最终验收

所有Cargo阶段通过[槽位包装器](task07/slot.sh)动态原子mkdir领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁记录plan/task/session/PID；finally处理正常、失败和信号退出，等待或终止并wait
本阶段命令组后释放自己的owner与锁；未绕过target-dir，保留缓存。

[验证脚本](task07/validate.sh)、[完整输出](task07/validation.out)和
[metadata](task07/metadata.out)记录确切命令/退出码。

| 命令 | 退出码 / 有效结果 |
| --- | --- |
| `cargo fmt --all` | 0，仅本任务Rust差异 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics-buckets --locked --lib state_task_07_bucket_degree -- --nocapture` | 0，5 passed/0 failed/0 ignored |
| `cargo test -p astersql-pd-statistics-buckets --locked` | 0，21 passed/0 failed/0 ignored |
| `cargo check --workspace --locked` | 0，仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0，1004 passed/0 failed/1既有ignored |
| `python3 docs/rust/state/task07/readonly.py` | 0，1101个Go/模块文件集合、SHA256与Git状态相同，两指定plan.md哈希相同，链接及diff检查通过 |

workspace唯一ignored为既有tso_processes；既有健康测试在环境不足时提前返回，
不将其计入本任务行为证据。5项聚焦测试均执行真实断言，无忽略。
[只读脚本](task07/readonly.py)、[起点哈希](task07/go-before.json)、
[核对输出](task07/readonly.out)及[资源证据](task07/resources.out)持久保存。
全部Cargo和槽位产物进程已退出，自己的锁已释放，缓存保留；无新增Go文件、
无Go/模块差异、无make/failpoint/生成命令。日志仅清理行尾空白及尾部多余空行。
两个指定plan.md与来源史诗未修改；用户既有变更未纳入提交。

状态：已完成，无待回归项。仅验收桶冷热判定与查询，不代表整个统计主题完成。
