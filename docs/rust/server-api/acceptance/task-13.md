# 任务13：MCS发现与primary驱逐HTTP验收

2026-11-06，基准 `1a0de9efeee44c9d8ff9d567017615a946638759`。状态：已完成。
[完整回执](task-13.json.gz)保留全部命令及退出码、有效red/green日志、原始HTTP头/正文、
租约/选主/时间戳窗口字节与revision、来源/输入摘要、Go只读守卫和清理记录。

来源为 HTTP-193–195，方法与路径从只读Go `RegisterMicroservice` 提取，要求非空，
未沿用历史路由总数。接通 GET members、GET primary及POST TSO primary evict。
原三条503分支由实际领域适配替换；未知v2路径仍404，不落入tonic的HTTP 200 fallback。
classic模式按Go返回JSON 404；API模式使用宿主同一MetaStore中的真实注册和primary租约。

## 领域实现与接线

- 在原58–61拥有的 `pkg/mcs/discovery` 建立唯一canonical crate：六字段注册记录、
  Go式空字段/null/重复字段/大小写处理、损坏记录跳过、未知服务错误、按元数据键排序读取。
  发布使用CreateRevision/Lease比较；续租失败重建前核实旧lease归属，撤销前先join续租worker。
  相同地址/相同启动秒的另一实例不能抢写，旧实例关闭不删除替代实例。
- 在原54/55拥有的 `pkg/mcs/utils/expected_primary.rs` 实现最新启动记录去重、组成员筛选、
  名称/无scheme地址识别、预期primary租约、marker revision CAS及旧任期fencing。
  marker提交成功后才resign旧term；胜选者只清理自己观察到的marker，不删除新转移目标。
  在canonical `pkg/tso/tso_group_watch.rs` 读取/比较marker后campaign，读失败不退化成自由选主。
- 扩展原81/82/136的现有TsoServiceHost。HTTP与tonic共用一个listener和同一个
  Arc<Mutex<GroupAllocatorWatch>>；驱逐预检目标及split状态，然后调用上述领域转移。
  shutdown取消请求/worker，等待退出，再释放本实例注册租约；没有第二套选主或allocator。
  原140的 `pd-server services tso` 入口消费同一宿主，命令层拥有runtime和信号；
  从共享后端读取真实cluster ID及PD leader，注入进程身份，SIGTERM后正常退出。
- PD HTTP只处理路由/参数/JSON/错误转换，以及向已注册canonical地址转发一次请求。
  复用既有request-owned HTTP/TLS驱动，丢弃客户端node查询，改写为节点本地evict路径；
  连接及响应体随请求取消/丢弃而回收。仅注册成员可作为目标；scheme别名不改变实际转发地址。

这是本项实际消费接口的实现及证据交接，不将原54–64、81/82、136/140的所有服务、
配置、指标和TLS拓扑一并标为已完成；它们的剩余行为仍在原拥有计划。
没有在transport复制注册、expected-primary或TSO领域算法，没有引入另一套生产后端。

## 当前运行证据

- classic双实现的三条源码注册逐条匹配status、原始Content-Type值、JSON和Location。
  API模式29项稳定观察逐项比较，共32项双实现配对；另保留实际就绪/收敛轮询的原始HTTP。
  live成员只按节点身份排序、归一化实际地址与编译/启动元数据；六字段原值全部保留。
  其余固定输入的JSON、错误文本、Content-Type、Location及重定向正文均原样比较。
- API目录：空列表为null；未知服务500；四种合法服务名；编码的服务名；GET 301与POST 307；
  六条持久记录中错误JSON及错误数值类型被跳过，null记录为零值条目，重复字段按顺序覆盖，
  null不覆盖已赋字符串。真实lease到期后成员从目录消失。
- 同一真实etcd WAL及PD重启后，固定注册目录的bytes/create/mod revision逐项不变，
  HTTP目录恢复。Go与Rust使用隔离进程、数据目录和backend，未用MemoryStore替代。
- 每侧启动两个真实独立TSO进程。宿主注册具有非零lease，六字段类型有效；
  等待共享组目录包含两个成员后，通过HTTP primary查询及真实双向TSO流确认当前服务者。
  缺失/未注册目标、JSON语法/类型、重复字段/null、无效替代节点、自驱逐与scheme别名、
  4097字节请求和已注册但无法连接的目标均有相同HTTP错误合同。
