# 任务 1：统计维度与负载换算

实施基准：`69ba16e6d47dfe59a21498c21641e1c1eaa726e0`。目标 Go 文件与该基准无差异；
与旧 oracle 二进制的来源基准 `db1b56605b43b973afbd1eef8d03a7e7ba25e0bf` 同样无差异。
已读根规则、Copilot、ADR-001、依赖基线、移植指南、两个只读计划和任务文件。
目标目录无更深规则。主题4前置验收见 [批次41](../schedule/integration/41-证据.md)：
规则链与八个基础包、真实内存/etcd 后端及完整 workspace 已验收。
本任务是无依赖的纯值工具，不复建 core、movingaverage 或存储。

## Go oracle（实现前记录）

复用已有 Go PD debug 二进制 `/tmp/pd-types-oracle/pd-classic-linux`，来源与构建审查见
[主题4任务1](../schedule/types/1-证据.md)。SHA256：
`4537466d65d253b1e291cacecf53faee14117386e8f2c5ff7b380108e49cc268`。
未构建/生成/改写任何 Go；只读挂载该目录到私有容器 `pd-state01-dlv`，
Alpine edge 的 Delve 1.27.1 在 main.main 的下一行安全点暂停，服务未启动。
调试调用只操作该被调试进程的内存；不构成生产 FFI 或 Go 运行时依赖。

命令（退出0）：
`docker exec pd-state01-dlv dlv exec /oracle/pd-classic-linux --allow-non-terminal-interactive --init=/oracle/dlv-state01-final -- --version`。
[调试命令](task01/oracle.commands)、[原始输出](task01/oracle.out)、
[解析器](task01/parse_oracle.py)、[真实结果 fixture](../../../tests/rust-compat/fixtures/state/task-01.json)。
解析器只读取实际返回值，不计算函数预期。无时间/随机归一化；非有限浮点以字符串保留。原始输出仅清除行尾空白及文件尾多余空行。

事件序列：初始阈值/索引；名称与无效名称；Write/Read/无效RW(2)的反向、周期、反抖与速率；
单标签；write后read写入完整负载；0/0、负/0、正/0；全部 Region/Store/Action 名称与边界。
初始完整负载为阈值 `[8192,128,128,1024,32,32,1,1]`，间隔2秒时
read=`[4096,64,64,0.5]`、write=`[512,16,16,0.5]`。CPU 索引在6/7，不能按连续4维切分。
零间隔按Go浮点除法保留NaN/±Inf；无效RW无索引，速率全零，inverse=Write、周期10、anti=12。
set操作使用原数组的4:8作为源，每一步读取在相关覆盖之前完成，等价独立源快照；
Rust安全借用接口接受互不别名的切片。Go调用者可能重叠源/目标的特殊别名顺序不作为新接口能力。

源码补充契约明确区别于运行差分：SourceKind.String 被旧二进制链接器移除，
仅依据 kind.go 的 direct/inherit/unknown 分支验收；ForeachRegionStats 的read优先顺序、
短切片索引panic、默认滤波窗口与单位依据固定源码。必要速率/无效名称/阈值已取得真实运行证据。
最初Delve对非地址整数临时值调用方法报nil pointer，改用现有可寻址整数转型后成功；
此调试器限制不冒称Go函数故障。最终oracle输出无调试错误。

源码与调用方审查：kind/constant/labels 无I/O、failpoint、后台任务、持久key；
调用方 hot_peer_cache.go:272 读取速率，hot_region_solver.go:432/436 写入完整负载。
Go常量字节阈值使用KiB=1024，速率按秒，CPU保持Go原计量单位，不添加百分比转换。
可变Go阈值在Rust提供不可变默认数组；调用方需修改时持有自己的副本，无全局Registry。

## 验证记录

