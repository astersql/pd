# 任务048：Schedule 配置热更新

状态：已完成。

基准 `75cca260c`；依赖[任务047](task-047.md)已确认。Go、模块文件与计划保持只读。

## 实现与来源

[HTTP入口](../../../pkg/server/http_schedule_config_write.rs)接通 POST `/pd/api/v1/config/schedule`，复用同一监听器、leader 转发、请求取消和领域配置类型。[宿主接线](../../../pkg/runtime/member_role_worker.rs)要求有效 leader 任期；[协调器命令](../../../pkg/runtime/schedule_scatter.rs)在既有命令队列及资源锁下读取最新配置、合并 patch、执行 canonical `ScheduleConfig::validate` 及弃用字段检查，通过 `TermStore` fencing 和 `PersistedConfig::persist` 原子写入同一 MetaStore。持久化成功后不经过 await，发布给 cluster state、协调器和 scheduler configuration；持久化失败或提交前取消不发布。

[协调器](../../../pkg/schedule/coordinator.rs)原先持有启动快照，因此扩展本任务范围修复必要上游能力：更新 provider、成功回调读取的配置以及[等待队列上限](../../../pkg/schedule/operator/controller_admission.rs)，保留 controllers、记录和 pending work。未复用 `HotConfig::update_schedule` 的先发布后回滚语义；复用其 canonical 持久化文档及存储入口，满足本任务先持久化后发布的顺序。

Go oracle：`server/api/config.go` 的 `SetScheduleConfig`，`server/server.go` 的 `PatchScheduleConfig/updateScheduleConfig`，`pkg/schedule/config/config.go` 的 `UnmarshalJSON/Validate/Deprecated`。新 store-limit 从同一候选配置的 default-store-limit 继承，已有 entry 保留省略字段；null 标量和 store-limit 保留，数组按替换语义处理。配置写不等于 scheduler 实例增删；后者属于后续 scheduler 管理任务。

清单： [source-inventory](source-inventory.md)任务48行及 `server/config/persist_schedule.rs`；[wire-inventory](wire-inventory.md) HTTP-025；[surface-inventory](surface-inventory.md) `router.go:175` 注册行、ScheduleConfig 字段/验证与 persist_schedule 文件指纹。审计/限流仍归81/82，不在本任务宣称覆盖。

## 行为证据

[HTTP测试](../../../pkg/server/http_schedule_config_write_test.rs)：真实 TCP + etcd，成功写入及 GET 回读；混合合法/非法字段整体拒绝；实际持久文档及使用相同身份重启恢复。

[故障测试](../../../pkg/server/http_schedule_config_write_fault_test.rs)：两个真实节点验证 follower POST 转发；在真实 config 事务中附加不存在 lease，让 etcd 拒绝写入，检查持久条目（含 revision）和活动配置不变；并发修改不同字段无丢失；挂起 config 提交，取消宿主后收到500，shutdown/join在5秒内完成，持久状态保持。取消由已有 Redirector 或 handler 响应，分别可能是 plain text 或 JSON，均检查同一个 ErrServerNotStarted。断连等后端提交结果不确定的情况不做盲目重试，也不宣称能够确定回滚；这里的故障证据是etcd明确拒绝与提交前取消。

[协调器测试](../../../pkg/schedule/config_update_test.rs)：修改 witness interval 后新记录使用新 TTL；等待上限从0升为1后实际入队成功，降回0后保留已有队列并拒绝后续入队；旧 controller 身份和 scheduler 初始化状态保持。

[只读差分入口](../../../scripts/http-schedule-config-oracle.py)复用任务004守卫，使用已有 Go 二进制与真实 Rust 节点。逐次比较响应状态/完整 JSON 错误及完整 schedule 读回：有效值、域验证失败、弃用字段、未知字段、null/空patch、store-limit 继承与保留、字符串布尔、负数、错误类型、无效 duration、默认及逐store limit的非法数组。Rust 原有运行默认 region-score-formula-version 为空而 Go 为v2；差分先向双方真实 POST 同一 v2 值建立相同输入，不删除该字段或归一化输出。Go二进制 build info 没有VCS revision，不声称它由当前HEAD构建。

