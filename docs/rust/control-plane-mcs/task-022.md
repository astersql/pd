# 任务022：真实ID补洞候选

状态：已完成，待统一包级与workspace回归。任务102尚未完成。

按有效Region索引生成头部、中间、尾部或全空间空Region；优先使用相邻有效store，
回退筛选TiKV，排除TiFlash write。Region/peer ID由宿主canonical allocator实际分配，
epoch均为1、单Voter。没有可用TiKV时返回Go原错误，不分配私有计数器ID。

paranoid检查在真实ID分配后执行，拒绝覆盖任一初始化报告的健康范围；失败不发布
候选，但保留已消耗ID。关闭检查时生成原范围。借用Region引用检查，避免克隆候选。
term在Region ID后失效、peer ID失败时不发布半成品；真实etcd停顿取消也不发布。

实际差分暴露Go启动先Rebase、Rust延迟分配的差距，已独立修复
[leader启动ID窗口及bootstrap调度准入](task-022-id-startup.md)。Go文件保持只读。

## 验证

[所有阶段命令、退出、原始日志及来源SHA](task-022-evidence.json.gz)。首次编译成功后的
red缺少实际creates；实际节点差分又暴露连续范围场景allocator边界不一致，修复上述
启动依赖后六种差分全部通过。曾中断缺少真实bootstrap的暂停时钟fixture，未计通过。

| 最终命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-unsaferecovery --lib mcs_0 -- --nocapture | 0 | 34通过：6本项+28相邻 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_0 -- --nocapture | 0 | 10通过：1本项+9相邻 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 通过 |
| cargo tree --locked -p astersql-pd-unsaferecovery -e normal | 0 | 领域无具体allocator/backend依赖 |

去重7聚焦+37相邻，0失败、0忽略。动态槽位1，绝对CARGO_TARGET_DIR为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`；进程结束后释放自有锁，保留缓存。
1101项Go及模块存在性、SHA和Git状态保持一致。

已有只读Go与真实Rust节点分别验证头中尾空洞、全空间、连续无洞、健康旧范围重叠、
禁用paranoid、无TiKV六种场景。creates全部字段、实际Region/peer ID及后续公开AllocId
逐值比较，无归一化；allocator边界1000，读取header revision与key mod_revision分别记录。

当前Rust输出仍为step=0候选，Go实际已进入下一阶段；分发/确认在任务23实现。
Go生成错误会进入ExitForceLeader，Rust此处仍上抛相同原因，任务24负责错误退出编排。
未把这些不同的完整心跳帧判为等价。全包及workspace测试依用户授权留待统一回归。
