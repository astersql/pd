# 任务093：启动失败清理

基准 `98414ee41`；任务92证据及任务1–4清单/只读oracle入口已核验。

## 行为与来源

[start_cleanup.rs](../../../pkg/runtime/start_cleanup.rs)保留启动错误为source；
清理也失败时同时提供清理错误，避免注销collector或释放lease的错误覆盖初始失败。
[node.rs](../../../pkg/runtime/node.rs)在最终admission确认失败时先完成worker、lease、
Registry和listener清理，再由`ready()`返回worker原始错误。此前此路径只返回channel closed，
并可在清理结束前返回。最终admission错误优先于后续清理错误。
不改变任务94的Ready状态门或任务95的正常长流关闭设计。

复用已有Member、ClusterHost和ServiceMetrics的资源所有权、取消与join，不新增平行装配。
持久成员登记、TSO窗口及join意图不是临时资源：失败不删除有效身份或回滚已提交窗口；
重试沿既有join恢复路径完成。外部MetaStore生命周期仍由宿主拥有。

[source-inventory.md](source-inventory.md)任务93行、basic_server.go的3/43/93行、
node.rs共享1/3/5/7/83–87/89/93–95/98行及相关manifest来源行适用于本任务。
[wire-inventory.md](wire-inventory.md)与[surface-inventory.md](surface-inventory.md)
没有独立owner93协议/配置行；共享RPC/HTTP准入以真实health/GetMembers回归验证，
不把共享来源行视作所有相邻功能通过。
只读Go的basicserver.Server定义Run/Close边界，server.Server.Close取消并等待后台循环，
关闭客户端；cmd/pd-server/main.go的Run失败为进程fatal边界。
Go没有Rust宿主Registry或外部MetaStore join确认的同构入口，不伪造同构故障注入差分。

## 测试

[独立测试](../../../pkg/runtime/start_cleanup_test.rs)由lib显式注册，默认features。
`task_093_behavior`执行六个真实etcd/TCP场景：成员存储事务拒绝、角色TSO窗口事务拒绝、
真实Registry冲突、listener TLS配置拒绝、worker最终join确认事务拒绝、已取消启动。
事务失败由实际etcd拒绝超过2MiB的请求产生，其余事务仍使用同一MetaStore，
不是mock返回值或调用次数。断言失败事务未写入、leader key已消失、Registry无遗留、
端口可立即重绑；同存储/身份/Registry再次启动，经真实HTTP health及GetMembers RPC确认leader，
shutdown后再次检查端口与Registry。另一个纯错误链测试仅验证复合错误source，
不把它当作真实后端清理故障注入。包级既有取消/lease-loss/恢复测试覆盖在途取消边界。

首轮可编译失败：`cargo test --locked -p astersql-pd-runtime --lib task_093_ -- --nocapture`，
退出101，0通过/1失败；前四类失败和立即重启通过，在worker场景断言原始错误时得到
`channel closed`。真实后端拒绝的稳定分类为`unexpected metadata backend response`，
保持现有MetaStore分类，不把底层etcd消息当作公开错误契约。

## 验证记录

动态原子mkdir领取slot-1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
默认features，`CARGO_BUILD_JOBS=4`、`RUST_TEST_THREADS=1`。
最终命令、退出码和有效数量随最终验证补记；原始日志及守卫见压缩证据。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-runtime --lib task_093_ -- --nocapture` | 0 | 2通过、0忽略：1个含六场景的真实行为测试及1个错误链测试 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 73通过、36项既有忽略 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 包级编译 |
| `cargo check --workspace --locked` | 0 | workspace编译 |
| `cargo test --workspace --locked` | 0 | 2212通过、76项既有忽略 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 当前真实进程产物 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_CLI_EXIT_REPORT=/tmp/task093-oracle.json python3 scripts/cli-exit-oracle.py` | 0 | 18组CLI、1组启动失败、4组HTTP/信号退出差分 |

## 进程差分与只读守卫

复用任务4只读入口的固定Go二进制和现有cli-exit-oracle.py调用链，不生成或修改Go，
fixture仅以`-mod=readonly`编译已有源码，禁用代理和工具链下载，不执行failpoint入口。
真实TCP端口占用使Go/Rust均在启动时失败并退出1；原始输出保留错误分类。
四组正常启动均以真实HTTP members确认身份，分别发送SIGTERM/SIGINT/SIGHUP/SIGQUIT，
退出码及listener释放一致；服务与fixture进程组清理由现有supervisor断言。
Go二进制没有VCS revision，按SHA-256固定，不能声称来自当前HEAD。
Rust实际使用外部MetaStore，Go使用embedded-etcd；仅比较共同进程语义，
不声称Rust Registry、TSO事务故障或join确认阶段已在Go上同构注入。
宿主阶段的故障及立即重试证据来自上面的真实Rust/etcd测试。

最终Go/模块保护集合1101个文件（含ignored/untracked），路径、内容SHA-256及Go Git状态
与开始一致；target中没有额外Go/模块文件。plan.md与HEAD原字节一致。
所有Cargo、脚本及产物子进程结束后，EXIT trap释放自己的slot-1锁，保留缓存。
原始失败/通过日志、确切命令、源码与二进制哈希、5行来源归档、进程观察和前后守卫见
[压缩证据](task-093-evidence.json.gz)。文档链接及`git diff --check`通过。
用户原有prompt.md修改不纳入提交。

状态：已完成。
