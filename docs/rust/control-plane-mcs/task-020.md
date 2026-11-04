# 任务020：失败副本降级计划

状态：已完成，待统一包级与workspace回归。任务102尚未完成。

只在最新有效Region失去完整quorum且有已报告force leader时，向该force leader
准备DemoteFailedVoters。没有force leader的新增/变化Region跳过；健康quorum不降级。
failed peer选择复用真实失败及orphan判定，保留peer ID、store ID、role和witness。
incoming quorum仍能存活的joint场景保留空failed-voters降级项，用于退出joint state。

对不在最新区间树中的旧Region，只有其自身也失去quorum才生成tombstone；
健康旧重叠Region不删除。受影响Region及meta/table集合来自实际操作元数据。
生成器不修改原报告或健康成员，失败状态不再产生新计划。

Go的FailedVoters载荷也会包含已为Learner的失败peer，仍携带Learner角色；本实现和
真实Go差分均保留此行为，不自行过滤或改为Voter。任务所称“正确处理learner”在
此按实际Go协议验证，不能仅因字段名就假定它只序列化Voter。

真实term在ForceLeader准备无工作后准备该候选，复用同一报告索引和生产protobuf转换。
阶段step分配、分发及推进仍由任务23接续；没有声称完整恢复流程已完成。

## 验证

[命令、退出码、原始日志及来源SHA](task-020-evidence.json.gz)。red编译后在缺少
当前force leader的真实降级载荷处失败；补齐实现后领域及真实协议用例通过。

| 命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-unsaferecovery --lib mcs_0 -- --nocapture | 0 | 23通过：5本项+18前置相邻 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_020_ -- --nocapture | 0 | 1本项，四种真实Go/Rust场景 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_019_ -- --nocapture | 0 | 1相邻，三种真实计划场景 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_018_ -- --nocapture | 0 | 1相邻通过 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_017_ -- --nocapture | 0 | 1相邻通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 77 packages |

去重6聚焦+21相邻通过，0失败、0忽略。动态槽位1，CARGO_TARGET_DIR为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`；自有进程结束并释放锁，保留缓存。
Go/模块1101项存在性、SHA和Git状态一致。

已有只读Go二进制通过真实公开API接收ordinary/joint/failed-learner/healthy-overlap
四种报告输入；Rust节点以同样输入准备计划。每种三个store的demotes、tombstones、
force_leader和creates逐字段相等；无工作store按协议空payload比较，不伪造额外工作项。
受影响Region集合与Go实际载荷ID相等，健康旧Region800不被tombstone，存活健康peer
不出现在failed-voters中；Go已有Learner载荷也原样保留。

Go实际进入DemoteFailedVoter，原始收集/阶段step记录并检查恰好+1。Rust候选step=0
等待任务23分配，只比较生成器payload，不掩盖尚未实现的阶段分发。
完整包级/workspace回归依用户授权统一执行，保留待回归任务文件。
