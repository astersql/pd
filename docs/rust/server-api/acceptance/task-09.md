# 任务09：键空间管理HTTP验收

2026-11-06，基准 `59e50389c`。状态：已完成。
[本轮原始回执](task-09.json.gz)保存源码/输入摘要、确切命令、退出码、所有失败/通过日志、HTTP响应和真实持久状态。
来源为wire-inventory.md:311–317的HTTP-179–185；从只读Go RegisterKeyspace逐条提取方法/路径，未沿用历史路由总数。
批次8已完成；36–41的领域API与证据已核验，原拥有任务[131](../../control-plane-mcs/task-131.md)在本轮补齐必要宿主接线。

七条路由委派同一宿主KeyspaceManager：目录、按名称/ID读取、两种创建、配置PATCH和状态PUT。
只移除本领域拒绝，剩余18个未准入注册仍返回依赖拒绝。
classic和API模式分别使用独立Go/Rust真实TCP进程和隔离数据目录/etcd。
矩阵比较status、Content-Type、JSON、Location和重定向正文；仅归一独立进程产生的墙钟时间字段。
原始时间、完整持久bytes和create/mod revision保留，不声称独立集群物理revision相同。

本轮行为证据：

- 未bootstrap优先级；名称/显式ID/专用分配器创建、重复名称/ID、缺少ID、非法名称、配置保留项。
- 目录分页、空页null、page token/limit错误、GET /keyspaces/id的名称语义、uint64→uint32按ID转换。
- 配置增删、空字符串、前置条件成功/409失败，失败后元数据原bytes/revision不变；不支持配置项保持Go错误。
- 生命周期迁移、非法回退、保留空间保护、未知状态的Gin `{}`响应，以及绑定错误的text/plain与完整typed消息。
- force_refresh_group_id分别验证classic的nil manager错误与API的真实归属；尾斜线301/307的Location、正文和Content-Type。
- 从Go源码提取的所有注册在leader执行，并逐条比较真实第二节点HTTP转发；同节点Keyspace gRPC返回not leader。
  幂等状态PUT按Go合同仍提交事务：protobuf语义/时间/create revision不变，目标mod revision增加；其他元数据不变。
- HTTP创建→gRPC读取、gRPC状态迁移→HTTP读取，watch的初始快照及增量，独立存储adapter读取并比较最终领域元数据。
- 分割超时在真实labeler/etcd边界触发，标签与名称索引/元数据回滚；未完成及完整请求体断连均不遗留新空间。
- 双方重启PD及真实etcd WAL，原元数据和标签bytes/create/mod revision完全保持。
- 真实配置pre-alloc由宿主驱动，API模式预分配ID、状态与配置和Go相同，重启不重复分配。

首次行为red为0通过/1失败，exit101，实际503而预期进入bootstrap门。
完整请求体断连另取得0通过/1失败的真实red：Rust留下DISABLED元数据；
修复为宿主任期TaskTracker持有创建直到领域清理结束，取消场景随后通过。
中间夹具/接线失败同样保留：错误复用另一槽位旧二进制、使用尚未准入的v1配置写、CLI配置未传入manager、
生成tonic trait签名、测试中未声明regex依赖（改为无依赖源码提取），及错误假设Go幂等PUT不增加mod revision、成功重写的protobuf map字节顺序必定不变。
预分配另取得0通过/1失败的真实red，随后补齐原131的宿主工作驱动；普通重启不再调用领域bootstrap。
一轮fmt check恰逢补充新测试而失败，随后先fmt再执行最终验证。没有隐藏失败或将编译失败计作行为red。

最终验证（默认features；以下均exit0）：

- `cargo test --locked -p astersql-pd-compat-tests --lib sa_009_ -- --nocapture`：4通过，0失败/忽略。
  矩阵4个独立实现/模式运行，各52项观察，包含从Go源码提取的7条leader及7条follower路由。
- `cargo test --locked -p astersql-pd-runtime --lib mcs_131_ -- --nocapture`：1通过，0失败/忽略。
- `cargo test --locked -p astersql-pd-server --lib task_078_ -- --nocapture`：2通过，1既有ignored；本项双进程oracle没有忽略。
- `cargo test --locked -p astersql-pd-runtime --lib mcs_132_ -- --nocapture`：2通过，0失败/忽略。
- `cargo test --locked -p astersql-pd-keyspace --lib mcs_03 -- --nocapture`：61通过，0失败/忽略。
- `cargo test --locked -p astersql-pd-runtime -- --test-threads=4`：104通过，0失败，38既有ignored；0 doc-tests不计行为证据。
- `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server`、`cargo metadata --format-version 1 --no-deps`、
  `cargo check --workspace --locked`、`cargo fmt --all -- --check`、Python语法及`git diff --check`通过。

完整workspace test按调度集中验收24，未重复执行。
所有Rust修改均先cargo fmt --all。最终git diff --check及Go只读守卫必须通过后才完成。
最终有效Cargo验证使用槽位1，CARGO_TARGET_DIR为`/Users/xiangmin/worker/code/pd/target/rust-slot-1`；
早期一次错误验证动态领取槽位2并使用其旧二进制，失败日志明确保留，不计最终证据。
各阶段结束后释放自有锁，保留缓存。Go oracle、kvproto f8ad079、官方etcd3.5.15/etcdctl、PROTOC及SDK归档本轮核验摘要。
没有Go构建/测试、make、failpoint或Go生成/整理。外部etcd/WAL不替代验收17–18的原生生产后端责任。
