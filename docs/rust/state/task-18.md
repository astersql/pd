# 任务18：热点单副本时间窗口

开始HEAD `a8a746b7a9cb1e849b33d192544ea9eacc432c5a`；Go源码基准
`69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，hot_peer.go/hot_peer_test.go相对基准无差异。
依赖[任务17](task-17.md)累计1–17验收完成；真实TimeMedian、ActionType/RWType和TopNItem已可用。
已阅读两份只读计划、根AGENTS、Copilot、ADR、依赖和移植指南，无更深AGENTS。

## 实现前只读oracle

`GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics -run '^(TestHotPeerStatGetLoadBounds|TestHotPeerStatIsHotSkipsNilRollingLoads|TestHotPeerStatIsHotUsesCPULastAverage|TestCoolDownTransferLeader|TestUpdateHotPeerStat)$' -count=1 -v`
退出0，5个顶层测试通过，[输出](task18/go-oracle.out)。审查TestMain/goleak/WaitForEtcdConnections、
HotPeerCache更新/构建region/调度测试辅助、TimeMedian和指标初始化调用链：所选路径无failpoint，
无源码生成/格式化/模块整理，强制Go模块只读。仅执行现有Go测试，没有新增Go oracle。

原运行断言：短Loads [1,2]取维度0/1/2/-1得到1/2/0/0；全nil rolling回退[3,4,0,0]；
Key delta10/1秒与threshold5为hot；CPU delta100/10秒与threshold10为hot，delta0为cold。
TestUpdateHotPeerStat的4秒增量60三次在累计12秒填满10秒报告窗口（前两次hot degree为0，第三次为1），
之后4秒未满保持，10秒填满后再次更新；冷却测试证明转leader后冷却，未转移/移动副本无冷却。
运行输出只有原断言通过记录，未声称有Go逐事件JSON差分。

补充源码契约：dimStat累计每次interval的截断整数秒；clear只清last平均，不清TimeMedian；
dimStat.clone深拷贝窗口和累计秒数，lastDelta重置0；HotPeerStat.Clone取4维圆整去噪快照并去掉rolling。
GetLoads无rolling保留原短/未圆整Loads；有rolling固定4维，缺失维回退Loads；非法维0。
冷却为elapsed秒严格小于minHotDegree*RW报告秒数，时间由宿主显式输入。
小数秒/NaN/零除/克隆隔离/冷却等号扩展由只读源码与真实movingaverage契约验证，区别于原Go运行断言。
无后台任务、持久化或传输适配，本任务不实现HotPeerCache生命周期。

## 实现与测试驱动

先显式注册独立测试模块并提供最小可编译接口：
`bash docs/rust/state/task18/slot.sh bash -c 'cargo fmt --all && cargo test -p astersql-pd-statistics --locked --lib state_task_18_hot_peer -- --nocapture'`
退出101，0 passed/5 failed：[红灯](task18/red.out)。失败为真实断言：0!=1、CPU平均不热、
累计12秒未判满、快照[]!=[3,5,0,0]、转移后未冷却，不是未定义符号或零测试。
实现后同命令退出0，5 passed/0 failed/ignored：[绿灯](task18/green.out)。

[实现](../../../pkg/statistics/hot_peer.rs)与[独立测试](../../../pkg/statistics/hot_peer_test.rs)
在原canonical crate扩展，无依赖/锁文件变更。DimStat显式纳秒interval、每维真实TimeMedian、
累计整秒与lastDelta；HotPeerStat复用utils动作/方向/TopN，宿主提供时间及共享同步。
保留Go的两种clone语义，stores采用独立Vec以满足Rust隔离；无Go FFI、协议泄漏或私有runtime。

5个行为测试覆盖原Go的负载/缺失rolling/CPU输入，以及[60/4s,60/4s,60/4s]窗口
[0,0,15]、clone后单独加入600/10s得到37.5而原窗口仍15、clone后lastDelta=0、
clear保留中值、每次0.5秒截断、零除NaN/Inf、CPU last-average与median分离、
短/空rolling与Loads、多余维度/负维度、半值离零round、去噪快照与元数据独立、
真实TopN替换、READ30秒/WRITE180秒冷却在边界前1ns/等号/后1ns/未来时间/未转移的结果。
原Go运行断言与同输入Rust结果一致；额外边界是源码契约扩展，未声称新增Go运行快照。

## 资源与只读边界

[槽位包装器](task18/slot.sh)每阶段原子mkdir动态领取1–10，记录plan/task/session/PID，
子进程继承绝对CARGO_TARGET_DIR、GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly和GOPROXY=off。
正常、失败及信号退出等待命令结束后释放自有锁，保留缓存。
审查pkg/proto/build.rs的固定ZIP校验/OUT_DIR生成和真实Etcd夹具调用链：只读构建已有
scripts/rust-etcd-fixture/main.go，二进制/数据在所领槽位，Drop关闭stdin、必要时kill并wait。
本任务本身为同步内存窗口，不涉及后台cancel/join或MetaStore；workspace既有契约验证真实MemoryStore/EtcdMetaStore。

## 最终验收

`bash docs/rust/state/task18/slot.sh bash docs/rust/state/task18/validate.sh`退出0。
[验证脚本](task18/validate.sh)、[完整输出](task18/validation.out)、
[metadata](task18/metadata.out)、[汇总](task18/summary.json)。

| 命令 | 退出码 / 有效测试数 |
| --- | --- |
| 上述只读Go oracle | 0；5个顶层测试通过 |
| `cargo fmt --all` | 0；无范围外格式化变更 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_18_hot_peer -- --nocapture` | 0；5 passed，0 failed/ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0；51 passed，0 failed/ignored |
| `cargo check --workspace --locked` | 0；仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0；1066 passed，0 failed，1既有ignored |
| `python3 docs/rust/state/task18/readonly.py` | 0；Go/模块文件集合/内容/状态一致、文档链接与diff检查通过 |

既有ignored为tso_processes，不计本任务验收；真实MemoryStore/EtcdMetaStore契约通过。
红灯、绿灯及最终验证三个阶段均动态领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[资源检查](task18/resources.out)确认命令和使用该槽位产物的进程均结束、锁已释放、缓存保留。
[只读检查](task18/readonly.out)、[起始哈希](task18/go-before.json)、
[起始Git状态](task18/go-status-before.out)确认1101个Go/模块文件无新增/内容/状态变化，
aggregate SHA256=`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
指定两份只读plan的[哈希](task18/readonly-plans.json)复核相同，来源史诗及现有用户变更保留。

状态：已完成；仅验收热点单副本窗口边界，不代表热点缓存或整个主题完成。