新增 canonical crate `astersql-pd-statistics-utils`，唯一生产模块 `kind.rs`、
crate入口 `lib.rs` 及相邻 `.rs.md`；7项独立行为测试显式登记于 `kind_test.rs`。
生产没有外部依赖；仅测试复用已有serde_json。根manifest注册member/dependency；
Cargo.lock只新增该本地包，不改变任何既有版本。首次generate-lockfile离线生成时
附带更新pulldown-cmark，已恢复原锁定条目，最终差异不包含该更新。

失败验证：`cargo test -p astersql-pd-statistics-utils --locked --lib state_task_01_kind -- --nocapture`，
[初次输出](task01/red.out)退出101，3 passed/4 failed；其中两个为fixture JSON整数与Rust浮点
表示不同，修正harness按f64比较后，[确认红灯](task01/red-confirmed.out)仍退出101，
4 passed/3 failed/0 ignored：read实际0期望4096、write实际0期望512，短输入未发生预期越界。
此时最小接口的速率方法只返回零。补齐实际索引选择与除法后通过；未破坏已有正确实现制造失败。

聚焦与包级均7 passed/0 failed/0 ignored。用例覆盖：完整整数名称映射、
名称/阈值/默认值、RW顺序/反向/周期、逐维速率与零间隔、完整负载写入隔离、
短切片访问边界、独立单标签。短切片panic被catch_unwind验证，日志panic文本不表示失败。

全部Cargo调用通过[共享槽位包装器](task01/slot.sh)，每阶段按原子mkdir动态从1到10领取，
记录计划/任务/会话/PID；finally等待命令进程组退出后释放自己的锁，保留缓存。
本次所有阶段实际领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[最终脚本](task01/validate.sh)、[完整输出](task01/validation.out)、
[metadata输出](task01/metadata.out)记录确切命令及退出码。
脚本继承GOFLAGS=-mod=readonly、GOPROXY=off、CGO_ENABLED=0、GOARCH=arm64。

workspace的Go调用链已复核：proto/build.rs仅go mod download固定已缓存版本，
生成只在槽位OUT_DIR；etcd_fixture_test.rs仅go build既有scripts/rust-etcd-fixture，
该夹具无generate/failpoint，WAL及二进制在槽位，Drop通过EOF关闭并wait，必要时kill/wait。
没有执行make、failpoint启停、Go格式化或模块整理。

最终回归结果与资源释放记录在下文。

| 确切命令 | 退出码与有效结果 |
| --- | --- |
| `cargo fmt --all` | 0；无其它批次源码差异 |
| `cargo metadata --format-version 1 --no-deps` | 0；canonical crate已登记 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics-utils --locked --lib state_task_01_kind -- --nocapture` | 0；7 passed/0 failed/0 ignored |
| `cargo test -p astersql-pd-statistics-utils --locked` | 0；7 passed/0 failed/0 ignored |
| `cargo check --workspace --locked` | 0；仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0；974 passed/0 failed/1既有ignored |
| `python3 docs/rust/state/task01/readonly.py` | 0；1101个Go/模块文件集合、SHA256及Git状态不变；链接与diff检查通过 |

workspace原有ignored为`tso_processes`，未计入本任务行为证据；原有
`health_member_processes`缺少进程环境时直接返回，同样不作为本任务证据。
完整回归包含既有真实MemoryStore/EtcdMetaStore测试，不以它们替代纯值工具测试。
本任务没有新增异步任务/后端；独立oracle容器已移除。Cargo子进程全部退出，
`target/rust-slot-locks/slot-1.lock`已释放，`target/rust-slot-1`缓存保留。
资源检查见 [resources.out](task01/resources.out)。

[只读校验脚本](task01/readonly.py)、[最终校验输出](task01/readonly.out)保存前后证据。
所有Go/模块文件均未变，两个指定plan.md及来源史诗保持只读，用户原有计划变更未纳入提交。

状态：已完成。本证据只验收统计维度与负载换算，不代表整个统计/同步主题完成。
