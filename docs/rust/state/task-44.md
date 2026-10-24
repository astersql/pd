# 任务44：分裂合并覆盖清理

依赖task-43.md已完成，累计前置沿该记录。已读两份只读plan、根规则、Copilot、ADR、移植指南、依赖基线，无更深AGENTS。
起点commit与Go/模块逐项SHA256见[before.json](task44/before.json)。
Go基准69ba16e6d47dfe59a21498c21641e1c1eaa726e0。

## 实施前oracle

复用任务41既有Linux/ARM64 cluster-oracle41.test，SHA256
8781f08225e2b8b746217d79fe78e3ff072369cc8ae8941cc8d616a8a77986d9。
审查TestMain/newTestRaftCluster/InitCluster/内存storage和同步runner调用链；
选择的三个测试不依赖failpoint，无Go/模块/生成写入。通过[oracle.sh](task44/oracle.sh)
只读挂载运行：[oracle.out](task44/oracle.out)，TestHeartbeatSplit、TestRegionSplitAndMerge、
TestOfflineAndMerge全部通过，退出0，先于实现。Docker --rm退出回收进程。

运行序列来自cluster_test.go:1613–1762：全范围1；先报告左半2后报告右半1；
再次先报告缩小右半1再报告中段3；未到达的半区为空洞。SplitAndMerge执行7轮拆分、7轮合并、
14轮混合。OfflineAndMerge以store1 Removing、三副本region开始，128片逐层合并至1；
每步OfflinePeer条目数等于存活region数。

源码补充契约（不冒充运行差分）：cluster.go:1406先提交core再HandleOverlaps；
pkg/cluster/cluster.go:45立即clear_defunct_region，标签mark_defunct_region延后回收。
该函数没有删除热点：旧ID热点保留至正常cold/TTL，新报告同ID副本变化由Expire处理，
继承检查先于Remove应用。规则/affinity缓存属于各自领域与宿主接线，不在本统计清理函数内。
核心epoch和跨store索引复用既有core，size为MiB、keys为计数、clock为纳秒（热点）/秒（分类）。

热点必要运行oracle复用已验收的[task-22.md](task-22.md)与
[原始输出](task22/go-oracle.out)：TestRemoveExpireItems/RemoveFromCache/RemoveFromCacheRandom
三项通过，确认消失region由既有TTL和冷却路径清空。本任务没有重做热点算法；
Rust验证overlap清理不提前破坏双向热点历史，99ns保留、100ns到期清空。

## 红灯与实现

[red.out](task44/red.out)实际执行4项，0通过/4失败/0忽略、退出101：
覆盖后旧ID仍is_observed、OfflinePeer仍为[1,2]而非[2]、延后标签清理后计数2而非1。
初次测试使用错误字段region_count造成编译失败，见[red-compile.out](task44/red-compile.out)，
修正为现有region字段后取得上述真实红灯；不把编译错误计为行为失败。

新增region_overlap.rs::handle_region_heartbeat_with_overlap_cleanup：真实心跳提交成功后，
消费真实overlaps清除可选RegionStatistics、标记LabelStatistics；错误在副作用之前返回。
原入口仍可供不持有统计组件的宿主使用，宿主自行选择组合入口。
不主动清热点、不重新生成overlap列表，不引入调度、协议或MetaStore反向依赖。
只有测试新增已有workspace schedule-config依赖，Cargo.lock对应增加一条既有crate边。
调用方持有独占core/statistics状态；该同步函数不新增后台任务。

第五项测试复用core拆分/合并helper执行7轮拆分、7轮合并和14轮混合，
每轮核对真实OfflinePeer数、标签清理后计数及store1/2/3角色索引。
[初轮输出](task44/green-initial.out)4通过/1失败：MergeRegions与Go一致返回未分类literal，
不能把它直接作为真实报告来断言store角色计数；测试在提交前用clone_with([])重建分类，
保留helper原有ID/range/epoch，并按Go的每三步第三步合并纠正混合序列。
这属于输入构造修正，没有放松索引断言或修改依赖实现。

## 最终验证与退出

入口：`bash docs/rust/state/task44/slot.sh python3 docs/rust/state/task44/validate.py`。
[逐命令统计](task44/summary.json)、[阶段输出](task44/validation.out)及validation-0.out至
validation-6.out保留结果；输出仅统一制表符与行尾空白。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 无范围外Rust差异 |
| `cargo test -p astersql-pd-cluster --locked --lib state_task_44_region_overlap -- --nocapture` | 0 | 5通过/0失败/0忽略 |
| `cargo test -p astersql-pd-cluster --locked` | 0 | 26通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 成功 |
| `cargo fmt --all -- --check` | 0 | 成功 |
| `cargo check --workspace --locked` | 0 | 成功 |
| `cargo test --workspace --locked` | 0 | 1250通过/0失败/1既有忽略 |

既有忽略为tso_processes，本任务无忽略。workspace验证真实MemoryStore与EtcdMetaStore契约；
本函数不含持久化或监听，不把通用后端验证当作本函数持久化能力。
审查etcd_fixture_test.rs/scripts/rust-etcd-fixture/main.go：只构建现存Go夹具，
GOFLAGS=-mod=readonly，不需failpoint；EOF关闭、wait回收，失败路径kill后wait。
proto/build.rs校验固定kvproto ZIP后仅向OUT_DIR解压和生成Rust，不写Go源码/模块。
未运行make/failpoint/Go生成或格式化命令。

完整基线之后仅加强测试断言：左半先到后[m,∞)空洞，右半先缩小后[m,q)空洞，
后续报告恢复完整覆盖；多轮序列无空洞且root数量等于存活数。
生产实现未再改动。补充命令：
`bash docs/rust/state/task44/slot.sh sh -c 'cargo fmt --all && cargo test -p astersql-pd-cluster --locked --lib state_task_44_region_overlap -- --nocapture && cargo test -p astersql-pd-cluster --locked && cargo fmt --all -- --check'`，
[final-range-check.out](task44/final-range-check.out)退出0，聚焦5通过、包级26通过，无失败/忽略。

锁文件更新阶段使用`cargo check -p astersql-pd-cluster --offline`，退出0，见
[lock.out](task44/lock.out)；仅增加测试对已有workspace crate的依赖边。
全部Cargo、oracle和补充验证均动态领取槽位2：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
[资源核验](task44/resources.json)：自有槽位产物进程全部结束，自有锁已释放，缓存保留，
未操作他人槽位1。无新后台任务或持久化；函数内不需要新增cancel/join。

[只读核验](task44/readonly.json)：1101项Go/模块清单、内容哈希与Git状态等于起点，
聚合SHA256为ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4，
无新增Go/模块文件；两份指定plan.md哈希未变。相关Go参考与基准commit无差异。
`git diff --check`通过，文档相对链接均可解析，用户已有计划变更保留。

状态：已完成。范围仅为统计overlap应用，非整个主题或完整心跳副作用迁移完成。
