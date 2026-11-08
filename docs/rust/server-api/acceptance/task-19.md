# 第 19 项源码来源与行为归因

审计基准 `5f84295157d7ed78d9709fabe2896c0900a5e615`。
[来源清单](../source-inventory.md)和[责任交接](handoff.md)保留原始行。
[机器回执](task-19.json.gz)记录逐路径旧/新 SHA-256、变动、Git 归因、
原拥有者、候选测试、回执定位及未准入原因。入口为
[coverage.json](coverage.json) 的 `source_audit_19`。

## 来源集合

- 按原 Markdown manifest 的三列解析路径、owner 和 SHA-256。路径按仓库
  相对标识去重，Markdown 显示路径必须与链接目标一致。实际文件按原始字节
  哈希。历史集合仍为 1909 项，摘要
  `a6765e82a34496928c803b5cd616955b17c430bd5ebd243aa16239c9c5cdee91`。
- 扩展集合以历史基准 Git 树为起点，核对源码、工具、脚本、测试和 Rust 证据
  目录中后续出现的路径，以及本轮直接引用的行为来源。对新增文件记录改动
  提交和现存位置。对历史哈希漂移保留旧值和最近一次相关提交。
- 旧服务端计划的 `plan.md`、`prompt.md` 在工作开始前已在本地删除。
  保留原 manifest 条目和 Git 中的字节摘要，后继责任见交接页。干净 checkout
  若仍含旧文件，校验其记录的 Git 字节。其余来源必须在磁盘存在。
- 本清单文件整体摘要由
  `38de1e710ebe0b9db119fd670be56a40a77ed1e38b79099ae53527ebafc4c65c`
  更新为 `0d6895d86752cd4b93f3657d3f1af8fa6dbac909ce56d31bb2c14f567669a3ee`。
  审计索引、回执和本说明独立存放，避免回执哈希自引用。

## 行为归因边界

同输入运行绑定当前源码、确切命令、二进制指纹及 Go/Rust 原始观察。
旧回执的单路径哈希即使匹配，也仅记为候选输入。指标属性只用于对应指标检查。
分项审计不会扩大场景覆盖，逐路径记录保留缺少回执的原因。全协议、HTTP
矩阵和整仓行为回归继续分别归第 22、23、24 项。

批次 2 的守卫在后来加入的 MCS 适配层上出现误报。核对 ADR 后，显式登记
承担协议或持久化字节转换的 crate 和源码目录。Router 和 Scheduling 复用 PD
传输适配的依赖边单独登记。现有领域到协议、运行时、测试层的反向依赖检查
以及库生命周期边界继续执行。

## 本轮行为与阻塞

- 三个旧版 GC RPC 同跑固定 Go 和当前 Rust。回执保留正常响应、follower
  拒绝、重启和实际物理键值。SearchLog 同跑两边，保留请求、分批响应、终止
  状态、取消及文件关闭结果。
- 正式 Rust 入口的当前 debug 构建通过真实进程探针。SIGKILL 后保留
  506792 字节 WAL，重开后读回已确认的 MetaStorage 字节与 revision 11，
  ID 从 1 前进到 1001，TSO 前进。固定 Go oracle 同跑核心请求。
  release 和嵌入拓扑的原证据见 [任务 18](task-18.md)。
- 首次审计中，批次 16 的 RM 代理取消隔离缺双边同输入运行证据。批量补跑中第二条
  token 流超时。单独补跑退出 101，固定 Go oracle 在第一条前端连接关闭后，
  第二条仍在使用的流返回 `Unknown: EOF`，Rust 分支尚未执行。
  Go `AcquireTokenBuckets` 的非 EOF 错误路径关闭目标共享 delegate，
  与观察一致。保留原断言及 [任务 16 历史记录](task-16.md)，把本轮差异
  单独登记在 `historical_incomplete_cases.rm-proxy-independent-stream`。

## 验证

