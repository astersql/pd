# 任务11：时间戳分组拆合HTTP验收

2026-11-06，基准 `cb68a99ac`。状态：已完成。
[本轮完整回执](task-11.json.gz)保留命令、退出码、原始失败/通过日志、来源及输入摘要、
HTTP原文、gRPC观察、etcd持久字节与revision、Go只读守卫和清理结果。

来源为 wire-inventory.md 的 HTTP-205–209。五条 POST/DELETE 注册从只读
`RegisterTSOKeyspaceGroup` 提取，不沿用历史26条的计数；另覆盖组PATCH、节点通配PATCH、
具体路径遮蔽通配方法及其尾斜杠变体。只移除本项五个拒绝方法，其余六个未准入方法保留。

批次10及原31、32、34、42、76、131的实际接口、已有故障证据与本轮相邻回归已核对。
本轮在 canonical `pkg/keyspace` 补齐首次拆分前旧键空间归属巡检和分批merge-all等待；
在 canonical `pkg/tso` 完成所需窗口交接及监听取消重放；
在原82对应的 `pkg/mcs/tso/server` 建立唯一的 `astersql-pd-mcs-tso-server` 库宿主。
HTTP只做绑定和调用既有管理器，没有在transport复制拆合或回收算法。
原82/144的本项接口和实链证据在原覆盖表交接，不把其它RPC、指标或独立服务的整体验收一并销项。

## 当前行为证据

- classic/API分别启动隔离Go/Rust进程和数据目录，真实TCP比较97/98项HTTP观察：
  status、Content-Type、JSON、Location及重定向正文。每模式86组前后持久快照，
  两实现的实际目录字节完全一致；检查创建revision保持、更新严格递增及多组同次提交关系。
  独立审计再次核对172组事务的字节、写入集合及revision关系；绝对revision全部保留。
- 列表/范围拆分、重复ID去重、受保护键空间及副本/状态/缺失组错误、完成拆分的源/目标角色、
  合并/重复完成/默认组保护、非法merge-all组合、节点调整在过渡状态的拒绝及通配优先级PATCH。
  JSON语法/类型/负uint32、字段顺序与重复字段、null标量保留、null字符串元素及int溢出文本均与Go比较。
- 五条注册分别比较真实第二节点转发；未bootstrap门禁、五种尾斜杠PATCH、POST/DELETE重定向均有本轮观察。
  拆分/合并/回收的未完成请求体断连不写目录；重启PD及其etcd WAL后字节/create/mod revision不变。
- archived/tombstone键空间从组内回收，enabled、缺失及重复ID按Go规则处理，最终仍只保留enabled键空间。
  原42的真实事务容量、取消、模糊提交、领导权fencing及WAL故障回归10通过。
- 对两个真实HTTP进程放入同样的旧键空间元数据，首次拆分前巡检写入默认归属，随后拆分/完成结果一致。
  canonical巡检另验证失败后重试、组与元数据同revision、只执行首次成功巡检，以及范围外整页不得循环。
- Go oracle直接运行现有 `services tso`，Rust运行上述库宿主；各有两个真实TCP/tonic副本及真实选主租约。
  HTTP拆分10→11、合并11→10、merge-all→0，TSO流响应及FindGroup重新查询均返回实际归属，主成员恰有一个。
  每一步发号严格高于迁移前值；重启全部PD/etcd WAL/TSO资源后继续严格递增。
  只归一化进程地址的角色标识及默认组自动分配的非确定成员顺序，显式成员顺序继续逐项比较。
  Go实际分组监听也直接使用共享etcd；本项没有虚构额外MetaStorage RPC跳转。
- Rust交接读取源的真实持久窗口，先以metadata/window compare和目标选主事务保存严格上界，
  合并时使旧源窗口失效，再经PD现有HTTP完成入口发布状态；嵌入宿主可调用同一注入管理器的条件完成API。
  拆合期间旧目录条件失效即拒绝发号，不能等watch消费删除事件后才停止。
