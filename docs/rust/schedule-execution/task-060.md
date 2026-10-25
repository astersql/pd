# 任务060：散射操作生成与分组记账

状态：已完成。

前置59：提交 `0dc510f1f`，证据 [task-059.md](task-059.md)，已实现真实
目标选择与工厂消费；本次包级测试同时重跑其5项测试。
本任务生产路径 [scatter_operator.rs](../../../pkg/schedule/scatter/scatter_operator.rs)，
[边界说明](../../../pkg/schedule/scatter/scatter_operator.rs.md)。API：
`create_scatter_operator`、`ScatterCounts::record_distribution`、`ScatterOperatorError`。
operator 从 dev-dependency 移至生产 dependency，没有新增依赖或锁文件差异。

用户授权解除 Go 运行 oracle 准入，采用**只读 Go 源码/测试契约推导 + Rust 行为验证**。
未运行 Go，不宣称 Go/Rust 运行差分。来源与 SHA256 见
[provenance.json](logs/task_060/provenance.json)，固定输入和期望见
[fixture](fixtures/task_060.json)。Go outputs/binary hash 为 null；没有时间或随机归一化，
目标 leader 显式给定。oracle-index 的历史 internal failure-accounting 用例不作为本次
admin 运行证据。Go TestMain 只是环境入口；TestScatterRegions/ScatterCheck 提供单区
调用契约；TestStoreLimit 包含 controller admission，本任务仅验证 skip 对真实步骤的影响；
TestSomeStoresFilteredScatterGroupInConcurrency 不验证结果，不能视作覆盖。

来源 `region_scatterer.go:825–876`/`Put` 实际记账语义：成功写目标；无操作写当前布局；
错误写原布局。任务目的“失败不污染计数”指未生效目标不被计入，不能解释为取消 Go
既有的原布局记账。`create_operator.go:newScatterRegionOperator` 规定 add-light、
可选 remove-light、ADMIN、强制 leader；`builder.go` 允许转 leader 到中间副本后移除源，
最后转到目标。测试验证最终目标与真实步骤属性，不要求从源直接转到目标。

独立测试显式注册，共3项有效测试、0忽略：

- `task_060_behavior_cases`：6个冻结fixture案例，移动/skip移动/相同布局、
  allocator I/O边界失败、无效数量、无目标leader；同时验证组隔离、tikv/tiflash隔离、
  peer/leader计数、metadata/HIGH/ADMIN、真实AddLearner/TransferLeader/RemovePeer，
  ID=900、输入快照不变。3成功/无操作，3错误。
- `task_060_accounting_boundaries`：leader=0无变化、重复记录累加、缺失store不计、
  无源leader类型错误且不记账。4个边界场景（重复记录执行2次）。
- `task_060_invalid_roles_and_identity`：witness改变、learner角色改变、map/store不匹配、
  目标store缺失工厂错误，4个错误案例。全部只记录原布局。

共14个不同语义场景。无MetaStore操作，不声称持久化故障覆盖；allocator失败通过
真实工厂回传 `BuilderError::Allocation`，断言原布局记账和目标计数保持不变。
无新全局状态、Tokio运行时或协议依赖。内部 seeded-state、CPU策略与批处理不在本入口范围。

失败—通过证据：

- [red-api.log](logs/task_060/red-api.log)：退出101，缺少本任务API。
- [red-accounting.log](logs/task_060/red-accounting.log)：错误分支变异为记录目标；
  编译成功，1通过2失败，allocator失败原计数actual=0/expected=1，目标store错误亦被检出。
  恢复原布局记账后同组测试通过；变异未保留。
- [focused.log](logs/task_060/focused.log)：退出0，3通过，6个fixture逐案例打印。
- [package.log](logs/task_060/package.log)：退出0，8通过，含前置59的5测试；doc-tests=0。

Cargo 通过 [run.py](logs/task_060/run.py) 动态原子mkdir领取槽位，锁中记录任务/计划/会话/PID，
子进程退出后释放；异常时终止并等待子进程组再释放。每次Rust修改后先cargo fmt --all。
实际命令、退出码、槽位、绝对target均保存在各同名JSON，缓存保留。

最终基线均退出0：

| 命令 | 结果 |
| --- | --- |
| `cargo metadata --format-version 1 --no-deps` | workspace解析通过 |
| `cargo fmt --all -- --check` | 格式通过 |
| `cargo check --workspace --locked` | 全仓检查通过 |
| `CARGO_BUILD_JOBS=1 cargo test --workspace --locked` | 1532通过、0失败、1既有忽略 |

全仓 [workspace-test.log](logs/task_060/workspace-test.log) 与
[counts.json](logs/task_060/counts.json) 包含1530项非doc测试及2项compile-fail doctest；
既有 `tso_processes` 要求独立进程二进制，默认忽略，不计入有效通过。
现有 server/core_region_adapter.rs:134 的 cpu_usage deprecated 警告保持原样。
全仓完成后仅删除冗余clippy allow属性，未改行为；再次通过fmt、聚焦3项、包级8项和workspace check。

实际各阶段均为**槽位1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，
完成后已释放本任务锁，缓存保留。没有使用其它target目录。

[审计结果](logs/task_060/audit.json)：1101个Go/模块文件清单、SHA256和空Git状态保持不变；
两份只读plan.md与HEAD逐字一致；远程Cargo.lock条目不变，锁文件无差异；
链接与git diff --check通过。没有范围外Rust格式化差异。原有其它编号任务删除保持原样，
仅提交本任务实现、测试、接线与证据；任务文件完成后按要求删除，不纳入代码提交。

日志仅清理行尾空白与文件末多余空行，未改断言、错误或运行结果。
