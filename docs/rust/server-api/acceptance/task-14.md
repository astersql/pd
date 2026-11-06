# 任务14：Diagnostics/SearchLog 日志流验收

2026-11-06，基准 `a74eae97e`。状态：已完成。
[原始回执](task-14.json.gz)包含本轮命令、退出码、失败与通过日志、输入日志字节及
SHA-256、RPC请求和分批消息、终止状态、二进制/来源指纹、Go首尾守卫与清理记录。

来源为 [wire-inventory.md 第99行](../wire-inventory.md#L99)，交接ID
`wire:a5a03ba0ba4c676c25e3177d`；只验收 SearchLog，ServerInfo 仍由任务15负责。
上游任务13的真实公共接口和已保存证据已核对；本项无新增领域前置。
Go `server/server.go:279` 将宿主 `cfg.Log.File.Filename` 传给固定版本 sysutil，
`:332` 注册 Diagnostics；使用准入的现成 Go 二进制，没有编译或修改 Go。

## 实现与真实差分

- 在现有 canonical listener 注册 Diagnostics；直接消费宿主已有 telemetry config
  的日志路径与宿主取消令牌，独立于 leader term。没有新增日志器、私有 runtime、
  audit 替代入口、领域算法或后端实现；本地文件读取不需要新 WAL/etcd 耐久性认证。
- 窄模块按 sysutil 解析统一日志时间/等级/正文；非法行继承前一有效记录的时间和等级。
  保留正文、重复字段与跨文件顺序。轮转文件按前缀/扩展名选择、探测首十行及尾部反向
  窗口、按起始时间读取；不合格文件及无法读取文件跳过，文件资源以 RAII 回收。
- 时间范围闭区间，end=0 无上界；所有 pattern 为 AND，复用已有 Go regexp 适配。
  未知日志等级保留；已知等级按 int64 bitset 过滤，空等级不限制。
  每批最多1024条，整批末尾追加空批次，空结果也发送一个空批次，随后正常 EOF。
- blocking pool 执行本地 IO；有界 channel 防止全量缓存搜索结果。流丢弃和宿主取消
  都停止读取、释放文件；宿主中断返回取消，不能把未完成搜索当成成功 EOF。

当前真实 TCP/tonic 双实现逐项比较 **41个请求场景**：空日志配置；多轮转文件，
文件名顺序与时间顺序相反；非法首/尾部、目录和错误扩展名排除；CRLF、无末尾换行、
多行日志与重复正文；闭区间、反向区间、无匹配文件、end=0；全部声明等级、未知等级、
重复等级及位64；多pattern AND、ASCII字符类、引用字面量、大小写和或模式；
Slow target沿用sysutil忽略行为；非leader读取接收节点本地文件；严格日期格式与
非法时间戳继承；0/1/1023/1024/1025/2048条分批；客户端取消后新请求；9种非法正则。
请求、批次、每条time/level/message、错误code/message及正常EOF逐条原值相等，
没有排序消息、重排重复字段或归一化日志时间。固定夹具在两侧的字节/摘要一致。
固定输入写入同前缀轮转文件，保留宿主活动日志；动态宿主日志由固定时间范围/正文
pattern排除，不将不同宿主日志字节伪装成相同输入。

两侧另以超过传输流控容量的真实流检查客户端reset和SIGTERM：读取首批后中断，
`lsof`确认相关轮转文件不再打开；宿主成功退出，夹具进程与目录由现有harness回收。
非leader证据保存真实members响应，确认leader为compat而接收节点为follower14；
本地日志包含唯一标记，leader日志不包含它。

关闭整个进程的原始终止结果保留其差异：Go为客户端观测的Unknown/h2 body读取错误，
Rust为Cancelled/context canceled。这里验收二者均中断而非成功EOF、无文件泄漏，
**不宣称进程关闭时的传输错误码或文本等价**；该连接级差异交给任务22的协议矩阵。
正常耗尽EOF与请求级错误则在上述41场景中精确比较。

## 失败证据与修复

首次可运行 `sa_014_log_stream`：Go返回已知日志，Rust返回Unimplemented，
0通过/1失败/0忽略，退出101。实现后38场景通过；扩展严格日期矩阵再次得到真实
0通过/1失败、退出101：Chrono把非补零月份当有效日志，而Go将其当续行。
收紧日期宽度后40场景通过，再补充真实非leader场景，最终41场景通过。
原始失败日志与原始日期差分回执均保留，不以最终日志覆盖。

最初接入生成的tonic默认桩时还遇到关联流类型和私有BoxStream别名编译错误；
修为实际 `tonic::codegen::BoxStream`。这些编译失败为0有效测试，单独保存，
不计作行为red或通过。
非leader扩展曾暴露夹具将活动日志文件截断后仍由真实logger写入的问题：Go首批实际
1021条而非1024条。改为把固定输入写入轮转文件，保持原条数断言；该失败单列保留，
不把夹具竞争记为产品缺陷或弱化断言。

## 验证与环境

默认features，最终均退出0：

| 命令 | 有效结果 |
| --- | --- |
| `cargo test --locked -p astersql-pd-server --lib sa_014_ -- --nocapture` | 2通过、0失败、0忽略 |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_014_ -- --nocapture` | 1通过、0失败、0忽略；41个双实现配对 |
| `cargo test --locked -p astersql-pd-server --lib task_078_ -- --nocapture` | 2通过、0失败、1历史ignored |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 成功 |
| `cargo metadata --format-version 1 --no-deps` | 成功 |
| `cargo fmt --all -- --check` | 成功 |
| `cargo check --workspace --locked` | 成功 |
| `git diff --check` | 成功 |

去重5个测试通过，0最终失败、1相邻历史忽略；本项行为测试没有ignored或提前返回。
完整工作区test遵照总计划集中任务24；没有重复上游领域验收。
已有deprecated/dead_code警告未扩展为本项改动。

每阶段原子动态领取的实际槽位均为1：
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
所有Cargo及子进程继承该绝对路径，阶段结束释放本项锁，编译缓存保留。
准入的Go oracle、SDK/build info、固定kvproto ZIP、PROTOC、官方etcd3.5.15和etcdctl
均核对路径与摘要；真实Rust宿主使用现成官方etcd，未回退Go fixture构建。

Go源码/模块1101项存在性、SHA-256和受保护Git状态首尾一致；没有生成临时Go，
没有执行make、failpoint、Go生成/格式化/模块整理或Go测试。所有plan.md及来源史诗
保持只读；保留开始前已有的无关删除和未跟踪计划，仅提交本项实现、测试和证据。
