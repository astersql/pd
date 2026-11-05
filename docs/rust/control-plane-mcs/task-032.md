# 任务032：合并时间戳分组

状态：已完成，2026-11-05，基准42df1c10c。
[task-032-evidence.json.gz](task-032-evidence.json.gz)包含完整历史日志、输入SHA和物理状态回执。

canonical manager新增merge_groups和finish_merge。源组和目标组均读取当前持久状态，
按Go顺序检查缺失、split及merge角色，保留目标节点配置，将成员键空间去重排序，
同一事务保存目标merge-list并删除所有源组，成功后才更新索引。空源列表不读目标即成功；
最多59源组，容量检查先于默认源组保护，复用中性storage-domain MAX_BATCH_OPS=120。
默认组可作目标，不能作源。Finish使用revision保护的读取，确认所有源组已消失才清除状态；
源组重新出现或重复Finish保持Go错误语义。本项完成范围为这两个命名领域行为，
后台MergeAllIntoDefault的分批等待编排及HTTP装配由后续宿主负责。

本轮验证：

- 最初可运行red 1项，断言源组未被删除失败，退出101；补事务后通过。
- 最终键空间包38通过/0失败/0忽略，其中本项5个真实etcd用例。
  覆盖原Go split→finish→merge→finish输入，59源组成功及60源组拒绝、默认源保护、
  空列表、源角色、源重新出现、重复Finish、并发同目标仅一项成功。
- 实际etcd请求大小上限拒绝合并提交，源、目标及索引全部保持原状；改用小默认目标
  成功后，真实停服使Finish失败，同WAL重启的新manager保留merge状态及已删除源组，
  最后完成合并。没有以MemoryStore或mock调用次数替代事务/网络故障。
- 只读同源Go测试二进制的TestKeyspaceGroupMerge真实通过；Rust回放相同1/2/3组和
  [111,222,333,444,555]成员断言。原Go该例使用MemoryKV，耐久性单独取自真实etcd。
- 新增compat-tests明确环境用例mcs_032_default_group_go_differential，实际--ignored运行
  1通过/0失败/0忽略：启动准入Go二进制的api服务模式，经真实HTTP把组1/2合入默认0组，
  Rust领域执行同样输入；合并和Finish的完整DTO逐字段相等，[0,111,222,333]去重相同。
  捕获Go初始0组完整值作为显式共享fixture输入，未删掉节点地址等字段来掩盖差异。
  官方SDK读取Go内嵌etcd及Rust真实etcd，合并后仅剩00000物理键，JSON字节完全相同，
  create_revision保持，mod_revision真实推进。不同集群的绝对key/revision原样保存，
  没有把独立集群的全局revision强行等同。
- 初次环境入口因PATH没有go元数据工具失败；补准入SDK PATH后普通Go模式返回500，
  源码确认其未初始化分组manager，改PD_SERVICE_MODE=api后在线差分通过；历史均保留。
- fmt、workspace check通过；sa_002动态metadata守卫通过，78包689生产源码、5负向控制。
  完整workspace test依外层任务24统一回归；本项明确环境测试默认ignore不计默认成功数，
  只接收实际--ignored通过回执。
- 全阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；
  所有测试、Go和etcd进程退出后释放自有锁，缓存保留。

1101项Go/模块输入首尾SHA及存在性一致。没有新Go源码、模块改写或failpoint注入；
所有plan.md和来源史诗保持只读。
