# 任务029：时间戳分组目录

状态：已完成。基准`0876c6c9c`，2026-11-05。
本项提供 canonical KeyspaceGroupManager 的 Bootstrap、批量创建、查询和删除；
后台分配与角色生命周期由后续拥有任务实现，不计入本项完成范围。
原始命令、red/green、Go逐例回执及输入指纹见
[task-029-evidence.json.gz](task-029-evidence.json.gz)。

默认0组使用basic和键空间[0]；重复Bootstrap不覆盖已有组，重新读取持久目录建立三种
user-kind索引。创建事务先检查每个ID，任意重复会放弃整批写入；按Go只保存创建DTO字段，
不带入split/merge过渡状态。分页起点包含，非正limit不限量，物理键顺序保持不变。
删除0组拒绝，split状态拒绝，缺失组返回None；事务成功后才发布本地索引。
所有修改共享成员变更锁；查询和修改读取持久状态，未以本地缓存代替事务读取。

沿用实际已有JSON值类型和存储编解码，将其实现移到storage-domain，endpoint只重导出。
领域层不引用proto或transport，没有另建分组目录实现。

验证结果：

- 初始可运行red：2项均在默认组未持久化及重启索引未恢复断言失败，退出101。
- 补全Bootstrap后，键空间包24通过/0失败/0忽略，其中本项2项真实etcd测试。
  覆盖默认组、CRUD、分页、三类索引、批量失败revision不变、split与默认删除保护，
  真实停止etcd时创建/读取/删除失败且缓存保持原状，以及同WAL重启后的目录重建。
- 受影响endpoint的keyspace_group过滤12通过，覆盖原JSON和存储协议，含真实etcd。
- 只读隔离Go测试二进制来自任务27同源构建；TestDeleteDefaultKeyspaceGroupIsRejected、
  TestKeyspaceGroupOperations、TestKeyspaceGroupSplit三个逐例运行通过。
  Rust回放其默认组、ID1/2/3及键空间[111,222,333]输入与对应目录断言；
  这里只接收Split用例中的目录/非法删除证据，不宣称本项实现分裂算法。
  原Go目录suite使用MemoryKV；耐久性与网络故障证据来自本轮Rust真实etcd，
  原Go真实etcd事务冲突/取消证据另由任务27的TestEtcd提供。
- cargo metadata、fmt检查、workspace check通过；sa_002架构守卫及5个负向控制通过。
  首次架构命令误用不存在的package名称，未执行测试；修正为compat-tests --lib后通过。
  完整workspace test按外层验收计划由任务24统一执行。
- 每阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1，
  所有进程退出后释放自有锁，缓存保留。命令及退出码完整存入压缩证据。

Go/模块1101项首尾SHA与存在性一致，未新增Go源码、模块或failpoint注入。
所有plan.md和来源史诗保持只读。
