# Store 与操作影响量

`StoreInfluence` 保存有符号的 region/leader size/count、witness count 和 step cost。
`resource_property` 复用 core constant 的 `ScheduleKind`；未知 leader policy 返回 0，
region 非 count policy 返回 size，witness 忽略 policy，未知 resource 返回 0。
聚合使用 Go int64 的 wrapping 语义，只合并 `TYPE_NAME_VALUE` 的四种成本；
直接成本读写仍保留未知 type，缺失条目返回 0。

`adjust_step_cost` 复用真实 core `is_empty_region` 和 storelimit 常量：size >20
收费1000；其它非空 region 收费200（仅 add/remove peer）；空 region 不收费。
没有成本表条目的非空 region 请求属于程序错误，与 Go slice 越界一样 panic；
它不是业务 I/O 失败。该领域不访问存储或协议，也没有后台运行时。

`OpInfluence` 使用一个 RwLock 保护整个 map。`get_store_influence` 会建立缺失 store，
返回 owned value；调用方通过 `update_store` 原子修改 store。回调不得重入同一容器，
panic 会遵循标准 RwLock poisoning；正常业务调用不应在临界区做外部 I/O。
`snapshot` 深拷贝成本 map，避免 Go 指针/map 别名在 Rust 中变成数据竞争。
`add` 先取得源快照并释放源锁，再取得目标锁；自加和互相聚合都不会双重持锁。
快照只保证单次读取原子性，不保证跨两个容器的事务一致性。

来源为只读 `influence.go`、`store_limit.go`、`region.go`，以及
`operator_test.go::TestAdjustStepCostUsesRegionKeys`。用户已解除 Go 运行证据准入；
固定输入/期望属于源码推导，未运行 Go 或声称运行差分。详见
[任务17证据](../../../docs/rust/schedule-execution/task-017.md)和显式注册的
`task_017_test.rs`，覆盖成本阈值、空 region、错误索引、全部资源属性、跨 store
聚合、抵消、整数溢出、快照所有权与并发读取。
