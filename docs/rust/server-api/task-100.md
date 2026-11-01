# 任务100：PD协议完整性回归

状态：已完成（主题7已实现的41个classic PD方法；保留下述异构后端边界）。

基准 `80bc6c0aa`；依赖[任务099](task-099.md)及传递依赖已核验。
新增[回归目录与执行器](../../../tests/rust-compat/grpc_regression.rs)、
[独立测试](../../../tests/rust-compat/grpc_regression_test.rs)，在
[lib.rs](../../../tests/rust-compat/lib.rs)显式注册。复用真实场景与只读supervisor，
没有复制领域实现，也没有改动生产服务或Go/模块文件。

## 矩阵和来源

[wire-inventory.md](wire-inventory.md)的56个classic PD方法中，主题7已实现39个，
另有owner78的两个废弃watch由任务37实现明确的Unimplemented契约，共41个。
其余15个GC领域方法仍由主题8准入，不计本任务通过；独立MCS、Dashboard和
Go embedded-etcd替代同样不在本次声明范围内。协议固定
`f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

`task_100_behavior`从真实清单提取方法集合，与执行目录精确比对；任何缺失、
未登记或新增方法都会导致断言失败。23组入口必须全部成功，不能以某个默认桩、
ignored或0 tests补齐矩阵。22组复用`run-pd-compat-readonly.sh`，共享标记另用
`http-admin-markers-oracle.py`；各supervisor要求恰好1通过、0失败、0忽略，
检查Go只读集合、进程组与工作目录清理。

| 入口case | 完整协议矩阵方法 |
| --- | --- |
| members | GetMembers, GetClusterInfo |
| classic-tso | Tso, GetMinTS |
| bootstrap | Bootstrap, IsBootstrapped |
| alloc-id | AllocID |
| store-registration | GetStore, PutStore, GetAllStores |
| cluster-host | StoreHeartbeat, RegionHeartbeat |
| bucket-heartbeat | ReportBuckets |
| sync-regions | SyncRegions |
| region-lookup | GetRegion, GetPrevRegion, GetRegionByID |
| region-scan | ScanRegions, BatchScanRegions |
| region-query | QueryRegion |
| ask-split | AskSplit, AskBatchSplit |
| report-split | ReportSplit, ReportBatchSplit |
| cluster-config | GetClusterConfig, PutClusterConfig |
| scatter | ScatterRegion, GetOperator |
| split-regions | SplitRegions |
| split-scatter | SplitAndScatterRegions |
| global-config | StoreGlobalConfig, LoadGlobalConfig |
| global-watch | WatchGlobalConfig |
| report-min-resolved-ts | ReportMinResolvedTS |
| external-timestamp | SetExternalTimestamp, GetExternalTimestamp |
| deprecated-rpc | SyncMaxTS, GetDCLocationInfo, WatchGCSafePointV2, WatchGCStates |
| admin-markers | IsSnapshotRecovering |

[source-inventory.md](source-inventory.md)的任务100摘要，以及owner含100的全部93行
已作为原始来源行保存在压缩证据中：涉及proto、grpc_service、兼容测试框架和
共享worker/failpoint入口及各自哈希。共享文件行不把其他owner的领域或Go failpoint
自动计作通过；本次只按41个方法和23组真实场景验收。兼容框架中的既有schedule/state
测试另由包级/工作区回归覆盖。[surface-inventory.md](surface-inventory.md)没有100专属
配置/CLI/指标行，不扩张其覆盖声明。

## 字段、错误和状态

既有runtime差分通过`PdNodeBuilder`启动真实Rust节点及TCP listener，并与已有Go服务
二进制执行同场景；新增负例实际启动已构建的Rust `pd-server`。这里不把仅设置
`PD_COMPAT_RUST_BIN`描述成所有矩阵方法都经独立Rust进程运行；独立进程/滚动重启
和真实Go客户端的装配证据沿用任务97–99，并由完整回归再次验证。

复用场景通过生成消息的完整编码/类型比较或逐字段断言检查响应，包含默认值、
optional presence、未知enum数值、错误header、流消息和EOF；真实RPC、HTTP及
MetaStore状态共同验证注册、bootstrap、心跳、查询、拆分、watch和恢复。
ID与TSO比较唯一/单调性质，随机identity及URL按已校验关系比较，时钟原点与revision
按已记录规则处理。流批次可由后端合并，事件内容、顺序和revision边界必须一致。

两个已审查的异构后端边界保持可见：Go embedded-etcd的etcd_leader是成员，
Rust外部etcd不伪装成PD成员，因此为None；GetMembers超时错误分别保留Go的
ErrEtcdMemberList/DeadlineExceeded与Rust MetaStore Unavailable原文。
新增执行器分别精确断言两条原始文本，不再只依赖旧场景的UNKNOWN摘要，
不将不同文本归一化为相等。取消的本地deadline竞态按原场景允许的终止状态验证，
原始transport诊断仍保留。这些边界不宣称逐字节/拓扑等价。

`task_100_negative_controls`通过真实Rust二进制/TCP取得成功消息和错误Status，
检查改变服务enum、URL、缺失header、present-default error的完整生成类型差异；
对真实Status分别改变code/message后必须产生字段差异。
TSO编码/解码负例覆盖count、physical delta、logical、suffix bits、timestamp presence、
error enum和message；identity规则不能吞掉相同数值的error/state。

## 失败与通过

首次先提供可编译的最小目录接口及矩阵断言，未登记41个方法。
`cargo test --locked -p astersql-pd-compat-tests --lib task_100_ -- --nocapture`
实际运行1项，0通过/1失败/0忽略，退出101，断言列出缺失的41个方法。
这是缺少统一回归接线的行为失败，不宣称生产服务当时缺少这些方法。

补齐23组接线后同一聚焦命令2通过、0失败、0忽略；每组双实现差分也均1通过。
随后补强GetMembers原始文本检查和错误字段变异检查，已由完整包级及最终工作区验证覆盖。
无新生产实现需要人为制造失败。所有证据只使用已有只读Go二进制/源码，
没有运行Go测试、make、failpoint启停、生成器、模块整理或Go格式化。

## 执行与检查

每阶段从1至10依次原子mkdir领取共享槽位，owner记录计划、任务、会话和持有PID。
本次实际领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
全部Cargo、测试、supervisor及服务子进程继承此绝对路径，默认features，
无`--target-dir`覆盖；`CARGO_BUILD_JOBS=4`、`RUST_TEST_THREADS=4`。

```sh
cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server
export PD_COMPAT_MODE=dual
export PD_COMPAT_GO_BIN="$PWD/bin/pd-server"
export PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server"
cargo test --locked -p astersql-pd-compat-tests --lib task_100_ -- --nocapture
```

包级和工作区外层清除三个PD_COMPAT变量，沿用既有测试的标准环境；
本任务执行器在两个子supervisor中显式设置dual和绝对Go/Rust二进制路径，
因此新任务在这些命令内仍实际执行双实现。Go客户端工具来自已验证依赖任务99，
本次不构建或改写Go工具源码。已修正[README](../../../tests/rust-compat/README.md)
中会改写Go/failpoint的旧入口推荐；新版明确区分只读入口、已有工具前提和元数据规则。

| 确切命令 | 退出码 | 有效测试（通过/失败/忽略） |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `cargo test --locked -p astersql-pd-compat-tests --lib task_100_ -- --nocapture` | 0 | 2/0/0；嵌套23组各1/0/0 |
| `cargo test --locked -p astersql-pd-compat-tests --lib task_100_negative_controls -- --nocapture`（增强后） | 0 | 1/0/0 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-compat-tests` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `env -u PD_COMPAT_MODE -u PD_COMPAT_GO_BIN -u PD_COMPAT_RUST_BIN cargo test --locked -p astersql-pd-compat-tests -- --nocapture` | 0 | 40/0/1 |
| `env -u PD_COMPAT_MODE -u PD_COMPAT_GO_BIN -u PD_COMPAT_RUST_BIN cargo test --workspace --locked` | 0（最终） | 2228/0/79；此前失败见下文 |

