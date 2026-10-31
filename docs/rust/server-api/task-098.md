# 任务098：多成员滚动重启

基准 `8ec75f63a`；依赖[任务097](task-097.md)及此前传递依赖已满足。
新增[滚动重启辅助](../../../tests/rust-compat/rolling_restart.rs)和
[独立行为测试](../../../tests/rust-compat/rolling_restart_test.rs)，在
[lib.rs](../../../tests/rust-compat/lib.rs)显式注册。
复用真实进程 harness，增加仅允许已停止进程恢复的 `Server::resume_with_args`，
原样保留调用方身份、网络地址及完整拓扑参数；无生产领域实现变更。

## 来源与边界

[source-inventory.md](source-inventory.md)任务98行136、node.rs公开入口行157、
持久化/worker行339–342、pkg/member来源行1252–1264及node.rs行1338均由本次
进程/持久身份/leader切换证据覆盖。member.rs的bootstrap持久身份和campaign租约、
node.rs的bootstrap_config及member_role_worker沿用既有实现。
[wire-inventory.md](wire-inventory.md)无98专属协议，使用已有GetMembers、Bootstrap、
AllocID、Tso、StoreHeartbeat、GetRegion及HTTP members。
[surface-inventory.md](surface-inventory.md)无98专属配置或指标；已有CLI启动参数
和use-region-storage配置用于构造真实拓扑，不新增服务开关。
Go只读参考：pkg/member/member.go、election.go、server/server.go::bootstrapCluster、
pkg/storage/storage.go的coreStorage。Go现有二进制构建信息及hash由supervisor记录；
没有VCS revision时不声称该二进制由当前HEAD构建。

Rust三独立OS进程连接同一真实外部etcd；Go三进程运行自身embedded-etcd集群。
Go设置`[pd-server] use-region-storage = false`，与已有task005恢复差分一致，
让region持久状态走共享元数据，避免将默认本地region缓存的异步同步当作共享存储恢复。
这不是Go/Rust embedded-etcd拓扑等价证明，主题10边界不变。

## 行为与失败证据

`task_098_behavior`不忽略、默认执行Rust场景；dual环境再执行真实Go同场景。
先按下表构建当前槽位的真实pd-server，兼容测试不递归调用Cargo或使用过期的其他槽位产物。
先启动三成员、Bootstrap真实store/region，保存cluster ID和三成员完整身份
（name、member ID、client/peer URLs）。从当前leader开始逐个SIGTERM、wait回收，
另两成员继续服务；每轮在成员离线时采样3次、同身份/端口重启后采样1次。
三条原有tonic channel始终保留，重启节点必须通过原channel重新响应。
两实现分别验证12个采样ID无重复且严格增加、TSO严格增加、StoreHeartbeat成功、
GetRegion完整元数据恢复；每次重启RPC身份集合不变，HTTP members返回三成员。
额外AllocID就绪探针会消耗ID，不计入12个采样数，允许ID不连续。

错误案例：每轮真实错误cluster ID的AllocID必须拒绝。取消案例：每轮真实TSO流
先得到响应，保留发送端使流保持打开，再丢弃流/发送端取消，随后同channel查询仍可用。
TSO取消首响应也纳入全局顺序检查。查询/选举/重连均有期限；不以任意网络错误代替成功。
三成员最终SIGTERM退出成功，Drop回收子进程、工作目录，fixture随后结束。
使用真实系统时钟检查跨进程TSO顺序，不靠sleep推进时间或构造固定时间戳；
100ms间隔仅用于有界网络/选举轮询。

