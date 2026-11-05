# 任务049：恢复启用键空间监听

2026-11-05，基准fb7edafbf。[完整回执](task-049-evidence.json.gz)保留全部命令、退出码、失败/通过、源码摘要、双内核Go原例及Go守卫。

EnabledKeyspaceIndex使用48同一loader，从完整snapshot revision+1接续watch。事件只进入私有pending集合；仅有序progress可发布整个集合及对应revision，创建watch的header不推进进度。
删除/禁用删除索引项，管理类型值原样保留；损坏metadata不发布部分批次。失败/compaction/无进展超时后完整重载，退避1s至30s，持续健康watch重置退避。
只在第一次完整snapshot之前接收page hooks，initial hook在发布后仅调用一次；所有回调拿独立值且不持索引锁。snapshot_at_least先建立通知订阅再检查状态，取消/退役唤醒等待者，旧集合可继续满足已证明的旧revision。
MetaWatch新增可选next_with_progress；etcd适配保留真实progress响应并抑制created header。按绝对1秒期限请求progress，持续事件不能饿死进度；确认progress还推进重连游标，避免重放已compact的旧event cursor。默认next原有事件语义保持。
index由宿主run/cancel/join，未安装后台runtime或全局状态；50–52继续负责预热和实际任期装配。

验证：

- red：真实新增空间2后返回旧快照，行为断言失败，1 failed、exit101；随后接线通过。
- 最终`cargo test --locked -p astersql-pd-gc --lib mcs_049_ -- --nocapture`：6 passed、0 ignored、exit0。
  实际etcd测试覆盖新空间、删除/禁用、无prefix事件的progress屏障、持续事件下progress、拥有快照不受调用方修改影响，损坏metadata阻止新revision且修复重载，真实WAL重启后的索引恢复，取消前ready/后ready等待者。
- 首snapshot发布后暂停watch创建，真实写入并compact缺口；旧revision的watch失败后重载取得新集合，initial hook只触发一次。
  首次测试用同步回调阻塞Tokio驱动导致writer超时，改为block_in_place后通过；失败原样保留，不算生产故障证据。
- 独立真实watch先收到无事件progress，compact该revision并停启etcd，再恢复读取；没有从已compact的旧event cursor重放，实际新事件到达。
- Go Classic与NextGen各9项原例通过，复用已核验同源gc二进制和只读sandbox，无Go源码/模块写入或failpoint注入。
  原例有自己的client拦截器；不把它们说成Rust实网故障。Rust原生测试用真实etcd状态/网络边界独立证明恢复。
- GC全包在最终增加游标专项前50项通过；最终6项聚焦通过。存储包34单元+12真实etcd契约通过，最后受影响watch 4项通过。
- metadata、最终fmt check、workspace check、git diff --check通过；完整workspace test依外层任务24集中执行。
- 所有Cargo阶段原子领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1，所有自有子进程结束后释放锁，缓存保留。
- Go/模块1101项首尾SHA和存在性完全相同，所有plan.md及来源史诗保持只读。
