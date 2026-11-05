# 任务050：批量预热GC缓存

2026-11-06，基准206f977df。[完整回执](task-050-evidence.json.gz)保留两阶段red、通过、重构回归、命令/退出码、Go原例、来源指纹及Go只读守卫。

GCWarmup以独立Null singleton保留第一个后台槽位，其余固定worker通过已到达page组装至多60个缺失scope；缓存命中和已有flight不占读槽。
16页非阻塞提示队列满时丢提示，首个完整snapshot在worker中补齐目标并剔除失败metadata尝试的候选；之后不因新page扩充初始目标。
短调度锁不跨manager锁和I/O；前台与后台复用同一generation flight表、读锁、真实LoadGCSafePointPairs及唯一load_claimed_gc_batch发布实现。
读锁持有至发布结束，写入不能被迟到预热覆盖。scope完成由不可变终态证明，即使后续缓存失效仍完成；joined由固定worker共同按100ms期限检查，不创建每scope goroutine。
所有已领取flight都有RAII取消清理，run取消后join固定worker，关闭输入并释放候选/快照。旧generation不能重绑到新generation。
失败scope保留未完成，51继续负责有期限重试和过时目标退休；不把未接入的重试或实际宿主任期后台装配记作本项完成。

验证：

- 批量接口red：实际etcd状态未发布完成项，0!=2，1 failed、exit101。
- campaign red：空run返回后真实停止etcd，Null读取报Unavailable，1 passed/1 failed、exit101；真实campaign填充后通过。
- 最终`cargo test --locked -p astersql-pd-gc --lib mcs_050_ -- --nocapture`：6 passed、0 failed、0 ignored。
  120-op真实etcd限额下，256 scopes分为60/60/60/60/16；已缓存256项可在一次组装完成，损坏scope单独失败、修复重新加载。
- 完整snapshot补齐超过16页队列容量的256项，加Null共257项真实缓存状态。
  实际停止etcd后Null暖值仍为40，不通过重新读存储伪造预热。
- TCP闸门持有真实批量响应时，前台同scope等待共享flight，writer仍不能提交；放行后writer推进到60，最终缓存保持新值。
  该闸门冻结整个连接，不声称证明了同连接的无缓存前台请求能够越过网络冻结；Go原例另覆盖独立前台容量。
- 实际取消批次owner后，joined终态立即失败，flight表清空，放行后重新加载成功；满输入队列下取消worker池，所有worker退出、输入关闭、Arc释放。
- 同源Go Classic和NextGen各运行11个顶层原例、16个pass节点，叶例数量见回执；选取原例没有failpoint Enable/EnableCall依赖，使用现有二进制及只读sandbox。
  page case同为IDs100..355、256个初始scope、每批至多60个缺失项；原例内部断言证明其结果，不伪造Go未打印的内部响应。
- GC全包57 passed、0 failed、0 ignored；metadata、fmt check、workspace check、git diff --check通过。
  完整workspace test继续由外层任务24集中运行。
- 所有Cargo阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1，退出后释放自有锁并保留缓存。
- Go及模块1101项首尾SHA/存在性一致；未写Go、模块、make/failpoint生成注入；所有plan.md及来源史诗保持原样。
