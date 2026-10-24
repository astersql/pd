# 任务16：放置规则驱动的区域分类

实现前 HEAD `fdc3330af101b53de259b2e90805d4e9fdfc46b4`；Go 基准
`69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，region_collection.go/test.go 无后续差异。
依赖 [任务15](task-15.md) 已完成累计1–15；现有真实RuleManager与规则range查询满足依赖。
已读两份只读plan、根规则、Copilot、ADR、依赖基线与移植指南；目标目录无更深AGENTS。

## 实现前 Go oracle

只读运行 `GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics -run '^TestRegionStatisticsWithPlacementRule$' -count=1 -v`：
退出0，1个测试通过，[输出](task16/go-oracle.out)。只运行已有测试，无新增Go工具。
审查TestMain/goleak/WaitForEtcdConnections、NewTestOptions、内存Storage、RuleManager.Initialize、
GetRulesForApplyRegion及Observe调用链：本路径不需failpoint，不调用生成器或格式化器。
GOFLAGS强制模块只读。原require断言提供运行状态证据，无逐事件JSON输出。

输入顺序：初始化3副本默认voter规则；aa..bb的1peer region0为miss；ee..ff的5peer
region1（含learner及witness）为extra；gg..hh的3peer region2健康；region0替换为
3peer（含learner及witness leader），witness分类数1。Rust将原peer ID/store ID保持原值重放。

源码推导（区别于运行差分）：Observe启用placement且manager未初始化时直接返回、保留原索引；
总副本数为GetRulesForApplyRegion全部Count之和，voter数排除Learner规则；不计算fit。
跨规则分段返回空规则，因此期望计数0、有peer则extra；不使用max_replicas回退。
miss-voter仅在MissPeer分类内累计，learner不足而voter足够不计时；秒单位与Go零哨兵一致。
配置和规则每次observe重新读取；NeedUpdate不因副本计数变化扩张触发范围。
额外learner、未初始化与配置事件是源码契约扩展测试，不冒充Go逐事件运行差分。
边界还复用已有只读`TestKeys`运行：
`GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/schedule/placement -run '^TestKeys$' -count=1 -v`，
退出0，1 passed，见[输出](task16/go-boundary-oracle.out)。
审查其newTestManager、内存KV、Initialize/SetRules/Batch和范围查询，无failpoint或Go写入。
该运行确认跨分段GetRulesForApplyRegion为空；Rust边界测试按同一语义扩展为ASCII范围与override。

## 开发与验证

最小可编译入口先返回None，独立测试显式登记；首次编译修正KeyRange结构体语法及不同长度byte slice，
不将编译错误计作行为失败。维护已有workspace依赖的Cargo.lock边后，运行
`bash docs/rust/state/task16/slot.sh bash -c 'cargo fmt --all && cargo test -p astersql-pd-statistics --locked --lib state_task_16_region_placement -- --nocapture'`：
退出101，[行为红灯](task16/red-behavior.out)为4 failed/0 passed，具体断言为观察返回None而非Some，
以及合法初始化规则的observe返回None导致unwrap失败；不是零测试或缺符号。
初次锁文件维护使用`cargo check -p astersql-pd-statistics --offline`，仅增加依赖边，不改版本。

实现[region_placement.rs](../../../pkg/statistics/region_placement.rs)的
`RegionStatistics::observe_with_placement`，只读消费真实RuleManager适用规则；
将原分类器抽取内部observe_counts共享路径，保持原observe接口行为；
未初始化None明确表示未观察，不改变索引/计时。规则变更、placement开关和max_replicas更新均重读。
新增独立[测试](../../../pkg/statistics/region_placement_test.rs)，先4测试通过、包级41通过，
再补leader/follower计入voter、learner恢复时保留计时及已初始化空规则场景，最后统一验证5测试。
[行为说明](../../../pkg/statistics/region_placement.rs.md)记录所有边界。
仅依赖已有canonical placement；测试装配真实MemoryStore/StorageEndpoint/RuleManager，不mock规则或分类结果。

## 只读及资源边界

[槽位包装器](task16/slot.sh)原子mkdir依次尝试1–10，记录计划/任务/会话/PID；
正常、失败和信号退出均等待子进程后释放自有锁，不删除缓存。
所有Cargo与子进程继承绝对CARGO_TARGET_DIR及GOARCH=arm64/CGO_ENABLED=0/GOFLAGS=-mod=readonly/GOPROXY=off。
已审查唯一build.rs（pkg/proto/build.rs）仅固定ZIP下载校验、槽位OUT_DIR生成；
workspace真实etcd夹具及scripts/rust-etcd-fixture调用链使用已有Go源码只读build，产物/数据留在槽位。
夹具Drop关闭stdin、必要时kill并wait；没有Go生成/格式化/failpoint注入。
本模块只同步查询规则，无后台任务；真实MemoryStore规则已在聚焦测试覆盖；
EtcdMetaStore通过workspace已有后端契约回归，无需为只读规则查询另造后端协议。

## 最终验证

[验证脚本](task16/validate.sh)、[完整输出](task16/validation.out)、[metadata](task16/metadata.out)。


| 命令 | 退出码/有效测试数 |
| --- | --- |
| 上述两个只读Go oracle命令 | 各0，各1 passed |
| `cargo fmt --all` | 0，无范围外格式化变化 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_16_region_placement -- --nocapture` | 0，5 passed/0 failed/ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0，42 passed/0 failed/ignored |
| `cargo check --workspace --locked` | 0，仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0，1057 passed/0 failed，1既有ignored |
| `python3 docs/rust/state/task16/readonly.py` | 0，1101个Go/模块文件集合、SHA256及Git状态不变；文档链接和diff检查通过 |

workspace ignored为既有tso_processes，不计本任务验收；真实MemoryStore/EtcdMetaStore回归通过。
全部Cargo阶段实际动态取得槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[资源核对](task16/resources.out)确认Cargo/槽位子进程已退出、自有锁与owner已释放、缓存保留。
[只读核对](task16/readonly.out)、[起点哈希](task16/go-before.json)、[起点Git状态](task16/go-status-before.out)
确认Go/模块文件无新增/变化；aggregate SHA256
`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
两份指定plan.md及来源史诗未修改，用户和其他任务变更保留。

状态：已完成；仅验收放置规则驱动的区域分类边界。
