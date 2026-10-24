# 任务19：热点副本升温与降温

开始HEAD `452787ea1894d593875648029d20387f10c4335f`。
Go基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`；hot_peer_cache.go及测试无基准差异。
依赖[任务18](task-18.md)及累计1–17已完成。已读两份只读计划、AGENTS、Copilot、ADR、依赖和移植指南。

## 实现前oracle

审查HotPeerCache CheckPeerFlow/updateNewHotPeerStat/updateHotPeerStat、init/hot/cold/inheritItem，
TestMain/goleak/WaitForEtcdConnections、buildRegion/checkAndUpdate及movingaverage调用链。
选定测试没有failpoint路径，不执行生成/格式化/模块整理；仅运行现有测试，模块强制只读：

`GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics -run '^(TestUpdateHotPeerStat|TestUnstableData|TestDifferentReportInterval)$' -count=1 -v`

退出0，3个顶层测试通过，[原始输出](task19/go-oracle.out)。
输出只记录既有断言通过，不声称Go逐事件JSON快照。
TestUpdateHotPeerStat的原断言：0秒忽略，cold新项忽略；60/4秒前两次degree/anti=0/0，
第二次手设anti=12后第三次1/12，随后4秒保持1/12，10秒2/12；阈值提高至10后连续冷报告
依次1/11至-10/0，最终Remove。TestDifferentReportInterval在120/60/30秒下保持3个store各1个peer。
TestUnstableData的六个10事件序列及预期中值完整复用到Rust测试。

补充源码契约：全8维累计输入按READ/WRITE映射到4维速率；秒转换纳秒供真实DimStat窗口使用。
首次满窗口degree=1、anti=方向默认值（read12/write2），短窗口0/0；满窗口之后hot使degree+1、
anti仅+1恢复至默认上限，cold使二者-1并在anti<=0移除；每个完整窗口清last average而保留median。
显式空peers不回退，None回退region peers，显式额外store保留region原stores和leader标记。
额外边界由源码契约支持，区别于上面三个Go测试的实际运行断言。

本任务为同步内存生命周期；动态TopN阈值、继承/主节点变化、TTL/GC、async队列属于后续任务。

## Rust实现与测试驱动

先注册最小可编译接口后运行
`bash docs/rust/state/task19/slot.sh bash -c 'cargo fmt --all && cargo test -p astersql-pd-statistics --locked --lib state_task_19_hot_peer_update -- --nocapture'`。
[红灯输出](task19/red.out)退出101：1 passed、3 failed，真实断言为CheckPeerFlow输出0项而预期1/3项；
复用的TimeMedian序列已经通过，没有为红灯破坏依赖实现。
实现后同命令[绿灯](task19/green.out)退出0：4 passed、0 failed/ignored。
补充隔离/动作边界测试后最终聚焦5 passed，无忽略。

[生产实现](../../../pkg/statistics/hot_peer_update.rs)公开HotPeerCache::new、check_peer_flow、
update_stat、get、set_denoising和minimum_thresholds；私有update_hot_peer_stat实现完整窗口判热与降温。
[独立测试](../../../pkg/statistics/hot_peer_update_test.rs)显式注册到原canonical statistics crate，
无manifest/锁文件/依赖变化。HotPeerStat/DimStat复用批次18真实窗口；局部copy_for_report保留pending delta，
避免误用会重置delta的继承clone。检查返回独立拥有窗口的动作，宿主顺序消费；丢弃动作不改变已存状态，
该Rust事务所有权接口区别于Go共享旧rolling指针。成功应用后的Go事件快照一致。

5项测试覆盖上面全部原Go事件断言，以及CPU单维inclusive阈值、READ/WRITE隔离、
显式空/额外peers、leader和stores元数据、hot恢复anti每次仅+1与上限、denoising 0/1/2秒、
关闭denoising接收1秒报告、首个未热满窗口无anti储备而立即Remove、未知动作忽略、
动作丢弃保持pending 4秒与delta60且后续60/6秒仍判热（证明复制没有丢pending delta）。
所有成功路径检查最终状态，包括应用Remove后get=None；窗口输出中值六序列与Go原断言一致。
本任务不涉及后台任务、网络、持久化或MetaStore，无需新增cancel/join或后端故障注入。

## 最终验收与资源

`bash docs/rust/state/task19/slot.sh bash docs/rust/state/task19/validate.sh`退出0。
[脚本](task19/validate.sh)、[完整日志](task19/validation.out)、[汇总](task19/summary.json)。

| 命令 | 退出码 / 实际测试数 |
| --- | --- |
| 上述只读Go oracle | 0；3个顶层测试通过 |
| `cargo fmt --all` | 0；无范围外格式化改动 |
| `cargo metadata --format-version 1 --no-deps` | 0；[metadata](task19/metadata.out) |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_19_hot_peer_update -- --nocapture` | 0；5 passed，0 failed/ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0；56 passed，0 failed/ignored |
| `cargo check --workspace --locked` | 0；仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0；1071 passed，0 failed，1既有ignored |
| `python3 docs/rust/state/task19/readonly.py` | 0；Go状态/集合/哈希一致，文档链接和diff通过 |

既有ignored为tso_processes，不计本行为验收；workspace中的真实MemoryStore/EtcdMetaStore契约通过。
已审查现有Etcd夹具及scripts/rust-etcd-fixture/main.go、pkg/proto/build.rs：仅只读构建现有Go源，
固定kvproto ZIP摘要并在OUT_DIR生成Rust，二进制与etcd数据均在所领槽位；
夹具EOF关闭、必要时kill并wait。GOFLAGS=-mod=readonly、GOPROXY=off、GOARCH=arm64、CGO_ENABLED=0
由包装器传递给全部Cargo及其子进程，不调用failpoint、make或Go生成器。

[槽位包装器](task19/slot.sh)原子mkdir动态尝试1–10并记录计划/任务/会话/PID，退出时等子进程结束后释放。
红灯、绿灯、最终回归（含中间编译修复重跑）均实际领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[资源检查](task19/resources.out)：自有锁已释放、全部使用产物的测试及etcd进程结束、缓存保留。

[只读检查](task19/readonly.out)、[起始内容哈希](task19/go-before.json)、
[起始状态](task19/go-status-before.out)确认1101个Go/模块文件无新增/内容/状态变化；
aggregate SHA256=`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
两份指定只读plan的[哈希](task19/readonly-plans.json)与批次18记录一致；用户已有计划变更保留。

状态：已完成；验收仅限直接报告的热点副本升温与降温，不代表后续热点策略或整个主题完成。
