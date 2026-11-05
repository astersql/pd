# 任务08：GC状态与HTTP查询验收

2026-11-06，基准 `b72888d5e`。状态：已完成。
[本轮原始回执](task-08.json.gz)保存输入与源码摘要、全部失败/通过日志、确切命令、退出码、RPC/HTTP响应及真实持久记录。

来源交接表本项5行：wire-inventory.md:56–57 的 GetGCState、GetAllKeyspacesGCStates，
293–294 的 v1 GET/DELETE /gc/safepoint，以及329的 v2 GET /gc/safepoint/{keyspaceID}。
回执逐行保存coverage ID和原始行指纹。没有将历史输出冒充本轮执行。

复用[43](../../control-plane-mcs/task-043.md)、[47](../../control-plane-mcs/task-047.md)、
[48](../../control-plane-mcs/task-048.md)、[49](../../control-plane-mcs/task-049.md)、
[50](../../control-plane-mcs/task-050.md)、[51](../../control-plane-mcs/task-051.md)、
[52](../../control-plane-mcs/task-052.md)、[53](../../control-plane-mcs/task-053.md)、
[132](../../control-plane-mcs/task-132.md)的当前公共接口、领域与任期证据，以及[批次7](task-07.md)。
两个RPC和HTTP使用同一任期所有的canonical GC manager；SyncGrpc显式委派两个方法。
全局列表公开已有领域加载操作；v1展示的兼容worker/minimum转换放在既有legacy领域模块，
强制删除委派已有方法。没有在transport重造GC事务、缓存、发现或预热算法。
仅移除已准入的GC v2依赖拒绝，其余25个待准入路由仍保持拒绝；两个废弃watch保持task037合同。

本轮真实Go/Rust进程分别连接自身真实etcd/WAL，真实tonic客户端、curl和独立MetaStore观察：

- 两RPC未bootstrap、错误cluster、真实第二节点follower及deadline取消；精确码/消息与完整header保留。
  取消通过真实响应gate及暂停服务分别覆盖丢失响应和调用前取消，读取不改变GC持久entries。
- 单空间Null、DEFAULT、独立、disabled、unified、缺失及超范围ID；include-global与exclude-barrier四种组合。
  expired屏障仍返回TTL0，gc_worker过滤；无限TTL、有限1/60/3600秒剩余TTL边界均覆盖。
- 全量105个状态跨100项metadata分页；disabled过滤、unified保持自身零状态，Null保留；两种exclude组合交叉比较。
  单空间与全量完整protobuf经排序且仅归一独立cluster ID后精确比较。
- v2 HTTP成功、unified重定向、disabled读取、缺失ID、负数、非数字、u32溢出、+2拒绝及前导零；
  原请求ID与safe_point保持Go合同，未bootstrap优先于参数检查。
- v1未bootstrap列表、worker伪记录、expired屏障/minimum、reserved与缺失名称强制删除；
  每次DELETE持久bytes与对方精确比较，原始create/mod revision保留。
- present-empty revision键造成真实etcd NotExists CAS冲突：两RPC及v1 GET/DELETE均错误，entries不变。
  损坏txn point的完整header/消息跨实现精确比较，修复后重新读取。
- 两实现重启PD及真实etcd WAL；单空间GC状态、局部/全局屏障和v1删除结果的完整bytes/create/mod revision首尾一致。
  初始同输入GC持久suffix/bytes跨实现比较；物理revision原值保留，不声称独立集群物理revision相同。

验证（默认features）：

- 最初RPC行为red：0通过/1失败/0忽略，exit101，Unimplemented / Not yet implemented。
  补接Grpc后真实SyncGrpc仍未委派的失败被保留，随后修复实际对外服务。
- v1新增行为red：0通过/1失败/0忽略，exit101，实际404而预期200。
- 最终 `cargo test --locked -p astersql-pd-compat-tests --lib sa_008_ -- --nocapture`：3通过/0失败/0忽略，exit0。
- `cargo test --locked -p astersql-pd-server --lib task_078_ -- --nocapture`：2通过/0失败/1既有ignored，exit0。
  该旧HTTP oracle ignored不计本项兼容证据，本项上述真实双进程HTTP测试未忽略。
- `cargo test --locked -p astersql-pd-server --lib task_037_ -- --nocapture`：2通过/0忽略，exit0。
- `cargo test --locked -p astersql-pd-gc --lib mcs_043_ -- --nocapture`：8通过/0忽略，exit0。
- `cargo test --locked -p astersql-pd-gc --lib mcs_053_ -- --nocapture`：8通过/0忽略，exit0。
- `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server`、
  `cargo metadata --format-version 1 --no-deps`、`cargo check --workspace --locked`、
  最终 `cargo fmt --all -- --check`、Python语法及 `git diff --check`通过。
  完整workspace test按计划集中任务24。

中间失败全部保留：测试误认为+2合法、误用合法零填充revision作为CAS故障、
在两节点Go etcd关闭follower后读取失去quorum、Rust重启误走Go embedded路径；
均修正夹具/断言并取得后续实际通过证据。一轮fmt check在补充v1测试期间捕获未格式化文件，
随后先cargo fmt --all再完整执行本项聚焦/相邻验证；没有隐藏该轮失败。

全部Cargo阶段动态领取槽位1，绝对CARGO_TARGET_DIR为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`。全部Cargo及产物子进程结束后释放自有锁，保留编译缓存。
固定kvproto f8ad079、Go oracle、官方etcd3.5.15/etcdctl、PROTOC、SDK归档及成员本轮重验摘要。
1101项Go/模块文件首尾存在性及SHA一致；没有执行Go构建/测试、make、failpoint或Go生成/整理。
此处真实外部etcd/WAL证明过渡适配行为，原生生产后端证据仍归任务17–18。
只提交本项实现、测试、supervisor来源登记及证据；用户已有删除、其它任务与所有plan.md均保持原状。
