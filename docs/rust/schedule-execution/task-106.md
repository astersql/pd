# 任务106：注册全部内置调度工厂

状态：已完成（最终运行结果见下列日志记录）。

依赖105由提交 `e82869cbd32b3859be2ae922c7b4cf077da3f661`、
[task-105](task-105.md)及其final/test-counts验证记录核实：真实range调度策略、持久队列
和79条场景已通过，包164测试、workspace1713测试。当前任务仅串行处理106。
整体与包级plan保持只读；未使用子代理或改写原始主题。

本任务按用户后续授权，以只读Go源码/测试推导加Rust真实行为与故障验证替代缺失的
Go运行oracle/运行差分。oracle-index无106运行记录；未运行Go测试、工具或failpoints。
[provenance](logs/task_106/provenance.json)固定来源commit、计划基准、源码和fixture哈希；
runtime_oracle=null。fixture期望取自Go表达式及配置规则，不由待测Rust实现生成。

## 生产路径

[registry.rs](../../../pkg/schedule/schedulers/registry.rs)扩展canonical SchedulerRegistry，
从base迁走空表；显式`register_builtins`接入18类真实具体策略，默认空表契约保留。
`register_type`拒绝重复/未知/plugin，完整注册幂等，局部冲突先检查不发布部分表。
`create_args`、`decode_slice`接线既有配置构造器；`create_json`支持持久配置恢复。
`RegisteredScheduler::{scheduler_type,name,encode_config,save_config}`提供共同管理边界；
`restore`读取真实StorageEndpoint后重新校验。公共BuiltinScheduler枚举让宿主将真实
调度context传入既有策略，未注册Base或用空操作冒充调度器。

18条映射为balance leader/region/witness、evict leader/slow store/stopping store、grant hot、
balance hot、grant leader、label、random merge、scatter range、shuffle hot/leader/region、
transfer witness leader、evict slow trend、balance range。slow store同时创建既有disk和
network策略，完整保存网络字段和disk快照；沿用既有宿主串行配置协调契约。range变体
装有105真实策略和104真实持久队列，新增内部from_json不保存或重分配job ID。
没有新增依赖、features、transport或runtime。细节及宿主边界见
[registry说明](../../../pkg/schedule/schedulers/registry.rs.md)。

Go init的scatter参数为start/end/name，注册层转换到已有模块的name/start/end接口；
hot真实名称采用baseHot的balance-hot-region-scheduler，未沿用相邻配置模块的不同常量。
transfer witness忽略decoder且默认encode为null，保留Go Base行为。非空hot JSON使用
兼容优先级/空rank基线；零batch恢复各自默认值。默认disabled为本地owner快照，宿主
显式save/restore；range仍只编码jobs数组，符合Go EncodeConfig。
重复注册用领域错误替代Go log.Fatal，不退出宿主进程。未知名称/类型、坏配置和I/O失败
返回错误；名称最长匹配及scatter-range历史兼容别名保留。

## 固定输入与测试

[fixture](fixtures/task_106.json)冻结18条有效类型/args/name/配置字段及8条非法args。
范围键按Go keyutil hex编码；grant-hot保留输入peer顺序并在尾部补leader，不能排序。
range固定时间为Unix0，保留job0/pending/leader-scatter/tikv/1h及[a,z)。不忽略任何错误、
状态或操作差异；显式随机index=0、shuffle恒等控制Go不稳定抽样顺序。

独立显式注册的4个有效测试：

- task_106_behavior_cases：18类args创建，固定名称/关键默认和范围字段，slice decoder，
  真实MetaStore保存、重新构造、逐字节配置一致；range保留job字段，witness为null。
- task_106_errors_and_names：8个非法args，重复/局部注册冲突、未知/plugin、最长名称及
  历史别名，非法batch/hot维度、空散射名称。
- task_106_defaults_and_storage_faults：真实MemoryStore的read/transact I/O边界注入
  Unavailable，失败保存保持旧持久值，失败读取报错，恢复后成功重试；disabled保存/恢复，
  缺失/损坏名称配置、3类零batch回填、legacy hot默认恢复、坏JSON/形状、未知字段忽略。
- task_106_factory_policy_executes：真实BasicCluster、3个store、1个region及3个voter、
  RuleManager/配置；store1暂停transfer-in，factory和restore得到的shuffle策略各生成
  1个真实LEADER|ADMIN操作，恰有一步TransferLeader 1→2。时钟固定1000秒，无mock调用计数。

指定只读测试关联已检查：TestReloadSchedulerConfigWithoutSchedulerDoesNotPanic位于Go
Controller层，证明缺失调度器应返回错误，106对应未知restore，未声称移植Controller；
TestShuffleLeader证明创建及真实leader/admin操作，106从factory与restore实际执行；
TestRejectLeader验证label/balance/evict的过滤与操作，TestRemoveRejectLeader验证evict
移除配置，分别由前序97/70/75/76行为覆盖，本任务验证其真实工厂；
TestShuffleHotRegionScheduleBalance验证placement开/关下shuffle热点调度，由100覆盖，
106验证真实shuffle-hot factory和配置。未声称执行任何这些Go函数或完整重放其全部步骤。
包级全量验证继续运行这些既有Rust策略行为测试。

## 红灯、验证与只读审计

initial保存缺API的编译红灯exit101；focused保存测试装配编译错误；focused2保存实际
行为断言红灯：scatter入口参数顺序错误及grant-hot期望错误。对照只读Go setStore修正
冻结期望为[2,3,1]；只在注册层转换scatter顺序。focused3/4、expanded保存迭代绿灯。
[red_behavior.py](logs/task_106/red_behavior.py)随后分别将默认batch4故意改5、将shuffle
返回type故意改label，同一4个测试各以断言失败exit101识别；[red-results](logs/task_106/red-results.json)
明确记录。finally恢复原始源码；脚本exit0表示成功识别两个错误实现，不是错误实现通过。

[final.json](logs/task_106/final.json)记录最终确切命令、退出码、动态槽位与绝对target路径。
[完整日志](logs/task_106/final.log.gz)及[test-counts](logs/task_106/test-counts.json)保存有效测试数。最终聚焦4、包168、workspace1717通过（其中2个为compile-fail文档测试），
workspace既有ignored=1，failed=0；全部命令退出0。
通过命令按顺序为：

```sh
cargo fmt --all
cargo test -p astersql-pd-schedule-schedulers --locked --lib task_106 -- --nocapture
cargo test -p astersql-pd-schedule-schedulers --locked
cargo metadata --format-version 1 --no-deps
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
```

各阶段通过[run.py](logs/task_106/run.py)原子mkdir动态领取共享槽位；实际槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
最终阶段CARGO_BUILD_JOBS=1、RUST_TEST_THREADS=2；所有Cargo和子进程继承该设置。
正常及异常路径先等待子进程退出再释放自有锁，缓存保留。未使用其它target-dir。

[只读审计](logs/task_106/readonly-check.json)确认1101个Go/模块文件与开始内容哈希一致，
含未跟踪文件清单；[开始快照](logs/task_106/go-before.json.gz)可复核。Go和模块无新增差异，
两份只读plan未改。git diff --check通过；其它已完成任务的既有删除未纳入提交。
