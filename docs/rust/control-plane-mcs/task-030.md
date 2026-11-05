# 任务030：键空间分组归属迁移

状态：已完成，2026-11-05，基准272b1000d。
[task-030-evidence.json.gz](task-030-evidence.json.gz)保存原始命令与日志。

在同一个canonical KeyspaceGroupManager内实现UpdateKeyspaceGroup。
先解析两个64位十进制ID，再按Go缩窄为32位读取当前持久组；不依赖可能陈旧的本地缓存。
检查源和目标user-kind、split及merge状态后，在同一实际事务中移除源成员并向目标加入，
目标已有成员不重复追加，新追加后排序，保留其他成员和服务节点信息。
只有提交成功才发布两个缓存条目；与CRUD共用成员写锁。相同数字ID按Go的别名路径
先加入再移除，不擅自将其改成无操作。保护条件失败不写持久状态。

本轮验证：

- 可运行red 1项实际断言源成员未移除，退出101；补事务后同例通过。
- 最终cargo test --locked -p astersql-pd-keyspace：28项通过、0失败/忽略，
  包含本项4项真实etcd测试。覆盖与Go相同的陈旧缓存输入、重复迁移不重复成员、
  user-kind/缺失组/非法ID/过渡状态保护、相同组路径，以及4个并发真实任务不丢成员。
- 保存失败在真实etcd事务边界触发：两个单独合法的850000字节节点信息使双组提交
  超过etcd请求限制，实际返回request is too large。两个持久组及两个缓存均保持提交前状态；
  实际停止etcd后迁移失败，同一WAL重启后仍恢复原成员。未用mock保存次数代替状态。
- 复用隔离同源Go测试二进制，精确执行move_between_groups、
  TestUpdateKeyspaceGroupRollbackOnSaveError和TestUpdateKeyspaceGroupSaveErrorPreservesSortedCache，
  三个叶子用例通过（另有一个父测试pass事件，不重复计数）。Go的两个保存失败用例使用
  原有errorStorage；Rust使用真实etcd提交拒绝，分别保存证据，不混称同一种故障注入。
- fmt、workspace check及sa_002架构检查通过。后者动态执行cargo metadata并检查5个负向控制。
  完整workspace test依外层验收任务24统一回归，零doc-tests不计行为证据。
- 每阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；
  所有测试及fixture退出后释放锁，保留缓存。

Go/模块1101项首尾内容、存在性一致。未改写或新增Go输入、未注入failpoint；
所有plan.md和来源史诗只读。后续分裂、合并及服务装配不计入本项完成范围。

审查补齐Go式非法ID控制字符转义；加入NUL和前导+断言后再次包级28项通过。