初轮可编译运行，0通过/1失败、退出101：看到leader不等于分配器初始化结束，
AllocID收到Unavailable/not leader。增加真实AllocID就绪重试后，初版两实现均通过。
加严重启后采样产生两次region查询失败（None及等待超时），暴露Go默认本地region
storage不满足本次共享持久恢复前提，故按已有只读恢复oracle关闭该选项。
随后0通过/1失败、退出101：Go在空TSO流尚无首请求时等待响应头直到RPC超时；
改为先读真实TSO响应再取消。以上为测试假设/装配修正，不冒充生产缺陷TDD红灯。
已有生产行为已覆盖，无人为制造失败。
只读入口初次集成因重复`--nocapture`退出101、未运行测试；修正入口参数后重跑。
完整失败与成功原始输出保存在压缩证据中。

包级首次继承dual环境时，任务98本身通过，但既有health_member_processes期待
`storage-released`旧日志（实际是`PD stopped`）而失败，退出101；这不是滚动重启失败。
包级与workspace采用标准环境（清除PD_COMPAT_MODE/GO_BIN/RUST_BIN），任务98默认真实
Rust场景仍运行；dual差分通过独立聚焦和supervisor验证，不用环境清除冒充差分通过。
独立包级cargo check还发现storage-domain的hot_history.rs使用serde derive而未直接
启用derive feature，退出101；dev-dependency的feature合并曾掩盖该缺失。
按扩大范围修复依赖的授权，在pkg/storage/domain/Cargo.toml显式声明derive，
未添加新版本或修改Cargo.lock，随后重跑检查和行为回归。

## 验证记录

所有命令在仓库根，默认features，`CARGO_BUILD_JOBS=4`、`RUST_TEST_THREADS=4`。
每阶段依次原子mkdir动态领取slot-1，记录任务/持有PID；实际
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
`PD_COMPAT_MODE=dual`、`PD_COMPAT_GO_BIN=$PWD/bin/pd-server`、
`PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server`。
包级/workspace计数按libtest摘要汇总；既有辅助入口不计为任务98行为证据。
确切命令、退出码和有效计数见下表及[压缩证据](task-098-evidence.json.gz)。

| 确切命令 | 退出码 | 结果（通过/失败/忽略） |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 静态/构建，不计行为测试 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 静态/构建，不计行为测试 |
| `cargo test --locked -p astersql-pd-compat-tests --lib task_098_ -- --nocapture` | 0 | 1/0/0 |
| `bash scripts/run-pd-compat-readonly.sh --case rolling-restart` | 0 | 1/0/0 |
| `env -u PD_COMPAT_MODE -u PD_COMPAT_GO_BIN -u PD_COMPAT_RUST_BIN cargo test --locked -p astersql-pd-compat-tests -- --nocapture` | 0 | 37/0/1 |
| `cargo metadata --format-version 1 --no-deps > /tmp/task098-metadata.json` | 0 | 静态/构建，不计行为测试 |
| `cargo fmt --all -- --check` | 0 | 静态/构建，不计行为测试 |
| `cargo check --locked -p astersql-pd-compat-tests` | 0 | 静态/构建，不计行为测试 |
| `cargo check --workspace --locked` | 0 | 静态/构建，不计行为测试 |
| `env -u PD_COMPAT_MODE -u PD_COMPAT_GO_BIN -u PD_COMPAT_RUST_BIN cargo test --workspace --locked` | 0 | 2224/0/79 |

Go只读守卫：含ignored/untracked共1101个Go/模块文件，开始/结束路径集合和每文件
SHA-256一致，Git状态均无Go/模块变更；plan.md原字节与开始HEAD一致。
未运行make、failpoint启停、Go测试、模块整理或任何Go源码生成；fixture仅复用
既有只读Go源码，使用`go build -mod=readonly`输出到领取槽位，完整调用链沿用task004。
最后supervisor确认inputs_unchanged、readonly_unchanged、进程组清理和工作目录清理均成功。
slot-1锁已释放，保留编译缓存；自有rolling98数据目录与服务/fixture进程已清理。
Python AST、文档本地链接和git diff --check另行核验。
保留原有任务94–97删除及prompt.md修改，不纳入本任务提交。

状态：已完成。