嵌套场景不重复加到包级的40个顶层测试数中。任务100三项均非ignored；
既有TSO忽略项不算通过，classic TSO在本任务的真实dual入口已独立实际执行。
工作区最终通过；只读守卫及原始证据见文末。

Go oracle的build info确认kvproto版本为`v0.0.0-20261016101550-f8ad079c8e98`，
与Rust协议输入相同；已有Go二进制没有VCS revision，不声称由当前HEAD构建。

## 工作区发现的scatter接线问题

首轮聚焦和完整包级通过后，首次工作区运行在既有split/scatter场景等待split command
的15秒断言失败。只读定向入口再次复现，0通过/1失败、退出101。
通过复制自有临时目录中的运行日志保留诊断，没有修改Go源码、模块、生成器或oracle。
Go日志显示：region102的`scatter-region`已创建`store1 → store2` leader transfer，
后续`region-splitter`因`already exist`被拒绝；测试尚未确认scatter命令就开始下一轮split。
此前随机选择当前leader的no-op分支掩盖了这一测试准备问题。

最小修复仅扩展[既有场景](../../../pkg/server/grpc_split_scatter_scenarios_test.rs)：
读取真实operator；有transfer时消费真实RegionHeartbeat命令，以新leader心跳确认，
等待GetOperator Success及完整region/leader可查。下一轮按实际leader校验完整target peer，
并在leader变化时打开对应store的新心跳流。部分拆分保留该实际leader的peer，
继续验证单副本导致scatter无法创建operator的原边界。
完整/部分百分比、返回region ID、原15秒command deadline和65秒完成等待均保留。

修复后`cargo test --locked -p astersql-pd-server --lib task_031_behavior -- --nocapture`
退出0、1/0/0；`bash scripts/run-pd-compat-readonly.sh --case split-scatter`退出0、1/0/0。
完整包级40/0/1是修复前的通过记录；最终工作区已再次执行该包全部测试及23组差分，
提供最终测试接线的当前证据。全程没有生产服务实现变更。

### 延迟响应头与稳定分支

