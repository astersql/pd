# 任务052：把发现和预热绑定到GC领导任期

2026-11-06，基准66f90c34b。[完整回执](task-052-evidence.json.gz)保存red/green、命令/退出码、来源指纹、Go双内核原例和Go守卫。

复用43/132现有唯一generation、host子取消令牌和TaskTracker。显式start_metadata_discovery将49索引和50/51初始预热绑定到当前任期，返回同一索引；被动Classic默认行为保持，不声称独立MCS/NextGen装配完成。
KeyspaceManager只公开同一host storage/codec的clone，不建新MetaStore或领域实现。索引的load concurrency在每次尝试开始时取已验证快照，warmup并发数固定于campaign创建。
后台根任务在generation claim锁下登记到现有tracker，retire在同锁下取消并关闭tracker，避免join后补登记任务。
预热成功后索引继续运行；退役取消、join两个工作流后才清空缓存并发布新generation。旧角色延迟清理不触及新generation。
重复启用和非法并发数返回错误，退役句柄不能重新启动工作；没有私有runtime、全局任务或信号。

验证：

- red：manager公开了索引但没有运行，真实metadata readiness未完成，1 failed、exit101。一项格式错误单独保存，不计行为red。
- `cargo test --locked -p astersql-pd-gc --lib mcs_052_ -- --nocapture`：2 passed、0 ignored。
  256条实际metadata被索引，Null加256项实际缓存完成，非法/重复配置被拒绝，索引配置可更新。
- 实际TCP闸门持有旧代际50/40响应，持久值另行推进80/70；换代在不释放旧响应时即取消并join旧I/O，旧flight及tracker为空，新缓存先清空。
  放行后新代际只发布80/70；延迟执行旧cleanup后，真实停服etcd仍可读到新暖GC值70，证明没有清掉新缓存。
  同WAL重启并构造fresh SDK/manager仍读70；host取消后索引停止，旧manager不能补启动任务。
- `cargo test --locked -p astersql-pd-runtime --lib mcs_052_ -- --nocapture`：1 passed。
  两个真实节点中只有serving node发布manager；在该132实际任期manager上显式启用索引。停止旧节点后其索引退出，新节点换主后可启用自身新任期，最后停服无残留工作。
- `cargo test --locked -p astersql-pd-runtime --lib mcs_132_ -- --nocapture`：2 passed，保留value/lease/revision真实fence和任期资源所有权。
- Go Classic/NextGen各6项同源原例通过，覆盖入场gate、旧cleanup隔离、锁前取消、全部批次取消和实际warmup runtime；所选调用链无需failpoint注入，使用现有二进制和只读sandbox。
- GC整包63 passed、0 failed、0 ignored；metadata、fmt check、workspace check和git diff --check通过；全workspace test按外层24集中执行。
- 所有Cargo阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1，子进程退出后释放锁，缓存保留。
- Go/模块1101项首尾SHA及存在性一致；没有写Go/模块、make/failpoint注入/生成整理；所有plan.md及来源史诗保持只读。
