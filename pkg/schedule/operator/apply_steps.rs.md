# 真实操作应用测试支持

`apply_steps.rs` 通过 lib.rs 的 `#[cfg(test)]` 注册，提供 crate 内测试使用的
`apply_operator_step` 和 `apply_operator`。生产库不会编译该模块；没有第二份生产
调度器、运行时、持久化层或 wire adapter。

行为依据 `test_util.go` 的只读源码（基准 836fdc7cf3ef34f7eabc9d051418995bb97efabe），
不是 Go 运行差分。fixture 的 SHA256 与证据见
[task-038.md](../../../docs/rust/schedule-execution/task-038.md)。

单步先调用真实 Operator::start/check，克隆真实 RegionInfo 并应用 RegionOption。
支持 TransferLeader、AddPeer、AddLearner、PromoteLearner、RemovePeer；保留 epoch、
统计与 pending/down 报告，完成在后一次 check 确认。基础步骤创建普通非 witness
peer，promotion 移除再追加。请求 witness 的 add 不会自动修正为 witness，后续
调用会因已有 peer panic；这是来源工具的边界。

尚未完成的 joint、witness、split、merge 步骤触发 `Unknown operator step`；已完成
步骤由真实 check 跳过。Rust OpStep 为封闭 enum，未来新 variant 同样进入拒绝分支。
重复 add、非法 promotion、移除 leader 等按来源 panic；已经不存在的 remove 通常
被 check 视为完成，不会到达 helper 的缺失 peer 断言。缺失 transfer 目标产生 None
leader。此工具要求可执行、可完成的输入，和来源一样不承诺为持续 pending 步骤
设定人工循环上限。

整条操作先在本地 snapshot 循环，到终态后一次发布到 BasicCluster 的真实 root 和
role subtree indexes，然后按最终和原始成员两轮刷新 store。这样既刷新新增 store，
也清零已移除 store；中途步骤 panic 不发布 snapshot。store 刷新复用 canonical
update_store_status，再模拟 mockcluster 的 100 GiB 容量、旧 region_size 派生 used/
available、heartbeat 更新。共同成员保持两轮刷新，未成员 store 不变。显式传入
`now_unix_nanos` 只控制 mock heartbeat 时钟边界，不改 Operator 内部时钟。

没有 MetaStore I/O，故无持久化后端或 I/O mock。测试使用真实 core、Operator、
OpInfluence 和 BasicCluster，覆盖状态与影响前后；记录 panic 属于预期错误验证。
