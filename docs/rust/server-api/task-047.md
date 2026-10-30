# 任务047：完整配置查询 API

状态：已完成。

基准：`9135e8ed8`；依赖[任务046](task-046.md)已核验。Go及模块文件、plan.md保持只读。

## 实现与映射

[HTTP适配](../../../pkg/server/http_config_read.rs)接通 GET `/pd/api/v1/config`、`/config/default`、`/config/schedule`、`/config/pd-server`、`/config/replicate`、`/config/label-property`、`/config/cluster-version`、`/config/replication-mode`（后七项均在`/pd/api/v1`下）。复用现有监听器、JSON响应、request cancellation和子路由leader转发。

[宿主快照](../../../pkg/runtime/member_role_worker.rs)复用`astersql-pd-server-config::Config::adjust`、`PersistedConfig::reload`及[当前任期配置](../../../pkg/runtime/cluster_host.rs)。本地name/client/peer来自NodeConfig，动态部分来自与worker相同的配置代际；follower无leader任期时从同一MetaStore恢复动态部分，并通过真实HTTP取得leader的schedule/replication，保留本地身份。所有读取只复制配置，不发布修改，不执行持久化。leader连接future与请求一起取消，不创建脱离宿主的任务。

默认配置使用宿主当前目录和真实nodename；为此在workspace集中启用已有锁定`rustix 1.1.5`的system功能，未引入新版本。默认值、字符串布尔、duration、null/空map、弃用字段omitempty由现有领域序列化提供。当前配置及schedule接口将零max-merge-region-keys投影为size×10000（Go u64 wrapping语义）；default接口保留零值。

来源：

- [source-inventory](source-inventory.md)任务47行、config.go的八个Get函数及server/config指纹/类型清单；额外对照server/server.go GetConfig的动态字段合并、config.go getLeaderConfig及schedule/config.go GetMaxMergeRegionKeys。
- [wire-inventory](wire-inventory.md) HTTP-021、023、024、026、027、029、031、033八行已更新。
- [surface-inventory](surface-inventory.md) router.go 171、173、174、176、177、179、181、183注册行。审计/限流接线归81/82，不把这些后续能力记为本任务通过。

边界：本任务读取现有Rust配置，不新增配置写API或独立MCS。当前无持久配置时Rust运行时既有placement-rules=false仍如实输出；`/config/default`与Go默认true一致。data-dir等嵌入etcd字段沿用配置兼容投影，当前Rust仍使用外部MetaStore，不声称已实现embedded-etcd或原本忽略的CLI参数。恢复配置差分使用相同动态输入，未将Go的构建版本、随机身份和存储目录伪装为Rust运行事实。

## 失败、成功与差分

首次`cargo test --locked -p astersql-pd-server --lib task_047_ -- --nocapture`编译运行成功，1个测试失败、退出101：HTTP配置路由缺失，现有fallback返回`application/grpc`，真实TCP断言期望`application/json; charset=UTF-8`。不是以编译失败代替行为失败。

[普通行为测试](../../../pkg/server/http_config_read_test.rs)检查八个端点、未bootstrap读取、忽略无关query、默认/运行配置差异、有效merge keys、弃用字段省略。独立差分测试启动现有Go二进制及真实Rust节点，完整比较default JSON全部字段及类型；比较全量配置字段集合和八个动态section、各子接口与完整配置的一致性。Go经真实tonic Bootstrap后更新max-replicas=5、merge-size=37、merge-keys=0、flow-round-by-digit=4；Rust用同一持久配置恢复并重启，HTTP输出等价，merge-keys为370000。读取前后的MetaStore config条目（含revision）不变。

[故障/取消测试](../../../pkg/server/http_config_read_fault_test.rs)启动两个真实Rust节点（IPv6 leader、IPv4 follower）：follower的完整配置保留本地name/client-url，动态字段与leader一致，子路由转发一致；显式merge-keys=12345不被覆盖；持久弃用disable-remove-down-replica迁移为enable-remove-down-replica=false；label-property、cluster-version、PD配置恢复正确。只在真实etcd config读取边界注入future revision，确认500及实际etcd错误；恢复后继续读取。挂起下一次真实读取后取消follower宿主，收到500 ErrServerNotStarted，5秒内shutdown/join；leader配置条目不变。没有mock调用次数断言。

差分调试记录均保留：Go未bootstrap时配置写返回400，测试改为先通过tonic bootstrap；第一次不变性快照误包含异步scheduler启动规范化写入，最终通过watch_schedule明确等待Running再截取读取前快照。依赖边新增后用`cargo check --offline -p astersql-pd-server`更新锁，之后恢复locked验证。另有一次SchedulePhase公开路径写错造成编译失败，修正为cluster_host导出路径；此错误不计行为失败证据。最终成功没有放宽配置值或副作用断言。

只读差分入口：[scripts/http-config-oracle.py](../../../scripts/http-config-oracle.py)。复用任务004的Go快照与进程组清理；限定已领取槽位、关闭Go下载和模块写入、核验既有Go二进制指纹/build info，要求明确1通过/0失败/0忽略。Go二进制无VCS revision，不声称来自当前HEAD。

## 验证

实际动态领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。各阶段按1–10原子mkdir领取，锁中记录任务、会话与持有进程PID；所有Cargo与子进程继承绝对路径。阶段结束释放自有锁，保留缓存。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace元数据 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | server静态检查 |
| `cargo check --workspace --locked` | 0 | workspace静态检查 |
| `cargo test --locked -p astersql-pd-server --lib task_047_ -- --nocapture` | 0 | 2通过/0失败/1默认忽略 |
| `RUST_TEST_THREADS=4 cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 169通过/0失败/8默认忽略，含integration target |

独立差分：`PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-config-oracle.py`；内部执行`cargo test --locked -p astersql-pd-server --lib task_047_differential -- --ignored --nocapture`，1通过/0失败/0忽略。

首轮`RUST_TEST_THREADS=4 cargo test --workspace --locked --no-fail-fast`退出101：2057通过/1失败/45默认忽略。唯一失败为未修改的`cluster_host_test::task_005_registration_failure_releases_leadership`在同地址重新bind时出现`AddrInUse`（cluster_host_test.rs:423），未进入配置API逻辑。单独`cargo test --locked -p astersql-pd-runtime --lib task_005_registration_failure_releases_leadership -- --nocapture`退出0、1通过，未修改该测试。随后对最终IPv6地址处理与直接leader发现的代码重新运行聚焦、差分、包级及workspace检查。

最终`RUST_TEST_THREADS=4 cargo test --workspace --locked --no-fail-fast`退出0：2058通过/0失败/45默认忽略（含compile-fail doctest）。最终server包重跑169通过/0失败/8默认忽略；聚焦2通过；独立差分1通过；fmt及workspace check均退出0。最终差分原始报告为`target/rust-slot-1/http-config-oracle-1793328313814229000.json`，确认`process_group_clean=true`与`readonly_unchanged=true`。

最终1101个Go/模块文件的路径、内容SHA-256及受保护Git状态与开始完全一致，包含ignored/untracked；plan.md哈希不变。最终验证期间1229个Rust/Cargo文件哈希不变。文档链接、Python AST语法与`git diff --check`通过。自有进程和槽位锁已清理，缓存保留。

[压缩原始证据](task-047-evidence.json.gz)包含开始/结束守卫、全部失败/通过日志、确切命令及测试数量、槽位领取runner、Go二进制指纹/build info和oracle报告。SHA-256：`03f36dc6e6430ef46967a5666fdfcd46e132e7dd55b1641ddf55711eb7c56eaa`。
