# 散射管理响应

`scatter_admin.rs` 提供单区域、批量选择、显式范围与 ID 管理动作，以及
`build_scatter_regions_resp`。各动作借用宿主可选的 `&mut Coordinator`，复用其
scatter counts、operator controller、checker pending 队列。现有只读 Handler 不能
借出可变 counts，因此这里沿用输入分派已有的可变 Coordinator 函数入口；没有第二份
Handler、控制器、计数或私有 runtime。输入分派委托这些函数，避免重复业务实现。
现有 CreationContext 只承载宿主规则、hot/affinity、显式时间、取消和分配器边界；
响应结构是领域类型，JSON 和 HTTP 状态仍由既有输入分派/传输适配层处理。

`add_scatter_regions_operators` 先校验 bootstrap；仅两端均非空时解析 hex 范围并
优先于 IDs，否则保留 ID 路径。显式 `scatter_regions_by_range` 先解析两端，再检查
bootstrap，并允许空键表示无界。显式 ID 路径不解析 hex。retry i64 等效收敛到
[-1,30]，随后委托 canonical batch；负数不执行尝试，失败保留类型化 map、pending、
suspect 和取消结果。suspect 转入既有 checker 队列。

单区先检查存在与 hot 状态，调用真实 scatter_one；空操作成功。生成操作按 Go
addOperator 直接准入，确认并归档替换项。不同于此前分派中的 limited admission，
这遵循 handler.go 的单区管理路径；批量保持前序 canonical batch 的准入契约。
错误沿用 CreationError 的 bootstrap、region、hex、scatter、builder/admission 原因。

成功 Scatter 调用计入 processed，包含无操作和其后准入失败；failures 按 ID 去重。
比例精确采用 `100 - 100*失败数/(processed+失败数)` 的整数取整，不写成
`100*processed/总数`；例如 2/1 得67。无失败（包括0/0）返回100。内部用 u128
计算，避免 usize 乘法/加法溢出。领域响应只保存百分比，完整结果另保留故障信息。

只读来源：handler.go:654–726、1188–1236；region_scatterer.go:481–591，及
既有 scatter_split_test.go 的 TestScatterRegions。用户授权以源码推导和 Rust
真实行为/故障替代缺失 Go 运行 oracle；没有 Go 运行差分声明。测试控制时钟及
分配器 I/O，使用真实 Coordinator、BasicCluster、RuleManager、MemoryStore 和
operator 实现，不用 mock。运行证据与边界见
[任务125](../../../docs/rust/schedule-execution/task-125.md)。