首次transfer接线增强后，工作区日志实际观察到store1→store2的scatter完成，
但Go在新心跳流尚无首条命令时不返回响应头，提前等待重连结果阻塞了下一轮RPC。
通过向自有只读supervisor发送SIGTERM中止该次验证；supervisor报告130，
外层Cargo退出101，进程组、目录及Go只读守卫正常收尾。

最终场景使用待完成的真实流future：初始流一边被poll一边通过GetRegion确认metadata，
重连流与下一轮split一起在原有15秒command deadline内推进；timeout直接丢弃future，
不留下独立后台opening任务。初始leader设为store3，令canonical Rust选择器稳定产生
真实transfer；Go相同输入也实际走到store3→store1。

最终定向Rust测试1/0/0；独立dual场景1/0/0，并实际打印双方各一次
`scatter command completed`，随后完整/部分拆分、错误、取消及follower断言全部通过。
为补充真实Go transfer分支而运行的诊断只在每次场景成功后检查覆盖；任何场景失败即退出。
旧fixture的四次成功no-op诊断不算transfer覆盖，下一次诊断经supervisor主动中止后换成
稳定fixture；最终fixture首轮即取得完整Go transfer和后续split证据。
后续同步关闭检查完成后，最终冻结输入在整个工作区验证前后保持一致。

## SyncRegions关闭帧回归

随后一次工作区运行实际捕获Go的终止应用帧：UNKNOWN错误header、完整cluster ID、
`server stopped, close the region syncer client`文本，其余所有向量为空、start_index=0。
旧测试直接unwrap transport error而失败。Go `pkg/syncer/server.go::closeAllClient`会尝试
发送该帧，同时`Sync`因ctx/done结束返回Unavailable，因此网络交付可能只有transport，
也可能先有应用错误帧。Rust的transport-only路径是本次反复观测到的合法Go路径之一。

[关闭路径测试](../../../pkg/runtime/wire_sync_regions_test.rs)现在并发接收和等待自有Go进程
停止；若终止应用帧到达，通过[共享全字段比较器](../../../tests/rust-compat/harness.rs)验证
其完整生成消息，再要求后续Unavailable。Rust错误文本精确为`region syncer stream closed`；
Go精确接受源码两条终止原因`region syncer stopped`或`region syncer stream closed`，
原始状态与文本分别记录，不把应用错误帧转换成正常数据或隐藏。

为验证并发可选帧而执行的默认/单核诊断累计40次均实际通过1/0/0，但没有再次收到该帧，
覆盖搜索因此退出1；它们不算该帧的正例，也不被表述为协议测试失败。
按任务允许的已保存Go输出，新增[解码回放fixture](../../../tests/rust-compat/fixtures/task-100-sync-close.json)：
保存真实失败运行中的完整Debug消息、原日志SHA-256、Go二进制SHA-256及固定协议来源。
这是Go实际响应的解码字段，明确不声称保存了原始Go wire bytes。

`task_100_sync_terminal_recording`从这些实际字段重建消息，复用实时路径的同一比较器；
对header presence/ID、error presence/enum/message、cursor及全部六类向量分别变异，
12个变异均必须失败。首次重建把UNKNOWN误映射为OK=0，回放实际退出101；
比较器明确报告Ok与Unknown不同。依照固定协议修正UNKNOWN=1，并以OK=0作负例后，
回放1/0/0、runtime定向2/0/0及实时dual 1/0/0通过。
冻结输入包括1365项Rust/Cargo文件和此回放fixture，共1366项，最终工作区已通过当前版本。

## 最终守卫与归档

最终`cargo test --workspace --locked`退出0，2228通过、0失败、79既有忽略；
任务100三项均实际通过。完整矩阵23组均1通过/0失败/0忽略，修复后的scatter场景
再次实际完成双方transfer及后续split；保存Go输出回放及12个字段变异也通过。
最终metadata、fmt check、compat check、workspace check均退出0。

开始/结束含ignored/untracked的1101个Go/模块文件路径、内容SHA-256及Git状态完全一致，
Go Git状态为空；只读plan.md字节哈希与开始相同。1365项Rust/Cargo及1项回放fixture
在最终回归期间SHA-256一致。全部59份监督记录保留各自结果，包含预期失败、主动取消及
正例；各份Go/输入守卫与进程组/目录清理检查均通过，不把失败运行或覆盖搜索退出1算作通过。

实际槽位始终为动态领取的1；
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
自有服务、工具、fixture及槽位二进制进程均已结束；自有slot-1锁已释放，缓存保留。
保留原有任务94–99删除和prompt.md修改，不纳入提交；任务100文件按要求删除。

确切命令/退出码/完整日志、原始监督记录、来源93行、完整Go文件清单、Rust/Cargo/fixture
哈希、诊断脚本和保存Go响应来源见[压缩证据](task-100-evidence.json.gz)。
重复Go快照按内容SHA-256去重，保留原报告字节哈希且验证可逐字节重建。
证据SHA-256：`cf5aab18afe8fefdeb48d334896bdd71a9afc30eb9dc0b679cb4fc6a70995fc9`。
