# 任务12：元服务分组HTTP验收

2026-11-06，基准 `52f2501ec`。状态：已完成。
[完整回执](task-12.json.gz)保留确切命令、退出码、初始失败及最终日志、来源和输入摘要、
原始HTTP、持久字节及revision、Go只读守卫和资源清理结果。

来源为 `wire-inventory.md` 的 HTTP-190–192；三条方法从只读Go
`RegisterMetaServiceGroup` 提取，未沿用历史路由计数。仅接通本项GET/PATCH及状态PATCH，
MCS三个未准入方法继续保留显式503拒绝。批次11以及原27、28、131的公开接口和证据已核对。

原27/28拥有的canonical `MetaServiceGroupManager` 补齐 `get_status`、`patch_status`，
状态补丁只比较目标组，并复用现有状态编解码及CallbackTxn；
canonical `MetaGroupKeyspaceCounter` 分页读取真实键空间目录，禁止在持有分组写锁时装饰结果。
原131的宿主继续注入同一manager、MetaStore、任期fencing及取消资源；HTTP只做参数和错误转换。
配置持久化成功后才发布目录；保留已配置的其他keyspace字段。
端点检查使用新建的真实etcd连接，要求leader并执行线性化health读取；每次重读TLS输入，
复用宿主配置，无私有runtime、全局信号、额外领域算法或独立后端。
宿主跟踪已接收的更新：提交前尊重请求/任期取消，已准入的配置写入完成后发布，关闭时等待退出。

## 当前行为证据

- classic与API模式各启动隔离的Go/Rust进程和数据目录，经真实TCP比较61项HTTP观察，
  共122项同输入配对；逐项比较status、Content-Type、JSON、Location和重定向正文。
  每模式45组前后持久快照，共90组配对，比较实际状态字节并保留绝对create/mod revision。
- 覆盖三条注册的未bootstrap门禁、尾斜杠重定向、空目录、添加/修改/删除、
  JSON语法与类型、null、trim、URL不安全ID、trim后重复ID、重复字段及字段顺序、
  负数/溢出/非整数计数、enabled切换、缺失组和不健康端点。
  所有错误后的状态及配置字节/revision保持不变。
- 通过HTTP创建真实键空间，观察共享manager的分组归属和计数；将持久计数人为置零后，
  删除仍被真实引用保护。键空间进入TOMBSTONE后可以删除分组；同ID重加的状态重新为禁用/零计数。
- 用健康端点和超过真实etcd请求限额的配置触发持久化失败，Go/Rust均返回相同500错误合同，
  持久配置和可见目录均保持原状；没有使用mock响应或固定健康值。
- 重启PD及其etcd WAL后，目录恢复，状态字节和create/mod revision不变；
  三条方法分别经真实第二节点转发比较。两个写入口的未完成请求体断连不写入任何状态或配置。
- canonical状态测试在真实etcd两操作预算下验证：全组读取超限，但目标组补丁成功；
  缺失组和负计数不写入，停服失败再同WAL重启仍保留原值。
  权威计数器在真实etcd读取131条元数据，跨页得到130条目标组引用。
- TLS测试将真实etcd通过宿主现有mTLS listener转发：合法凭证成功，缺失CA和非受信凭证失败，
  恢复输入后重新连接成功；连接、代理任务和fixture均退出。没有将此测试扩张为全部TLS拓扑认证。

## 失败及修复

初始可运行red为0通过/1失败（503未到bootstrap门禁），退出101。
后续矩阵发现活动任期代理未转发新增接口、数值类型错误文本和超大配置持久化错误文本差异，
均由真实响应定位并修复。最终所有对比通过。

夹具问题独立保留，不计实现red：未关闭Region分裂等待、误用PATCH而非已注册PUT、
最初超大配置仍小于Go默认150MiB限额，以及curl的100 Continue中间响应解析。
另有类型/导入/fixture注册编译错误，未计为行为失败或通过。
最终fixture显式设置请求限额并关闭Expect，原始失败日志没有覆盖或删除。

## 最终验证

默认features，以下行为检查均退出0；重复执行不累计为新测试。

| 命令 | 有效结果 |
| --- | --- |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_012_ -- --nocapture` | 2通过；含最终补充的成功端点更新及已有键空间地址装饰 |
| `cargo test --locked -p astersql-pd-keyspace --lib -- --nocapture` | 112通过，包含本项2个领域测试及原27/28等相邻行为 |
| `cargo test --locked -p astersql-pd-runtime --lib sa_012_ -- --nocapture` | 1通过，真实mTLS健康连接 |
| `cargo test --locked -p astersql-pd-runtime --lib control_keyspace_test -- --nocapture` | 1通过，原131共享资源/任期切换 |
| `cargo test --locked -p astersql-pd-server --lib task_078_ -- --nocapture` | 2通过、1既有ignored；剩余MCS拒绝边界保留 |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_009_ -- --nocapture` | 4通过，键空间HTTP相邻回归 |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_002_ -- --nocapture` | 1通过：80包、737生产源码、6负向控制 |

有效去重123通过、0失败、1既有忽略。本项真实双实现矩阵独立执行，未以ignored代替必要oracle。
`cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server`、
`cargo metadata --format-version 1 --no-deps`、`cargo check --workspace --locked`、
`cargo fmt --all -- --check`及`git diff --check`均退出0。
主驱动13条命令均通过；补齐成功端点更新场景后，只重新格式化、执行本项2个测试及格式/diff检查，
追加4条命令均通过，没有扩大或重复完整workspace test。

构建和行为验证动态领取槽位1：
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
补充纯格式化领取槽位2：
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-2`。
全部本项子进程退出，自有锁释放，缓存保留。总plan、领域plan及来源史诗保持只读；
既有计划文件删除和其他未跟踪任务文件保持原样。

Go源码及模块1101项首尾内容、存在性及受保护Git状态一致；
未写临时Go，未运行make、failpoint注入、Go生成或模块整理。
固定kvproto ZIP、Go oracle、SDK归档、PROTOC及官方etcd3.5.15/etcdctl重验摘要。
完整workspace test按原计划集中任务24；本项不宣称原生生产后端或Go退役认证。