- 宿主复用调用方Tokio handle/取消令牌/同一MetaStore及管理器；加载赋值后注册真实租约。
  错误后关闭监听器、重复注册不取消首个宿主；shutdown终止开放流、worker、分组租约及注册租约，保留父令牌。
  未创建私有runtime、安装全局信号或全局指标注册器。

## 失败及修复

有效行为red均保存：初始503门禁0通过/1失败；删除分组后旧分配器仍发号0/1；
稀疏范围巡检重复整页0/1；重复字段null丢失标量0/1；尾斜杠PATCH的405与Go500差异0/1；
数值字符串元素及int溢出错误文本1通过/2失败；取消资源应用后丢失已消费watch批次0/1。上述red退出101。

最后一项由真实etcd、实际revoke RPC出口背压及SIGSTOP/CONT故障重现。
修复保留批次直至状态发布，并把宿主20ms预算限制在接收阶段，资源应用不再被轮询超时中断。
另修复默认组隐式成员与持久DTO差异造成的交接等待。

夹具问题单独记录，不计为实现red：Go合并检查器5秒周期需要足够等待；流发送侧保持开放直至收响应；
停止二节点etcd成员后不能再读取失去quorum的目录；默认组自动分配顺序不能按固定顺序比较；
独立测试模块的fixture计数器曾碰撞，现共享同一个计数器；初版watch故障未固定revoke边界，不计有效red。
Go巡检suite初始化依赖未注入failpoint，该次失败未作为oracle证据；替代的真实HTTP旧元数据场景通过。

## 最终验证

默认features，以下命令退出0；原始命令与完整日志见回执：

| 命令 | 有效结果 |
| --- | --- |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_011_ -- --nocapture` | 5通过，0失败/忽略 |
| `cargo test --locked -p astersql-pd-server --lib sa_011_ -- --nocapture` | 3通过，0失败/忽略 |
| `cargo test --locked -p astersql-pd-server --lib task_078_ -- --nocapture` | 2通过，1既有ignored |
| `cargo test --locked -p astersql-pd-keyspace --lib mcs_03 -- --nocapture` | 64通过，0失败/忽略 |
| `cargo test --locked -p astersql-pd-keyspace --lib mcs_042_ -- --nocapture` | 10通过，0失败/忽略 |
| `cargo test --locked -p astersql-pd-tso` | 11单元+9集成通过；0 doc-tests不计行为 |
| `cargo test --locked -p astersql-pd-mcs-tso-server` | 1通过；0 doc-tests不计行为 |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_010_ -- --nocapture` | 3通过，0失败/忽略 |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_002_ -- --nocapture` | 1通过：80包、734生产源码、6负向控制 |

`cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server`、metadata、workspace check、
fmt check及diff check通过。新MCS包仅登记为精确的协议适配边界；伪装为server的领域目录仍被负向控制拒绝。
Go现有只读测试二进制的拆分/范围/合并3个叶例另实际通过，suite父事件不重复计数。
完整workspace test按调度集中任务24；没有将缺失必要领域或oracle降为待回归。

最后11条命令均退出0。临时槽位脚本在运行中补充owner字段后，旧进程尾部因文件偏移变化报解析错误；
这发生于所有命令完成之后，退出trap已释放锁。随后重新检查脚本语法，并在新持锁进程执行
fmt/workspace check/diff check，驱动整体退出0；不将这次驱动错误记为测试通过。

Go源码/模块1101项首尾存在性、SHA及Git状态完全一致；未写临时Go、注入failpoint或运行make/生成/整理。
固定kvproto ZIP、Go oracle、SDK归档、PROTOC和官方etcd3.5.15/etcdctl均重验摘要。
主验证动态领取槽位1，`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`；
早期补充验证使用槽位2，`/Users/xiangmin/worker/code/pd/target/rust-slot-2`；
一次纯格式化使用槽位3，`/Users/xiangmin/worker/code/pd/target/rust-slot-3`。
所有本项子进程已结束、自有锁已释放、缓存保留。总plan、领域plan和来源史诗保持只读，保留既有无关变更。

本项认证限于列明HTTP合同与所需共享状态/发号链，不扩大为其它MCS RPC矩阵、TLS、原生生产后端或Go退役认证。
