# 任务031：拆分时间戳分组

状态：已完成，2026-11-05，基准375189593。
[task-031-evidence.json.gz](task-031-evidence.json.gz)保留命令、源码摘要和原始结果。

canonical KeyspaceGroupManager新增split_group、finish_split；先读取当前持久组、检查
角色及副本数，使用现有MetaStore事务，先保存目标再保存源，提交后再发布双方索引。
显式列表优先于范围，去重列表，范围含首尾；按已有Kernel规则保留受保护键空间。
显式重复列表的Go map迭代顺序不确定，Rust选择排序后的去重结果；非重复列表保留输入顺序，
没有归一化其他顺序、revision或故障结果。副本数复用canonical mcs-constant，
仅新增一个生产依赖边，不复制常量或引入transport。

Finish以目标ID读取，验证目标/源角色，原子清空双方split状态；失败保留持久状态和本地索引。
沿用Go的源优先错误检查顺序、默认/预留键空间保护、已有目标与不足副本错误。
HTTP巡检、后台自动拆分及服务装配留给各自拥有任务。

本轮验证：

- 最初可运行red 1项，断言拆分目标尚未持久化失败，退出101；补实现后同例通过。
- 最终聚焦5通过，完整键空间包33通过，0失败/忽略；其中本项4个真实etcd测试和
  1个纯分区边界测试。复用Go TestBuildSplitKeyspaces的13个固定输入，另外检查
  Classic/NextGeneration保护及显式列表优先/输入顺序。
- 真实etcd覆盖列表及范围拆分、Finish、缺失/非法角色/重复拆分/已有目标/副本保护，
  两个tokio任务竞争同源仅一个成功，源目标mod_revision相同，所有键空间仍归属唯一组。
- 拆分和Finish分别在真实提交边界超过etcd请求大小限制，确认没有部分持久化或缓存发布；
  实际停止etcd后Finish失败，重启后重新建立manager完成；大对象失败状态同WAL重启仍保持。
- 扩展测试首轮4通过/1失败，发生在重启后复用旧SDK连接的Bootstrap返回Unavailable；
  改为重启新宿主建立新SDK连接，聚焦5及包级33通过。没有改领域重试或掩盖先前失败日志。
- 同源隔离Go二进制真实执行TestKeyspaceGroupSplit、TestKeyspaceGroupSplitRange、
  TestBuildSplitKeyspaces，3个实际用例通过；后者内部13组断言只计1个Go测试。
  原Go suite使用MemoryKV，Rust耐久性取自本轮真实etcd，不将两者混称。
  未运行集成suite中需要failpoint.Enable的路径，Finish的可选暂停未被启用。
- fmt、workspace check通过；sa_002动态metadata架构守卫1通过、5负向控制、78包688生产源码。
  完整workspace test仍按外层计划交任务24统一回归。
- 每阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；
  所有验证和fixture进程退出后释放自有锁，缓存保留。

1101项Go/模块输入首尾SHA及存在性一致，没有新增Go源码、模块差异或failpoint注入。
所有plan.md及来源史诗保持只读。
