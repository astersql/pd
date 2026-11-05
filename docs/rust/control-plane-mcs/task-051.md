# 任务051：按期限重试GC预热失败

2026-11-06，基准7352b7cde。[完整回执](task-051-evidence.json.gz)保存red/green、全部命令/退出码、Go原例、来源摘要及只读守卫。

GCRetryQueue为每scope保留唯一到期项，按deadline有序；一个调度轮次最多移出60项。
GCWarmup将实际失败变为1/2/4/8/16/30秒封顶退避，批次共用记录时间；只在到期后重新排队。
worker等待最早重试/全局joined检查期限或输入通知，不按scope创建任务。joined ID单独索引，每100ms最多一次统一扫描，避免遍历全部完成scope。
首snapshot重整删除失败尝试提示产生的候选/重试；已完成终态不因缓存失效重新入队。只有原始失败目标到期重试时查49的live index，已禁用/移除/统一管理目标可退休；新出现成员不会扩充初始目标。
取消后join固定worker并释放所有期限/候选/输入，后续修复存储不会唤醒旧campaign。

验证：

- red：真实响应闸门持有已读取的坏txn bytes，随后修复存储；无重试时campaign不能完成，1 failed、exit101。
- 最终`cargo test --locked -p astersql-pd-gc --lib mcs_051_ -- --nocapture`：4 passed、0 ignored、exit0。
- 同一实网闸门场景接入期限队列后，重复page提示不能提前越过重试期限，后续真实重读取得55并完成；未把坏响应改成成功或伪造存储结果。
- 时间边界测试覆盖deadline前1ns不出队、相同deadline可出队，scope100/101/102顺序30s/2s/1s，退避1/2/4/8/16/30/30。
  20000项到期数据每轮只提升60项，重复同scope不产生重复队列项，过滤过时范围后剩余结果准确。
- 真实49索引收到metadata禁用事件后，旧初始目标的坏GC值仍在etcd，但retry退休该scope并完成；不写零值或伪造修复。
- 取消campaign后实际修复GC值并越过首个retry期限，旧worker仍不写缓存；flight表为空，新前台读取才获得55。
- 同源Go Classic/NextGen各9项顶层原例，13个pass节点；叶例数量见回执。
  原例的纯期限调度与运行时恢复分开记录；Rust纯队列用相同IDs/到期值/20000项/60项上限，另用真实etcd和网络边界证明恢复。
- GC全包61 passed、0 failed、0 ignored；metadata、fmt check、workspace check和git diff --check通过；全workspace test由外层24集中执行。
- 所有Cargo阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1，所有自有子进程结束后释放锁，保留缓存。
- Go/模块1101项首尾SHA及存在性一致；无Go或模块写入、make/failpoint注入/生成整理，所有plan.md和来源史诗保持只读。