| 命令或阶段 | 退出码及有效测试 |
| --- | --- |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_019_ -- --nocapture` | 初次 101，1 项因缺少当前审计行失败。最终 0，2 通过，0 失败，0 忽略 |
| `cargo test --locked -p astersql-pd-compat-tests --lib task_102_source_ -- --nocapture` | 0，2 通过，0 失败，0 忽略 |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_00 -- --test-threads=1 --nocapture` | 101，16 个第 3–9 项行为通过，依赖方向守卫 1 失败，0 忽略 |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_002_ -- --nocapture` | 守卫修正后退出 0，1 通过，6 个负向对照 |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_01 -- --skip sa_019_ --test-threads=1 --nocapture` | 101，59 通过，5 失败，0 忽略。保留原始失败，并分别复跑 |
| 旧版 GC、原生 WAL、SearchLog 各自的精确入口 | 均退出 0，各 1 通过，0 失败，0 忽略 |
| MCS TLS backend、进程配置补齐固定 etcdctl 后各自复跑 | 均退出 0，各 1 通过，0 失败，0 忽略 |
| RM 双流取消隔离单独复跑 | 101，0 通过，1 失败，0 忽略 |
| `cargo metadata --format-version 1 --no-deps`、`cargo fmt --all -- --check`、`cargo check --workspace --locked`、`git diff --check` | 均退出 0 |

本轮先校验已存 Go oracle、SDK、固定 kvproto ZIP、protoc、官方 etcd 3.5.15
和 etcdctl 的字节摘要。原准入文档的缓存目录缺该 ZIP，改用已有且同摘要的
只读缓存。精确命令、二进制指纹、原始日志和 Go 守卫在机器回执。

ServerInfo 首次对照混用了 arm64 Rust 与 amd64 Go，内存页大小不同。
以 `CARGO_BUILD_TARGET=x86_64-apple-darwin` 构建当前 Rust 后，
同架构复跑退出 0，1 通过，0 失败，0 忽略。保留两边原始采样与 RPC，
测试完成后恢复槽位的 arm64 默认产物。

最终逐字节核验 3122 个当前路径。原清单中 1697 项字节一致、210 项漂移，
另记录 1212 个新增文件、3 个本轮直接引用的既有来源和 2 个已移除的旧入口。
4 个当前通过的业务场景关联 13 条来源行，RM 取消隔离的失败另列。

实际使用 Cargo 槽位 1 和 2，绝对路径分别为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`、
`/Users/xiangmin/worker/code/pd/target/rust-slot-2`。测试及子进程退出后释放锁，
保留构建缓存。1101 个受保护的 Go 和模块路径首尾字节及 Git 状态一致，
守卫记录 SHA-256 为
`50b0502587d211282ec08855b2dcbc46f9db7e4d55e4d52881ca05d7256e1e3a`。

## RM 双流归因复验

以 `6fb38f37c` 为续跑基准。原测试与独立 Rust 测试共用同一夹具。
PD 使用 API 模式并关闭嵌入 RM fallback，两条已建立的前端流请求同一真实
Go RM primary。两边的建组请求、5/7/9 token 请求及关闭前两条响应的 protobuf
字节一致。真实 etcd 读回的组配置字节和修订号在各自进程的故障前后保持一致。
两边均在故障观测完成后查询 primary 并正常关闭进程。

- Rust 单独执行退出 0，第二条流继续返回 9 token。
- 默认时序的严格双实现重跑有一次退出 0。Go 已返回 9 token 后仍记录了
  delegate 关闭。此前的超时和 `Unknown: EOF` 均保留。
- 对照运行在第一条 TCP 前端关闭后，等待 Go 对实际 RM 地址记录
  `close delegate client connection`，然后在第二条流发送原有的第三帧。
  日志计数从故障注入前开始，排除启动阶段的旧关闭记录。
  arm64 Rust 与 amd64 Rust 分别对照同一 amd64 Go，均观察到 Go 的第二条流
  以 `Cancelled` 结束，Rust 在同场景返回 9 token。
  Go 源码中的 `reportErr` 经 `closeClient` 关闭按目标地址复用的连接。
  关闭发生在客户端及服务清理之前。
- 四种 PD/RM 组合的正常半关闭与上游丢失不重放各 1 项通过。请求头、组绑定
  和拓扑沿用原夹具，首两帧的成功响应与物理键值也均已记录。

第 19 项的归因证据已齐备。机器回执将当前双边复验存入
`observed_divergences.rm-proxy-independent-stream`，兼容断言的结果为失败。
[批次 16](task-16.md) 原有通过记录仅覆盖其当时的时序，该行仍是兼容缺口，
不计入当前通过场景。第 19 项的审计入口核对这条失败回执与通过回执的不同归类。

复验重新采集旧版 GC、SearchLog、ServerInfo、原生 WAL 各 1 个成功回执，
并重跑 `sa_019_` 和 `task_102_source_` 各 2 项，均退出 0。
两次等待 delegate 关闭的严格测试均退出 101，各 1 失败、0 忽略。
本次使用槽位 1，`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
锁已释放，缓存保留。1101 个 Go 源码及模块路径的首尾守卫一致，格式和 diff 检查通过。
