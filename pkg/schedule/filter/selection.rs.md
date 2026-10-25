# 基础候选筛选

`selection.rs` 是 `astersql-pd-schedule-filter` 的 canonical 实现，仅包含 Filter
契约、ExcludedFilter 和三类 store selection。配置借用 canonical
`SharedConfigProvider<Version = V>`，store 使用真实 `Arc<StoreInfo>`，诊断直接
使用 `astersql-pd-schedule-plan` 的 Status、Resource 和 Collector。没有复制 core、
配置或 plan 状态，也没有 protobuf/server/runtime 依赖。

输入顺序与重复项保留；返回值克隆 Arc，保持 store 对象身份。Rust 拥有返回 Vec，
不表达 Go 空 slice 的 nil/非 nil 区别。空过滤链对 source/target 全通过，对
unavailable target 全不选。每个 store 按过滤器顺序短路；内部保留首个拒绝的索引
和完整 Status。普通筛选使用 collector 当前 step，unavailable 强制 step 2，
每个被拒 store 只产生一个 diagnostic。collector 的 base 不会被修改。

ExcludedFilter 捕获 source/target 两个独立集合，只按完整 u64 ID 匹配。空集合对应
Go nil/empty map；拒绝为 101/空 reason，通过为 0/空 reason。FilterType 保留 Go
整数身份，Excluded 为 0。计数器和 Prometheus 生命周期由任务 11 迁移，当前实现
没有增加指标或过程全局状态。

这是纯领域筛选，没有 I/O、时钟、随机或持久化操作。错误状态原样交给 canonical
collector。错误装配（region step 1 接收 Store resource）沿用原 plan 类型断言的
panic；显式 unavailable selection 则将资源放到 step 2。Rust 静态类型避免空
store 指针输入，没有新增吞错或回退路径。

Go 运行 oracle、8 个前缀行为案例、真实 core/collector 的边界与故障测试，以及
4 次错误变更检出证据见[任务证据](../../../docs/rust/schedule-execution/task-005.md)。
StoreStateFilter 自身留给后续任务：Go 额外观察到的 removed-first=401 并不计入
本任务 Rust 差分案例。first-excluded 案例的后续过滤器在短路后不可达，Rust 使用
第二个真实 excluded filter，并单独检查首拒绝索引；没有用返回固定状态的 mock
替代尚未迁移的状态过滤器。
