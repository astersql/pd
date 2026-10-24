# BalanceSchedulerPlan 资源快照

对应 Go balance_plan.go 的资源与克隆部分；汇总留给批次 8。
公开 BalanceSchedulerPlan 的 source/region/target 为 canonical core 类型的
Option<Arc<_>>，status 为拥有的 Status，step 为 i64。new/default 为步骤 0、OK、无资源。
get_step/get_status/set_status、set_resource/set_resource_with_step/get_resource
对应原方法。Resource::Store/Region 的 None 表示 Go 的有类型 nil；错误 variant
在步骤 0..2 panic，未知步骤忽略资源但保留步骤。get_resource 对未来或未知步骤返回
0，对已到达步骤的缺失资源 panic（真实 Go 调试已确认）。

clone_with 仅保留严格早于 step 的资源，再依序应用 PlanOption。
标准 Clone 是普通值快照，供 Collector 输出复制使用；诊断分支须用 clone_with。
Arc 保留 Go 资源指针共享语义；替换字段相互隔离，不承诺隔离 core 内部共享统计量。
Status 使用拥有的字符串副本；Go 原指针原地修改可传播，此 Rust 值模型刻意排除这种
别名修改方式，set_status 的替换语义一致。不依赖 proto、存储、运行时或后台任务。
