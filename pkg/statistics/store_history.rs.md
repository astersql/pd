# 负载历史采样与回收

`StoreHistoryLoads` 对应 Go store_load.go 的同名类型与内部 storeHistoryLoad。
公开 `Loads = [f64; DIM_LEN]`、`HistoryLoads = [Vec<f64>; DIM_LEN]`，维度为
byte/key/query/CPU；使用现有 statistics-utils RWType 与 core-constant ResourceKind。
不依赖协议、网络、调度、持久化或异步运行时。

- `new(duration, interval)` 接受 Duration；默认300秒/30秒。非零间隔容量为整除结果；
  间隔0且duration非0固定10槽；duration0总是零容量。容量超出地址空间时panic。
- `add(ID, rw, kind, point, now)` 使用宿主显式单调时间（Duration），首次立即采样。
  与上次成功采样时间差小于interval则忽略，恰好相等接受；时间倒退也忽略。
  未采样项保持空；首次成功时分配完整零填充环，随后覆盖下一个槽位。
  游标有界，不累积溢出；返回物理槽位顺序，不按时间重排。
- `get` 返回深拷贝快照，修改结果不影响容器。未知RW/resource索引panic，与Go数组一致。
- `gc` 复用 core `is_removed`，保留输入中非Removed节点，清除所有分区的缺失/Removed ID。
  不扩展为健康过滤：Removing 和 physically_destroyed 本身不删除历史。
- `add_store` 使用当前 StoreInfo 并检查 `is_removed`；对应 Go 汇总调用方的防重建保护。
  后续汇总必须使用此入口或相同 guard。`add(ID)` 保留 Go 底层接口契约，不持有永久墓碑；
  节点恢复且接收新点时允许建立全新历史。
- `update_config` 任一配置改变时清空所有分区并返回true；配置相同保留环与采样时钟。
  Rust 原位更新代替 Go 返回新指针，要求可变借用。容器由调用者串行使用或外部加锁。

与Go不同，使用非负Duration配置与固定数组点，排除负容量/长度不匹配状态；
时钟由宿主供应，首次时间0也可以采样。无后台任务、系统时钟或私有runtime。

独立测试显式登记在 lib.rs；[任务证据](../../docs/rust/state/task-12.md)记录只读Go运行、
源码推导扩展、红灯与最终验证。
