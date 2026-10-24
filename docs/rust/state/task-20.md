# 任务20：热点动态阈值

依赖任务19及累计1–18的完成证据已核对；复用真实HotPeerCache、HotPeerStat、DimStat和TopN。
开始HEAD见最终补充。Go基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，参考生产/测试与基准无差异。
已读两份只读计划、根AGENTS、Copilot、Rust ADR/依赖/移植指南；目标子树无更深AGENTS。

## 实现前Go oracle与契约

只读审查hot_peer_cache.go的calcHotThresholds、UpdateStat、CheckPeerFlow及相邻调用，
hot_peer_cache_test.go三个对应测试，hot_cache_test.go的TestMain/goleak与既有testutil等待逻辑。
选定路径不需failpoint，无生成器/模块整理/格式化写入。
运行现有测试（只读Go模块；没有创建任何Go源码）：

`GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics -run '^(TestThresholdWithUpdateHotPeerStat|TestHotPeerCacheTopNThreshold|TestHotPeerStatIsHotUsesCPULastAverage)$' -count=1 -v`

[原始输出](task20/go-oracle.out)记录既有Go断言通过，不能称为逐事件JSON运行输出。
对应输入与原测试断言：read byte速率16384，每个region持续报告至rolling热满，
依次插入1..69，报告间隔120/60/30/17/1秒；少于60个peer阈值8192，达到60后13107.2。
write region id=0..99，byte/key/query累计id*6000，每次60秒，重复10次；id0冷而不入缓存，
最终Top60最小速率4000，默认8秒刷新未过期时key阈值32，立即刷新后3200。
CPU独热最近平均10恰好命中阈值10；CPU与其他维度皆0时不热。

补充源码契约（区分于以上实际运行断言）：各store/方向独立；四维byte/key/query/CPU，
每维threshold=max(第60大denoised GetLoad * 0.8, 最小阈值)，不足60回退最小值。
read最小[8192,128,128,1]，write最小[1024,32,32,1]；GetLoad使用真实rolling中值并取整。
已有阈值仅在elapsed>刷新间隔时重算，elapsed等于间隔或时间回退保留旧值。
Rust宿主显式传入纳秒时钟，避免库读全局时间；0刷新间隔测试须推进时钟。
TopN索引只在应用Add/Update/Remove时改变，丢弃检查动作不改索引。
TTL、GC、继承及异步队列不属于本任务。

## Rust实现与TDD

开始HEAD `aaf652a738ab6e3c56b21f3ca25cd3e2a475c473`。
先添加最小可编译接口和显式注册的5个行为测试，再运行：

`bash docs/rust/state/task20/slot.sh bash -c 'cargo fmt --all && cargo test -p astersql-pd-statistics --locked --lib state_task_20_hot_threshold -- --nocapture'`

[红灯](task20/red.out)：退出101，0 passed、5 failed，全部为真实行为断言。
包括read第60个peer仍8192而预期13107.2、write刷新后仍32而预期3200、CPU仍1而预期10.4。
[首次绿灯](task20/green.out)：同命令退出0，5 passed、0 failed/ignored。
最终测试补充丢弃动作不提前改变TopN、应用动作才改变边界、第二store真实高负载隔离。

[实现](../../../pkg/statistics/hot_threshold.rs)在原HotPeerCache公开
calc_hot_thresholds、set_thresholds_update_interval和check_peer_flow_dynamic；
[局部接线](../../../pkg/statistics/hot_peer_update.rs)在应用动作时维护每store真实TopN。
索引保留去噪独立快照，原缓存保留真实窗口，动态检查使用实际排名而非mock阈值。
[独立测试](../../../pkg/statistics/hot_threshold_test.rs)在lib.rs显式注册。
无Cargo manifest、Cargo.lock或依赖变更。

5项聚焦测试复现上述三个Go测试的阈值/判热断言，并补充：各维独立排名、更新替换、
删除使边界移动或不足60时回退、最小值钳制、未知动作忽略、read/write/store隔离、
8秒边界严格大于、时钟回退、CPU去噪12.5取整13而阈值10.4、CPU瞬时10.3拒绝/10.4接纳、
完整窗口CPU独热使hot_degree从1到2、CPU最近平均恰等阈值及清空后不热。
报告间隔120/60/30/17/1秒使用真实TimeMedian；1秒场景按Go原testMetrics绕过短报告过滤。
Rust事件断言与Go原测试预期一致；Go输出仅提供原断言通过证据，没有新建Go oracle或声称JSON快照。
本任务为同步内存策略，不涉及后台资源、MetaStore持久化或网络，无新增cancel/join需求。

## 验证与资源

最终入口 `bash docs/rust/state/task20/slot.sh bash docs/rust/state/task20/validate.sh`。
[脚本](task20/validate.sh)、[完整日志](task20/validation.out)、[metadata](task20/metadata.out)。
全部Cargo调用先由[槽位包装器](task20/slot.sh)原子mkdir动态领取1–10，
记录计划/任务/会话/PID，等待子进程或中断后终止并wait再释放；缓存保留。
红灯、首次绿灯、最终回归均实际领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

已只读审查pkg/storage/kv/etcd_fixture_test.rs、scripts/rust-etcd-fixture/main.go与pkg/proto/build.rs。
全workspace回归使用现有Go源码构建Etcd夹具，固定kvproto ZIP校验摘要后仅在OUT_DIR生成Rust。
全部子进程继承GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off；
不调用make/failpoint/生成器或Go格式化。Etcd夹具EOF关闭，必要时kill并wait，产物和数据在槽位内。

| 最终命令 | 退出码 / 有效测试数 |
| --- | --- |
| 上述只读Go oracle | 0；3个顶层测试通过 |
| `cargo fmt --all` | 0；无范围外格式化变更 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_20_hot_threshold -- --nocapture` | 0；5 passed，0 failed/ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0；61 passed，0 failed/ignored |
| `cargo check --workspace --locked` | 0；仅已有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0；1076 passed，0 failed，1已有ignored |
| `python3 docs/rust/state/task20/readonly.py` | 0；Go/模块/计划不变，链接和diff检查通过 |

[汇总](task20/summary.json)按最终日志逐命令统计；已有ignored为tso_processes，不计本行为验收。
全workspace中的真实MemoryStore与EtcdMetaStore契约通过。
[资源退出检查](task20/resources.out)确认自有锁已释放、无本槽位测试/Etcd进程、缓存保留。
[只读检查](task20/readonly.out)确认1101个Go/模块文件集合、内容哈希与Git状态和起点一致，
[起始哈希](task20/go-before.json)、[起始状态](task20/go-status-before.out)，
aggregate SHA256=`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
两份只读plan.md的[起始哈希](task20/readonly-plans.json)保持不变；用户已有计划改动保留。

状态：已完成。验收范围仅热点动态阈值，不代表后续缓存策略或整个主题完成。
