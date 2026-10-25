# 可疑范围复查

`SuspectRanges` 复用宿主持有的 `TTLString<[Vec<u8>; 2]>`，生产宿主按 Go 配置
一分钟 GC、三分钟 TTL，并在 worker 退出后 shutdown cache。`add/pop/clear`
分别对应 Go `AddSuspectKeyRange/PopOneSuspectKeyRange/ClearSuspectKeyRanges`。
范围标识使用 canonical keyutil 的十六进制编码；重复范围刷新 TTL，重叠范围保持独立，
pop 顺序未定义。该行为不是区间 union，也不是 FIFO。

`check_once` 在宿主锁保护的真实 `BasicCluster` 快照上扫描最多 1024 个 region。
有限 end 之前仍有数据则将最后 region 的 end 到原 end 重新排队；空扫描消耗原范围。
ID 进入同一个 `CheckerDispatcher` pending TTL，使用 `check_len=false` 绕过数量限制。
巡逻按已有 pending 路径执行真实 checker 和 operator admission。此函数不另建 region registry、
operator controller 或 pending 集合；clear 只清理范围队列。

`run` 在宿主 executor 每 100ms 调用一个 async callback，callback 获取宿主现有锁后调用
`check_once`；不在整个生命周期持有 cluster 的不可变借用，允许心跳在 tick 之间更新。
callback 的错误类型原样返回。域扫描与 cache 操作自身无可返回错误。
停止须 cancel 后 await，已进入的 callback 等待完成，闲置 worker 立即被唤醒；
直接 drop future 不算优雅停止。库不创建 runtime、不安装全局信号。

来源为只读 `checker_controller.go:470–533`，依据用户授权使用源码推导，未运行 Go oracle。
保留 Go 对 requested end 的字节比较：空 end 不会补排下一批；canonical core 把 Go nil
终点映射为空 Vec，不能表示显式 nonnil 空切片的指针差异。fixture 明确记录该边界。
错误和操作结果不作归一化；时钟由 Tokio 暂停时钟控制。

验证与来源哈希见 [task-057](../../../docs/rust/schedule-execution/task-057.md)；
测试独立位于 `task_057_test.rs`，真实巡逻装配复用 `task_056_test` 的 fixture，
只在测试中开放最小宿主辅助入口。
