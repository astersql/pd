# 任务019：ForceLeader计划

状态：已完成，待统一包级与workspace回归。任务102尚未完成。

从已收报告索引生成领域计划：分别计算incoming/outgoing投票集合，严格多数才有
quorum，处理Voter、IncomingVoter、DemotingVoter和Learner。已报告force leader的
Region跳过；commit merge先独立准备，普通阶段遇到未处理commit merge返回错误。
leader仍按raft新鲜度选择，允许其epoch低于索引中的最新元数据。

显式模式保留完整failed store集合；自动模式按所选leader元数据补齐失败/孤立peer，
已有incoming quorum时不额外列failed voters。产生任一ForceLeader计划时，其他预期
store也得到非nil的空ForceLeader，避免意外退出已有force leader。
记录实际受影响Region以及Go codec识别的meta/table集合；使用所选leader的范围，
不误用索引的另一个较新epoch范围。无效编码按Go返回“不属于表或meta”。

真实节点在报告收齐后通过同一控制器准备候选计划，保留在term资源中；新准入成功
时清除。server适配层负责实际protobuf转换。生成器不分配阶段step、不分发计划，
任务23接续；任务21还会补TiFlash清理优先级。不把完整恢复运行期判为完成。

## 验证与上游修正

[阶段命令、退出、原始日志、来源SHA](task-019-evidence.json.gz)。red编译后在
缺失实际ForceLeader计划的断言失败；补齐算法后通过。

核对Go changeStage完整尾部发现，任务16进入CollectReport时少推进一次step。
现已修正：recoveryStartStep=seed+1，初始当前step=seed+2；并补明确断言。
修正后重跑18个领域测试及真实任务16/17/19节点。Go差分的阶段递增断言也从
“不同”收紧为“恰好+1”，原始step分别保留，不归一化或隐瞒这个上游修正。

| 实际命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-unsaferecovery --lib mcs_01 -- --nocapture | 0 | 18通过：5本项+13前置相邻；step修正后再次通过 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_01 -- --nocapture | 0 | 7通过：1本项+6前置相邻 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_016_ -- --nocapture | 0 | step修正后重跑1相邻通过 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_017_ -- --nocapture | 0 | step修正后重跑1相邻通过 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_019_ -- --nocapture | 0 | step修正后重跑1本项，三种真实Go/Rust模式通过 |
| cargo test --locked -p astersql-pd-codec --lib table_test:: -- --nocapture | 0 | 2相邻通过 |
| cargo test --locked -p astersql-pd-core --lib peer_test:: -- --nocapture | 0 | 4相邻通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 77 packages |
| cargo check --workspace --locked | 0 | 通过；step修正后再次通过 |
| cargo tree --locked -p astersql-pd-unsaferecovery -e normal | 0 | 无protobuf、server和具体后端普通依赖 |

去重6聚焦+25相邻通过，0失败、0忽略。动态槽位1，CARGO_TARGET_DIR为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`；自有进程结束后释放锁、保留缓存。
Go/模块1101项存在性、SHA和Git状态一致。仅增加已有codec的workspace依赖，未升级版本。

真实Go普通/commit-merge/joint三种输入各生成3个store计划，Rust由相同真实心跳准备
候选计划，经生产转换后逐字段比较ForceLeader、creates、tombstones、demotes。
受影响Region集合与Go实际计划ID相等。joint场景证明incoming quorum存活时failed列表
为空；其他store的空ForceLeader也精确保留。所选leader的epoch较低但raft term更新，
与Go实际选择一致。

Go当前收集step和下一阶段step原样保存并检查+1。Rust候选计划step明确为0，等待
任务23分配，因此本项只比较计划payload，不声称已实现相同阶段分发。meta/table集合
另有实际codec输入的领域用例；不冒称读取了Go私有受影响表集合。
完整包级/workspace回归依用户授权统一执行，保留待回归任务文件。
