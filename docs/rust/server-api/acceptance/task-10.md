# 任务10：时间戳分组目录HTTP验收

2026-11-06，基准 `213abc35e3f0a22782e92af6e4fdaae8f63464ff`。状态：已完成。
[本轮原始回执](task-10.json.gz)包含完整命令、退出码、失败/通过日志、输入与源码指纹、
原始HTTP/gRPC观察、真实etcd元数据bytes/create/mod revision及Go只读守卫。

来源为wire-inventory.md:330–336的HTTP-198–204；方法和路径由只读
`RegisterTSOKeyspaceGroup`逐条提取：创建、列表、详情、删除、设置节点、设置优先级和分配节点。
PATCH含义已从Go handler核验。`/alloc`、`/split`、`/merge`、`/keyspaces`上的PATCH仍是节点优先级通配路径，
显式补齐Axum具体路径遮蔽通配方法的接线。拆分/合并/回收的POST/DELETE保留原拒绝，剩余11条注册未被误删。

批次9及29、30、33、35、131的公共API和证据已核对。本轮推进真正所需的原拥有任务
[72](../../control-plane-mcs/task-072.md)、[73](../../control-plane-mcs/task-073.md)、
[76](../../control-plane-mcs/task-076.md)，复用canonical Oracle、Leadership及ReconciledWatch。
HTTP薄适配委派同一宿主KeyspaceGroupManager；任期拥有的1秒工作循环调用既有成员调和逻辑，
注册查询来自共享MetaStore，旧任期写入仍经TermStore fencing。
API模式停止创建classic Oracle，避免与默认MCS分组争用同一个时间戳持久键。
MCS协议服务/代理、拆分与合并时间戳交接、原生生产后端分别由原拥有任务及后续验收负责。

本轮行为证据：

- classic/API各自运行隔离Go/Rust真实TCP进程与数据目录；分别47/48项HTTP观察，
  每次运行保留38组前后持久快照。比较status、Content-Type、JSON、Location和重定向正文。
- 默认basic与显式standard/enterprise、批量创建与整批重复回滚、分页多取一条、空页、隐藏键空间、
  state过滤含空值、uint64→uint32按ID转换、默认组删除保护、缺失组null和非法参数。
- JSON语法、顶层/字段/嵌套ID类型错误的Go错误文本及text/plain绑定响应。
  缺失成员的优先级PATCH保留Go实际400状态和连续两段JSON，持久bytes/revision不变。
- 两实现使用同一组真实TCP候选端口；设置两节点、扩到三副本、优先级-7、移除注册后的拒绝，
  比较完整响应和持久DTO。仅对Go遍历existing-members map产生的非确定成员顺序排序；原始顺序保留。
- 默认组与enterprise组由真实宿主自动调和；独立adapter重建三类user-kind索引并核对HTTP元数据。
- HTTP创建enterprise组101→创建键空间201自动归属101→Keyspace gRPC读取相同配置→HTTP目录含201，
  直接证明宿主共享manager与实际user-kind归属保持一致；完整gRPC观察和物理状态保留。
- 服务侧装配三个真实GroupAllocatorWatch，使用同一持久目录、实际选主lease和Oracle发号；
  发号者与HTTP primary一致。清空成员后移除组10/12的实际allocator，shutdown回收工作者和lease。
  初始primary缺失与真实protobuf participant记录的转换分别验证。
- 从Go源码提取的七条注册逐条比较leader和真实第二节点转发；未完成请求体断连不写目录。
  Go/Rust分别重启PD及其etcd WAL，目录bytes/create/mod revision保持不变。

失败证据均保留：初始503为0通过/1失败，exit101；实际差分随后发现列表参数检查优先级、
分页多取一条、primary错误文本、缺失宿主自动调和、API/classic Oracle争键、通配PATCH被遮蔽、
嵌套ID绑定错误及优先级拒绝双段JSON。原72另取得保留空间错误的真实red。
编译期夹具错误没有计作行为red。一次双节点夹具在停止成员后丢失quorum，已将转发验证移到重启后；
一次错误假定Go空state等于未设置，真实输出证实应返回空列表，已恢复实际合同。
早期动态槽位阶段间使用过旧二进制；最终验证在同一持锁阶段构建并使用该槽位二进制。

最终验证（默认features，以下均exit0）：

- `cargo test --locked -p astersql-pd-compat-tests --lib sa_010_ -- --nocapture`：3通过，0失败/忽略。
- `cargo test --locked -p astersql-pd-server --lib task_078_ -- --nocapture`：2通过，1既有ignored。
- `cargo test --locked -p astersql-pd-tso --lib mcs_072_ -- --nocapture`：2通过；`mcs_073_`：1通过；`mcs_076_`：2通过。
  补录原始时间戳和compaction观察后，`mcs_0`再次5通过，0失败/忽略。
- `cargo test --locked -p astersql-pd-tso`：8单元+9集成通过，0失败/忽略；零doc-tests不计行为证据。
- runtime的`readonly_classic_tso_differential -- --ignored`、`task_036_clock`、`mcs_131_`分别1通过；`mcs_132_`2通过。
- keyspace的`mcs_03`61通过；相邻`sa_009_`4通过，均无失败/忽略。
- `sa_002_`架构守卫1通过：79包、727生产源码、5负向控制。
- `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server`、metadata、workspace check、fmt check及diff check通过。

完整workspace test按调度集中任务24。Go既有测试在拒绝写仓库/模块缓存/SDK的隔离沙箱构建，
选定的6个叶例不依赖failpoint注入，实际执行通过；父suite及包pass事件不重复计数。
Go源码/模块1101项首尾存在性、SHA与Git状态一致，没有手写或修改Go源码/模块，没有make、注入或整理。
Go oracle、固定kvproto ZIP、SDK归档、PROTOC及官方etcd3.5.15/etcdctl均核验并记录指纹。
最终验证和补录均为动态槽位1，`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`；
较早阶段也动态领取过槽位2，路径为`/Users/xiangmin/worker/code/pd/target/rust-slot-2`。
全部本项进程退出后释放自有锁，缓存保留；所有plan.md及来源史诗保持只读。