首次失败验证 `cargo test --locked -p astersql-pd-server --lib task_048_ -- --nocapture` 编译成功并运行1个测试，退出101：POST缺路由，实际 Content-Type 为空，断言要求JSON。后续编译错误（服务方法名和Snapshot比较）不计TDD失败证据。差分暴露并修正400类型错误的结构化响应、字符串布尔和duration错误；重启测试沿用同一已登记身份，不能换名字规避身份约束。保留全部迭代日志。新增形状差分还证实Serde可按位置接受结构体数组，而Go返回400；修复后两种store-limit数组均与Go错误完全一致，全部schedule值保持不变。转义字段名差分进一步确认字符串搜索定位会漏掉非法输入；改为逐JSON token读取字段名/值边界，定位失败也必须返回错误，不能放行patch。

## 验证记录

动态领取槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。每阶段原子mkdir领取并记录任务/计划/会话/PID，所有Cargo及oracle继承同一路径，释放自有锁并保留缓存。

以下命令使用默认features、`RUST_TEST_THREADS=4`；测试数量包含实际执行项，不把编译成功或ignored当作行为通过。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace元数据 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | server检查 |
| `cargo check --workspace --locked` | 0 | workspace检查 |
| `cargo test --locked -p astersql-pd-server --lib task_048_ -- --nocapture` | 0 | 2通过，1默认忽略的差分另行执行 |
| `cargo test --locked -p astersql-pd-schedule --lib task_048_ -- --nocapture` | 0 | 1通过 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 171通过/0失败/9默认忽略，含integration target |

`PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-schedule-config-oracle.py`退出0，内部命令 `cargo test --locked -p astersql-pd-server --lib task_048_differential -- --ignored --nocapture`：1通过/0失败/0忽略。最后一次差分原始报告为 `target/rust-slot-1/http-schedule-config-oracle-1793330908843203000.json`，`process_group_clean=true`、`readonly_unchanged=true`。

第一轮 `cargo test --workspace --locked --no-fail-fast`退出0：2061通过/0失败/46默认忽略；第二轮该命令退出101：2060通过/1失败/46默认忽略。唯一失败是未改动的 `grpc_alloc_id_test::task_021_behavior`（grpc_alloc_id_test.rs:140），期望 `(1012,1005)`，实际 `(1140,1005)`；本任务的HTTP/协调器测试均通过。`cargo test --locked -p astersql-pd-server --lib task_021_behavior -- --nocapture` 随后退出0、1通过，未改该测试或ID分配器。最后对错误字段定位的小修复重新执行fmt、server/workspace check、聚焦、Go差分及server全包回归；不把第二轮workspace失败日志写成通过。


最终server全包复跑退出0：171通过/0失败/9默认忽略，包含前述ID用例；最终聚焦2通过、独立差分1通过、协调器1通过。最终fmt及server/workspace check均退出0。23个差分输入逐次核对完整配置与响应，没有删减失败案例。

最终1101个Go/模块文件（含ignored/untracked）的存在性、SHA-256与受保护Git状态与开始完全一致；plan.md与基准字节一致。最终server回归期间1233个Rust/Cargo文件SHA-256不变。自有锁已释放，缓存保留；oracle报告确认进程组清理。文档链接、Python语法和git diff --check通过。

[压缩原始证据](task-048-evidence.json.gz)包含开始/结束Go守卫、最终源码哈希、全部迭代输出、关键命令/退出码/测试数、槽位runner及全部Go oracle报告。SHA-256：`39cc15191229937a5ef6a09e311e5637becb2c9544ec48f76e7c93c2ffd94bcf`。
