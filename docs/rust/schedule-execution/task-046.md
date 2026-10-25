# 任务046：合并可行性判断

状态：已完成。全部必需验证退出0。生产实现位于 `pkg/schedule/checker/merge_guard.rs`，公开
`allow_merge`、`check_peer_store`、`merge_target_allowed`，在 canonical checker
crate 注册。只实现合并许可与结构性目标安全；后续任务负责热度/最近拆分门控、
完整 checker 生命周期、大小选择及 merge operator。没有新增 crate 或 feature。

## 依赖和来源

批次45由提交 `0b6fc9acd`（schedule/checker: port rule orphan and location repairs）
和[任务45证据](task-045.md)确认；其2测试/40场景及workspace1474测试通过。
本次包测试再次运行前序16项测试。复用真实 codec、core、config、placement、labeler、
filter，不复制规则、表身份或副本拟合算法。Cargo.lock 仅新增 codec/labeler 两条内部
路径依赖；离线 generate-lockfile 附带的 pulldown-cmark 升级已排除，远程版本未改。

已核查 oracle-index.json。用户后续授权解除 Go 运行 oracle/运行差分准入限制；
本任务是**只读 Go 源码及测试契约推导 + Rust 行为验证**，没有执行 Go、没有 Go 原生
输出，也不声称取得运行差分。[来源记录](logs/task_046/source-provenance.json)保存
当前来源 commit、Go 文件 SHA256 和最终 fixture SHA256；没有虚构 Go 输出哈希。

主要来源：merge_checker.go 的 AllowMerge/isSameTableIdentity/checkPeerStore/
checkTarget；merge_checker_test.go 的 TestAllowMergeCrossTable 十二项表身份矩阵，
以及 TestMergeCheckerTestSuite 中离线 store、异常副本、规则与 merge_option 分支。
codec.go 的 TableIdentity 对解码错误返回零 identity；filter/healthy.go 的健康及
replication 判定；placement/fit.go 与 labeler/labeler.go 的 fit/范围/标签语义。
TestMain 只是 suite 入口，不算独立行为覆盖；以上 Go 名称均为只读来源定位。

## 固定场景和真实组合

[fixture](fixtures/task_046.json)包含103个许可场景、22个目标安全场景，全部期望值
显式冻结。表驱动输入通过确定性 codec helpers 生成 memcomparable classic/API v2
边界，generation 与 Go 测试契约一致；不从待测 guard 生成期望。无随机字段和
归一化丢弃；标签时间用明确的 Unix 纳秒输入，TTL严格边界分别检查。

许可场景覆盖 classic/same-keyspace/different-keyspace 的同/异表、classic 与
keyspace0 身份区别、负表ID、malformed key、table/raw/txn/unknown policy 与
cross-table 开关组合；正反方向、间隙/重叠/空共享边界/无穷端点；缺失 placement
能力、内部分割键、起止点规则边界、禁用placement；label内部/外部/端点分割、
source/target/global deny、大小写、其他值、TTL相等及超过边界。每行检查两个方向。

目标场景覆盖 legacy 与真实placement两种模式下，健康副本、缺失/移除中共享store、
缺失/移除中新目标store、健康新store、down（含0秒）/pending/learner、过少/过多副本。
每行另检查 absent target。共享 removing store 合法；共享 missing store 的 legacy
许可不意味着其满足 placement，真实 fit 拒绝缺失 store。共125个固定场景，250次
布尔断言（206次许可、22次目标、22次absent），2个Rust测试，0忽略。

测试使用真实 RegionInfo/StoresInfo、RuleManager、RegionLabeler 和 canonical filters。
初始化规则通过现有真实 MemoryStore/StorageEndpoint 后端；生产判断不写存储、不
分配ID、不生成操作，因此不增加无关持久化/allocator故障模拟。错误输入按Go返回
false或零identity；新增API没有需要包装的I/O错误。

## 失败验证和错误识别

[red-api.log](logs/task_046/red-api.log)记录缺失模块/API的exit101（另发现test中
initialize需要普通StoreInfo切片，已修正）。首次 --locked 因内部依赖接线要求更新
锁文件，随后离线更新。green-first/green-2 记录规则role缺省无效、role必须使用
PeerRoleType以及共享removing store仍能fit的测试输入修正；这些不冒充错误识别。

[mutation.py](logs/task_046/mutation.py)逐项注入8类错误：允许空共享边界、只比较
numeric table ID、跳过placement、跳过label split、错认deny、绕过peer store检查、
跳过health、跳过replication。每项均exit101，且由真实行为assertion失败识别，
没有用编译失败充数。[mutation-results.json](logs/task_046/mutation-results.json)
从实际逐项日志记录汇总；runner的mutations.json记录阶段成功退出，二者路径分开。
finally恢复正确生产代码后重新格式化并执行完整基线。

## 验证与共享槽位

[run.py](logs/task_046/run.py)每阶段从1至10原子mkdir动态领取共享槽位，并记录
任务/会话/PID，等待子进程结束后释放自己的锁。所有实际阶段均领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[validate.py](logs/task_046/validate.py)和[commands.json](logs/task_046/commands.json)
保存确切命令、退出码及测试计数；最终基线设置 `RUST_TEST_THREADS=2`。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_046 -- --nocapture` | 0 | 2通过、125固定场景、250次断言 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 18通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1476通过、0失败、1既有忽略 |

## 交付审计

[before.json](logs/task_046/before.json)记录开始时1101个Go/模块文件和两个只读plan.md
的逐文件SHA256及Git状态；[audit.py](logs/task_046/audit.py)重新枚举含untracked文件
并比较内容/状态，保存[audit.json](logs/task_046/audit.json)。不运行Go/make/failpoint，
不修改两个plan.md或原主题。既有其他任务文件删除不纳入本次提交。

既有忽略为独立进程测试 `tso_processes`；未新增忽略测试。全部阶段槽位1的锁已释放，
缓存保留。日志仅去除行末空白和多余末尾空行，不修改命令或测试结果。差异检查通过，
1103个受保护文件内容和Go/模块状态完全未变，无新增Go文件。
