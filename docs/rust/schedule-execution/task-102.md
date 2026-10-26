# 任务102：区域随机扰动调度

状态：已完成。

批次101真实依赖通过提交 `c9801084d`、[证据](task-101.md)、canonical cluster、
store/region filters、placement manager 和 peer operator factory 核实；前置包级153项及
workspace1702项测试证据齐备。没有并行任务或子代理，两个 plan.md 始终只读。

采用用户后续明确授权：只读 Go 源码/测试推导 + Rust 真实行为及故障验证，替代缺失的
Go 运行 oracle；没有执行 Go、failpoint 或改写 Go/模块文件，没有声称运行差分。
[provenance](logs/task_102/provenance.json)冻结 HEAD、计划基准、来源和 fixture SHA-256，
runtime_oracle=null。准入 oracle-index 已检查；未将其中无关运行输出当作此任务证据。

## 生产路径及契约

[shuffle_region.rs](../../../pkg/schedule/schedulers/shuffle_region.rs) 和
[模块说明](../../../pkg/schedule/schedulers/shuffle_region.rs.md) 提供配置、按角色选择、
调度、角色更新及持久化/重载 API。lib 显式注册生产模块和独立测试；无需修改 Cargo
依赖或锁文件。复用 ShuffleLeaderConfig 范围解码、EvictLeaderContext/Result、真实
BasicCluster 索引、RegionReplicatedFilter、PlacementGuardContext 和 PeerOperatorFactory。

Go shuffle_region.go 固定 source shuffle、每个 source 的 follower→leader→learner
优先顺序、pending/down/replication 筛选、首个选中区域后不重试 source、排除现有 peers、
Low move-region/special-use/placement target、保留 peer role、REGION kind 和 Low 优先级。
shuffle_region_config.go 固定合法角色检查及保存失败回滚；init.go 固定 CLI 默认全部角色。
Go scheduler_test.go 的 TestShuffleRegion/TestShuffleRegionRole 是普通/learner 搬迁源码
依据；core/region_tree.go 的 RandomRegions 是空范围与完整包含语义来源，未运行这些测试。

## 测试及失败证据

[fixture](fixtures/task_102.json) 固定26个源码推导案例，classic/conf-change-v2各执行一次，
共52条真实行为案例。覆盖 follower/leader/learner/all/empty/unknown stored roles、
legacy learner 拒绝、pending/down、缺 region/target、special-use/offline/disconnected、
复制数不足、ADD_PEER真实预算耗尽、分配失败、legacy/rule placement 多样性保护、
范围外/部分包含/完整包含/空范围、随机末目标、反向 source shuffle。

归一化：store 按ID排序，source shuffle 用显式 identity/reverse，随机首/末索引；
region 固定ID1/[a,b)，初始peer ID及store1..3、分配ID10、时钟10^12ns。不忽略错误字段。

3项显式测试：

- `task_102_behavior_cases`：52条，校验 typed allocation 错误、分配次数、desc、region、
  kind、Low、add/remove来源目标与peer ID、learner不promote、leader移除的transfer；
  对每步实际获取心跳命令、应用peer/leader变化、校验完成、confver及最终peer role，
  最终operator成功；原region保持不变。
- `task_102_config_and_limits`：5组严格限流边界；URL/hex范围编码、JSON往返、非法roles
  更新原子拒绝、坏JSON/范围/CLI输入、JSON默认无角色。
- `task_102_config_storage_faults`：真实MemoryStore/StorageEndpoint保存和重建恢复；
  仅I/O边界注入Unavailable，验证persist/update_roles/reload失败保留旧配置；非法role
  更新拒绝，坏JSON reload保留旧值；成功role替换及空列表保存恢复。

initial：未实现API的编译红灯，Cargo exit101；仅作缺失接口记录。
focused：修正测试装配中的StringSlice/priority/kind接口；focused2：补齐显式max_replicas=3；
focused3：修正placement拒绝输入（目标与待移除source同zone不会降低多样性，应与保留peer
同zone）；没有据此修改正确生产行为。focused4：3项/48行为案例通过，后补2条rule案例。

[red_behavior.py](logs/task_102/red_behavior.py) 将实际Low设置暂改High，运行同一个
`task_102_behavior_cases`，priority断言失败，0通过/1失败，Cargo exit101；脚本确认预期
失败后返回0，并在finally逐字节恢复实现。最后完整验证在恢复后重新运行。

## 最终验证

所有Cargo命令和脚本均由[run.py](logs/task_102/run.py)原子mkdir动态领取共享槽位。
所有阶段实际使用 **槽位1**，CARGO_TARGET_DIR 为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。记录owner计划/任务/会话/PID；
正常/异常/中断先等待或终止并回收子进程再释放自己的锁，保留编译缓存。
[final.json](logs/task_102/final.json)保存确切顺序命令及退出码，完整stdout/stderr见
[final.log.gz](logs/task_102/final.log.gz)，`&&`串行链确保每项exit0。
最终设置CARGO_BUILD_JOBS=1和RUST_TEST_THREADS=2。

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 只保留本任务Rust差异 |
| cargo test -p astersql-pd-schedule-schedulers --locked --lib task_102 -- --nocapture | 0 | 3项通过；52行为案例 |
| cargo test -p astersql-pd-schedule-schedulers --locked | 0 | 156通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 | workspace元数据通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过；既有cpu_usage弃用warning |
| cargo test --workspace --locked | 0 | 1705通过/0失败/1既有忽略，含2文档测试 |

[audit.json](logs/task_102/audit.json)及起始hash表确认1103个Go/模块和只读plan文件内容
哈希未变，无新增Go/模块文件，相关Git状态为空；git diff --check退出0。Cargo.lock未变。
证据采用明确授权的源码推导口径，全部必需验证通过。仅提交本任务生产、测试、局部注册、
模块说明及证据；既有其它编号任务删除不纳入提交。完成后删除任务102编号文件。
