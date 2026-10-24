# Collector 与选项

PlanOption 替代 Go 的 Option 闭包，公开 SetStatus、SetResource、SetResourceWithStep
与 apply。顺序可观察：先更新步骤，再设置该步骤资源；后续选项覆盖先前选项。
Collector::new 拥有 base_plan；collect 调用 clone_with 后按 status.is_ok 分组。
get_plans 返回拥有的快照列表：成功组在前，每组保持插入顺序；修改输出不改变内部列表。
空 Collector 返回空 Vec。Go nil receiver 的 no-op/nil 由调用方 Option<Collector>
表达；Rust 没有无效的 &mut self。具体领域计划而非 any/协议对象作为收集边界，
后续扩展其他计划应在真实需求出现时增加类型边界。

独立 plan_collect_test.rs 在 lib.rs 显式登记，验证步骤保留、选项、克隆隔离、
panic/未知步骤、成功优先与输出隔离；证据在 docs/rust/schedule/plan/7-证据.md。
