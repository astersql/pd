# 任务21：热点副本继承与主节点变化

依赖批次20完成证据见task-20.md，累计依赖均已完成。开始HEAD
`826eb7965599c8a527fad7c712fd57c223e36e84`；Go参考基准
`69ba16e6d47dfe59a21498c21641e1c1eaa726e0`。
已读两份只读计划、AGENTS、Copilot、ADR、依赖与移植指南；无更深AGENTS。

## 实现前 oracle

只读审查hot_peer_cache.go的查找、更新、过期收集、leader变化和oldColdPeer分支，
对应既有测试及region构造、schedule/checkAndUpdate、TestMain/goleak调用链。
这些路径无failpoint需求，无生成/格式化/模块写入。
运行：`GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics -run '^(TestFindOldHotPeerStatForInheritance|TestCacheInherit|TestCoolDownTransferLeader)$' -count=1 -v`。
退出0，3个顶层测试、5个查找子测试通过，见[实际输出](task21/go-oracle.out)。
输出是原断言运行证据，不是新增逐事件JSON差分。

运行覆盖输入/断言：region100，旧store1可继承时新store2取旧store；禁用旧store1后
寻找可继承store2；最终direct命中/最终miss与重复store去重。读方向3副本暖机200次，
10秒报告，增加store10/移除副本，后续流量乘10再次暖机并替换；返回瞬时byte流量
分别等于旧/新累计bytes除以10秒。leader迁移才触发cooldown；移动/增加/移除follower不触发。

补充源码契约（不冒充运行快照）：先扫描缓存旧stores，再按新region peer顺序去重查找；
无allowInherited候选时保留最后一次查找结果，source=Direct。Go map遍历无固定顺序，
Rust采用稳定store顺序，不承诺候选内容不同情况下与Go随机选择同一store。
继承clone保留滚动窗口/累计秒、重置pending delta；未满窗口保留degree/anti且禁止再次继承，
满窗口冷热判断恢复继承资格。曾在旧副本集合但已不在缓存的cold peer重新进入时重新初始化或移除。
leader变化由整个region旧leader识别，所有peer记录宿主时钟；write跳过首个变化报告，
read正常计入。下一报告保留transfer时间；cooldown边界严格小于degree*reportInterval。
宿主显式纳秒时钟，不创建全局时钟或调度操作。窗口单位纳秒，输入delta为累计量，interval为秒。

## Rust实现、失败与通过证据

先注册hot_inherit/hot_inherit_test及最小可编译方法，再执行
`bash docs/rust/state/task21/slot.sh bash -c 'cargo fmt --all && cargo test -p astersql-pd-statistics --locked --lib state_task_21_hot_inherit -- --nocapture'`。
[红灯](task21/red.out)退出101，5 failed、0 passed/ignored，包含inherit=false、
expired长度0而预期1、transfer=None而预期Some(1000000000000)的真实断言失败；
partial clone因缺少继承返回空列表而越界。另一个新flow测试550/1000失败中，发现测试夹具
未移除上一轮store10，已修正为暖机前消费真实expired动作，避免将夹具问题当产品缺陷。
同一命令的[首轮绿灯](task21/green.out)退出0，5 passed、0 failed/ignored。
随后新增2个边界测试，最终7个聚焦测试全通过。

[生产模块](../../../pkg/statistics/hot_inherit.rs)公开
HotPeerCache::find_old_hot_peer_for_inheritance、collect_expired_items；
[接线](../../../pkg/statistics/hot_peer_update.rs)增加check_peer_flow_at，处理真实窗口克隆、
leader首报和cold peer重入；[动态入口](../../../pkg/statistics/hot_threshold.rs)传递宿主now。
[独立测试](../../../pkg/statistics/hot_inherit_test.rs)在lib.rs显式注册。
未新增依赖、feature或Cargo.lock变化，不依赖protobuf、调度或私有运行时。

有效行为证据包括：同region多store替换保留median100/degree、4秒继承窗口独立，
4+6秒满窗恢复继承；partial继承重置pending delta使degree变0、anti变11；
read迁移首报冷降1，write不降且保留4秒窗口，下一56秒热报正常degree+1；
全部store记录同一宿主transfer时间，严格冷却边界和后续时间保留；
冷peer重入热则1/12、冷则Remove；增删follower不触发冷却；丢弃动作不改变缓存leader/时间。
稳定流与10倍流暖机后的继承median分别100/1000，瞬时loads与Go原测试同方向预期一致。
这些Rust断言对照上述实际运行Go测试及明确标注的源码契约，不声称新建逐字段Go JSON oracle。

## 验证与资源

最终入口：`bash docs/rust/state/task21/slot.sh bash docs/rust/state/task21/validate.sh`。
[脚本](task21/validate.sh)、[完整日志](task21/validation.out)、[metadata](task21/metadata.out)。
所有Cargo调用通过[槽位包装器](task21/slot.sh)原子mkdir领取槽位，记录计划/任务/会话/PID，
正常/失败/中断均先等待或终止并wait子进程再删除自有owner和锁；缓存保留。
红灯、首轮绿灯、最终回归均实际领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

已审查pkg/storage/kv/etcd_fixture_test.rs、scripts/rust-etcd-fixture/main.go与pkg/proto/build.rs。
workspace回归继承GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off；
Etcd夹具仅构建现有Go源码，EOF关闭/kill并wait，数据与产物在槽位内。
proto只读下载指定版本ZIP并验证SHA256，仅OUT_DIR生成Rust，不写Go/模块。
本行为本身同步内存无后台资源或持久化，不新增MetaStore接口；workspace覆盖真实Memory/Etcd。

| 最终命令 | 退出码 / 有效测试数量 |
| --- | --- |
| 上述只读Go oracle | 0；3个顶层测试、5个查找子测试 |
| `cargo fmt --all` | 0；无范围外格式化改动 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_21_hot_inherit -- --nocapture` | 0；7 passed，0 failed/ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0；68 passed，0 failed/ignored |
| `cargo check --workspace --locked` | 0；仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0；1083 passed，0 failed，1既有ignored |
| `python3 docs/rust/state/task21/readonly.py` | 0；Go/模块/计划哈希、链接及diff检查通过 |

[逐命令汇总](task21/summary.json)由日志解析，既有ignored为tso_processes，不计本行为验收。
[资源退出检查](task21/resources.out)确认自有锁释放、无槽位产物进程、缓存保留。
[只读检查](task21/readonly.out)确认1101个Go/模块文件集合、内容SHA256及Git状态与起点一致；
[起点哈希](task21/go-before.json)、[起点Git状态](task21/go-status-before.out)。
aggregate SHA256=`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
两份只读plan.md与[起点哈希](task21/readonly-plans.json)一致，用户已有变更保留。

状态：已完成；仅验收本继承与leader变化行为，不表示整个统计主题完成。
