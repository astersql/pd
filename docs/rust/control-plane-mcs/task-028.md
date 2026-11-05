# 任务028：元服务分组目录更新

状态：已完成。2026-11-05，基准`2cfdc5eef`。依赖27的直接Go证据已解除，
本项交付canonical manager的更新行为，不代表后续HTTP装配或GC验收完成。
原始red/green、夹具故障、Go逐例JSONL、输入SHA及首尾守卫见
[task-028-evidence.json.gz](task-028-evidence.json.gz)。

## 行为及边界

候选目录先完整规范化，保留Go的空ID、斜杠、空地址及trim后重复错误；
所有变更新端点逐个重新检查，任意端点不健康时不执行配置持久化，不发布部分目录。
健康检查由宿主适配器提供；领域层不缓存连接或TLS输入。测试适配器每次通过现有
EtcdMetaStore/官方SDK访问真实etcd，未用固定健康值或调用次数替代状态。
TLS/HTTP宿主适配属于131的装配范围，本项没有复制传输实现。

更新锁覆盖健康检查、持久化与发布顺序；组目录写锁阻止并发分配。
分配返回的GroupReservation持有读guard直到调用方提交或回滚，避免删除尚在分配的组。
删除时优先在锁内调用实际键空间目录计数；未装配目录时保留Go的持久计数fallback。
测试目录计数直接读取实际protobuf键空间元数据；陈旧正计数不会阻止删除空组，
真实关联键空间仍会拒绝删除。配置先持久化再发布；失败时旧目录保持可见。
已提交配置后的状态清理按Go为best effort，失败随成功结果携带cleanup_error交宿主记录，
不会把已提交的配置伪装成回滚。AttachEndpoints装饰独立快照，GetGroups返回拥有的副本。

## 本轮验证

- 首次可运行red：3项执行，2通过/1断言失败（持久化后目录未发布），退出101。
  补发布后3通过，27的5项聚焦回归通过。扩展并发测试的两处fixture字段移动编译错误
  修复为clone，不计行为red。
- 两个测试模块重复引入夹具导致进程内目录计数器冲突；改为lib.rs唯一注册、各模块复用。
  后续一次native启动退出原日志已清理，未猜测根因；加入退出日志诊断后，失败用例单跑通过，
  完整包22通过，再针对未解释的启动失败追加三轮完整包稳定性复验，三轮均22通过。
  重复运行不累加为新增测试，日志中历史失败全部保留。
- 最终包级`cargo test --locked -p astersql-pd-keyspace --lib -- --nocapture`：
  22通过/0失败/0忽略，含7本项、5任务27、10基础相邻。
  7项覆盖实际健康/不健康端点、配置提交失败及同WAL恢复、所有规范化Go固定输入、
  protobuf键空间扫描删除保护、reservation排除删除、并发提交发布顺序、提交后清理失败。
- `sa_002_`架构守卫1通过，5负向控制，78包、682生产源码；
  `cargo check --workspace --locked`和格式检查通过。完整workspace test仍由任务24执行。
- 所有Rust修改先cargo fmt --all。每阶段动态原子领取槽位1，
  `CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`；
  验证进程退出后释放自有锁，缓存保留。

复用用户授权的只读源码沙箱和现有Go 1.26.8构建产物：
`TestMetaServiceGroupTestSuite`16个子例真实通过，其中本项接收AttachEndpoints、
GetGroups副本、ownership、实际etcd健康、TLS加载边界、串行更新及authoritative count等
原用例回执。Go suite中的TLS loader测试是原有oracle，不冒称新增Rust TLS传输验证。
另隔离构建`server/config`现有测试，`TestAdjustMetaServiceGroups`七个逐例通过；
Rust逐一回放相同map、结果和错误字符串。固定协议及源码指纹沿用27本轮同源构建，
没有新增Go oracle源码、failpoint注入或make/生成/整理命令。

Go/模块1101项首尾SHA和存在性一致、受保护Git状态为空；所有plan.md及来源史诗只读。
