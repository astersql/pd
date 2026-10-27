# 任务116：迁移调度热点查询与观测

依赖115提交`87665f602`已核实，已有6项生命周期测试、包级20项及workspace1764项
通过证据；本批次仅执行116。整体/包级plan只读，没有启动并行任务。

用户后续授权允许以**只读Go源码/测试推导 + Rust真实行为/故障验证**替代缺失的Go
运行oracle。任务期望明确为source_assertion；未运行Go调度oracle、不声称运行差分。
[provenance](logs/task_116/provenance.json)保留来源commit、当前checkout、逐文件SHA-256、
fixture哈希；Go command/output/binary字段均为null。已查oracle-index，其既有运行案例
不覆盖本次门面验收。已有statistics投影继续复用任务25已验证的canonical聚合。
只读核对MCS API/hot_status Go测试仅覆盖部分接口，不以列出测试名冒充本次运行结果。

生产路径：[coordinator_observe.rs](../../../pkg/schedule/coordinator_observe.rs)，
[领域说明](../../../pkg/schedule/coordinator_observe.rs.md)。主要API：

- `Coordinator::get_hot_regions_by_type`：真实统计聚合、当前region的learner、
  write region报告秒/read store心跳纳秒补充到两个人口。
  learner严格保留Go `GetPeer(StoreID)`的按peer ID查找行为，不修正现有Go语义。
- `Coordinator::get_hot_regions`：空ID取全部，重复ID去重，未知ID序列化为null。
- `Coordinator::collect_hot_spot_metrics`及`HotSpotMetrics`：宿主Registry、缓存Gauge
  handles、两种RW/两种角色的四维totals和count，缺人口删除、空人口保留0。
- `HotSpotMetrics::delete_store_metrics`、`reset_hot_spot_metrics`：按store清理与全reset；
  reset同时清理既有scheduler pending vector，保留其它families，重新收集恢复series。
- 必要依赖扩展`SchedulerMetrics::delete_hot_pending_metrics`只清理指定pending family，
  不改变既有scheduler-wide reset规则；未复制pending/operator状态或新建cache。

[fixture](fixtures/task_116.json)独立于Rust结果固定输入与源码推导期望：2个RW查询案例、2个learner身份案例、
6个pending状态/权重案例、4个时间案例（分别断言read和write）。表驱动14项，
共20个方向/状态比较；最终全部通过。更新时间按UTC表示同一Unix instant；不舍入精度，不忽略错误/目标。
读写stats来自真实HotPeerStat及canonical统计函数；pending来自真实Operator、
TransferLeader、HotLoadProjection、StoreSummaryInfo及既有SchedulerMetrics。
使用真实BasicCluster/StoresInfo、现成Coordinator装配及MetaStore MemoryStore；
只控制明确的operator时钟边界。无mock调用次数断言。

6项显式注册测试：

1. `task_116_behavior_cases`：read/write不同负载、learner、leader/peer时间、
   全部/显式store、重复ID、未知store的null及JSON形状。
2. `task_116_metrics_cleanup_and_reset`：20条指标逐值核对；重复注册错误；
   缺人口与存在空人口差别；任一RW无hot时删除全部RW pending；reset清除真实
   pending但保留scheduler status；clone/reset/delete后的handles可重建；
   removed旧snapshot与已删除store的旧snapshot均不能复活指标。
3. `task_116_pending_weights_use_real_operator_status`：running权重1/2、兼容下限1、
   canceled立即无影响、success窗口内完整影响及窗口末尾影响清零；查询原始负载不叠加pending。
4. `task_116_missing_region_and_negative_read_time`：region缺失保留write零时间，
   read仍查询store，-1ns正确为epoch之前一纳秒。
5. `task_116_timestamp_cases`：UTC epoch、闰日、year0/year10000、负纳秒与尾零裁剪；
   同一fixture分别核对read/write。region u64秒明确按Go int64转换。

6. `task_116_learner_lookup_uses_go_peer_id`：peer ID与store ID分离，以及peer ID碰巧
   等于另一个store ID两种情况；两种RW、两个人口均核对原Go的按peer ID查找结果。
   该追加测试先在错误的GetStorePeer实现上取得[行为红灯](logs/task_116/learner-red.json)：
   exit101、0通过/1失败，true与源码期望false不符；随后改为get_peer并最终通过。

[行为红灯](logs/task_116/red.json)中聚焦命令exit101，1项真实测试失败于learner断言，
编译成功，非缺API/零测试。测试保持并在最终通过。
开发过程日志也保留：green有Prometheus测试访问器编译错误；green2因测试只改旧
StoreState而非canonical NodeState导致故障输入未生效；pre-final包含错误的success
线性衰减期望。重新核对Go `calcPendingInfluence` 472–492行后，fixture修正为窗口内
完整权重、窗口末清零，未修改该既有Rust算法。上述中间日志不作为通过证据。

[最终命令](logs/task_116/final.json)记录精确命令、退出码、stdout/stderr压缩日志、
有效测试数及实际共享槽位。所有Cargo阶段均动态依次原子mkdir领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，
子进程继承CARGO_BUILD_JOBS=1、RUST_TEST_THREADS=2、GOFLAGS=-mod=readonly。
Cargo.lock只补充本crate已存在workspace依赖的边，不变更远程版本。
Rust修改后先cargo fmt --all再验证；没有范围外格式化改动。

最终验证：cargo fmt --all、聚焦cargo test、包级cargo test、cargo metadata、
cargo fmt --all -- --check、cargo check --workspace --locked、
cargo test --workspace --locked。精确结果在最终日志及test-counts.json。

workspace中既有etcd fixture仅对现有只读Go工具执行go build，产物/WAL在领取槽位；
proto产物命中共享槽位缓存，本轮未重新执行proto源码解包/生成；调用链已检查，
未注入failpoints。
[只读基线](logs/task_116/readonly-before.json.gz)与readonly-check.json核对全部Go/
模块文件（含未跟踪非忽略文件）清单/内容/Git状态，以及两份只读plan哈希。
所有阶段结束后待子进程退出，删除自有owner并rmdir释放槽位，保留缓存。

最终7条Cargo命令全部exit0：聚焦6通过、包级26通过、workspace1770通过/0失败，
1项既有ignored（含2项通过的compile-fail文档测试）。14项表驱动案例全部通过，
[统计](logs/task_116/test-counts.json)、[最终复核](logs/task_116/verification.json)可审计。
Go/模块1101份清单/哈希/Git状态、两份plan哈希完全保持一致，git diff --check exit0；
相关Markdown链接有效，槽位1锁已释放且无槽位产物进程残留。只提交本任务相关文件，
其它任务原有的编号文件删除记录不纳入提交。