- 未完成请求体的TCP断连不写marker；错误目标不改变当前primary。
  POST经PD代理到真实节点，scheme别名仍转到注册的canonical地址。
  返回`{"0":"success"}`后primary切到指定另一节点，leader bytes和lease均变化，
  时间戳严格递增，expected-primary marker被胜选者清理；重复驱逐旧节点返回空对象。
- 驱逐前保持打开的旧gRPC流及驱逐后新建的旧节点请求均不能再发放时间戳。
  原始错误分别为Go Unknown/ErrGenerateTimestamp、Rust Unavailable/leadership lost；
  本项验收旧任期停止服务，不把这两个不同gRPC错误码冒充全协议等价，协议矩阵仍由任务22负责。
  两个TSO进程SIGTERM成功退出后，注册目录收敛为null。
- 领域真实etcd故障测试验证：旧任期转移失败且不能覆盖新marker，旧清理不删除新marker，
  正确清理同时撤销lease；停服读失败不能成为空成功，同WAL重启后读回真实状态。
  注册测试在撤销旧lease并写入替代实例后，确认延迟续租和旧关闭均不覆盖/删除新实例。

## red与修复

首次已注册的`sa_013_behavior`实际0通过/1失败，退出101：成员GET为Rust 503、Go 404。
扩展矩阵发现Go错误类型名为`apis.EvictPrimaryRequest`，修复Rust错误文本后继续验证。
保留原始HTTP后发现共用测试助手曾把整个Content-Type值小写化；本项改用独立原文记录，
并修复MCS适配的`charset=UTF-8`为Go的`charset=utf-8`，没有通过归一化隐藏差异。

夹具曾在Go新primary尚不能发放时间戳时立即请求，以及在首次分组目录调整时固定旧节点；
已分别等待实际组成员、当前primary和真实RPC就绪。编译错误（含漏接证据变量）、
Cargo.lock待更新导致的`--locked`拒绝及夹具时序失败均单列保留，不算功能red或通过。
原始失败日志未覆盖。最终两项sa_013均真实执行，无提前返回、无ignored、无零测试豁免。

## 验证结果

默认features，以下有效行为检查均退出0，重复运行不累计为新测试：

| 命令 | 有效结果 |
| --- | --- |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_013_ -- --nocapture` | 2通过，包含32项稳定HTTP配对、WAL、真实双TSO驱逐与旧长流拒绝 |
| `cargo test --locked -p astersql-pd-server --lib task_078_ -- --nocapture` | 2通过、1历史ignored；当前classic MCS合同及未知路径保护 |
| `cargo test --locked -p astersql-pd-mcs-tso-server --lib mcs_082_ -- --nocapture` | 1通过，真实注册/重复地址/就绪/关闭 |
| `cargo test --locked -p astersql-pd-mcs-discovery --lib -- --nocapture` | 1通过，六字段/未知服务/损坏记录/取消/lease归属与替代实例保护 |
| `cargo test --locked -p astersql-pd-mcs-utils --lib -- --nocapture` | 1通过，candidate、marker CAS、旧任期、lease清理与真实停服/WAL恢复 |
| `cargo test --locked -p astersql-pd-tso --lib -- --nocapture` | 11通过，包括实际分组监听、取消、故障、handoff及WAL相邻回归 |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_011_mcs_144_behavior -- --nocapture` | 1通过，原有Go/Rust拆合及真实MCS实链 |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_002_ -- --nocapture` | 1通过，82 packages、745生产源码、6负向控制 |

去重20通过、0最终失败、1历史忽略。本项所需Go运行矩阵独立执行，不依赖该ignored。
`cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server`、
`cargo metadata --format-version 1 --no-deps`、`cargo fmt --all -- --check`、
`cargo check --workspace --locked`及`git diff --check`均退出0。
新canonical crate仅复用已有workspace依赖；Cargo.lock只增加本地包与必要依赖边。
完整workspace test按本计划集中任务24。

构建/行为动态领取槽位1：
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
一次格式化及锁文件检查阶段动态领取槽位2：
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-2`；
该阶段两个`--locked`命令因依赖边刚删除而拒绝运行，0有效测试，随后离线更新锁并重新验证。
全部本项子进程结束、自有锁释放、缓存保留；没有清理他人的槽位或进程。

Go源码/模块1101项首尾存在性、SHA及受保护Git状态一致；SDK、固定协议ZIP、Go oracle、
PROTOC与官方etcd3.5.15/etcdctl重验准入摘要。未生成临时Go、运行make、failpoint、
Go生成或模块整理。所有plan.md及来源史诗保持只读，保留既有无关删除和未跟踪计划。
