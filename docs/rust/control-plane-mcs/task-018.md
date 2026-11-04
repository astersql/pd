# 任务018：恢复Region索引

状态：已完成，待统一包级与workspace回归。任务102尚未完成。

恢复报告构建独立的半开区间索引，不改写canonical在线Region缓存。
每个Region保留全部peer报告，按version、conf_ver选择最新epoch；最新peer未初始化时
整组不进入区间树。候选按epoch降序插入，已存在的较新重叠范围保留，旧范围跳过。
空end表示正无穷。重复ID和错误epoch插入次序拒绝，保留已建索引。

raft新鲜度按term、max(last index, applied index)、last index、commit排序，
最后保留Go的TiKV/voter优先规则及其64位有符号差值回绕。
Orphan从最新成员列表中识别，显式失败store和自动识别下不存在的store不计orphan。
空Region重叠查询使用有序边界二分查找，跳过未初始化peer及相同range/version的重复检查。

真实节点在首次收齐报告时建立该索引，并在新恢复准入成功时清除旧索引。
已收报告不被索引排序或筛选修改，旧重叠报告仍保存在peer分组供后续分析。
领域实现不依赖protobuf或后端；未收齐报告、缺失必需Region返回安全错误，
不复现Go私有方法在非法调用下的panic。epoch完全相同的歧义不宣称复现Go map遍历顺序。

## 验证

[完整命令、退出码、原始日志与来源SHA](task-018-evidence.json.gz)。red编译后在
应保留高epoch重叠范围却索引为空的行为断言失败，之后按原算法补齐并通过。

| 命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-unsaferecovery --lib mcs_01 -- --nocapture | 0 | 13通过：6本项+7准入/收集相邻 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_018_ -- --nocapture | 0 | 1本项，真实Go计划观察与原生Rust报告索引 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_017_ -- --nocapture | 0 | 1相邻通过 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_016_ -- --nocapture | 0 | 1相邻通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 77 packages |

去重7聚焦+9相邻通过，0失败、0忽略。动态槽位1，CARGO_TARGET_DIR为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`，自有进程结束后释放锁、保留缓存。
1101项Go/模块文件的存在性、SHA和Git状态保持一致。

已有只读Go PD通过公开恢复API接收三存活store的真实报告，实际ForceLeader计划
选择700→store2、900→store1，较旧且重叠的800没有进入计划。Rust节点以相同报告
输入收齐后，其实际索引保留700/900，按raft新鲜度选择的leader与Go计划一致。
Go收集step、下一阶段step及Rust收集step分别保留原始值，分别检查相应阶段变化；
不把不同进程的随机初始step要求为相同值，也不归一化Region/store选择。

额外五组raft选择来自Go源码TestSelectLeader的期望断言，作为Rust矩阵用例，
不冒充已执行Go单测。真正运行差分为上述已有二进制的公开协议与实际计划输出。
当前Rust建立索引；完整计划生成、分发、推进和终止继续由19–24实现。
完整包级/workspace测试依用户授权统一执行，保留待回归任务文件。
