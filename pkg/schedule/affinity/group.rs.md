# 分组值模型（批次 31）

`Group` 是唯一分组持久化值，`GroupState` 展平组字段并排除两项调度许可。
`Availability` 表达 available/degraded/expired；`RuntimeGroupInfo::availability`、
`set_availability`、`state` 接收调用方提供的 Unix 秒，严格使用 `now > deadline`，
重复 degraded 不刷新 600 秒期限，expired 不接受 degraded，available 可以恢复。
`state` 克隆组值，按范围数、分布完整性和区域计数计算 pending/preparing/stable。
`validate_group_id` 只接受 1–64 个 ASCII 字母、数字、短横线或下划线。

本批次只提供值与状态投影，不提供 Manager、区域缓存、版本提交、范围事务或后台循环。
后续批次应扩展此 canonical crate；region_count 是投影输入，不是另一个区域缓存。
实际 Go HTTP oracle、既有测试、失败和基线输出见 docs/rust/schedule/affinity/31-证据.md。
