# 任务078：领导者授予配置恢复

状态：已完成。

批次77的真实依赖已满足：提交 `d90d1fbd9`，以及 [task-077](task-077.md)
的 canonical GrantLeaderScheduler、真实 core/operator 行为及全仓证据。
已读两级只读计划、ADR、porting-guide、仓库规范和 oracle-index.json。
按用户对整个计划的后续授权，以 **Go只读源码/测试推导 + Rust真实行为与故障验证**
替代缺失的Go运行oracle；不声称本任务取得Go运行差分。

生产路径：[grant_leader_config.rs](../../../pkg/schedule/schedulers/grant_leader_config.rs)，
[边界说明](../../../pkg/schedule/schedulers/grant_leader_config.rs.md)。扩展任务77唯一
canonical GrantLeaderConfig/Scheduler，没有复制实现、引入依赖或feature。
API：`GrantLeaderConfig::from_args/remove_store/reset_store` 和
`GrantLeaderScheduler::encode_grant_config/update_grant_config/delete_grant_store/`
`reload_grant_config`，复用既有 Prepare/Clean、真实 BasicCluster OUT暂停计数及
StorageEndpoint/MetaStore。独立测试在lib.rs显式注册。

## 来源与行为

基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`，
[provenance](logs/task_078/provenance.json)记录源码与fixture哈希，Go输出及哈希为null。
[fixture](fixtures/task_078.json)冻结源码推导输入/期望。TestGrantLeaderScheduler
仅覆盖grant调度和Prepare暂停，未覆盖本任务配置增删、持久化错误或reload；这些
场景从grant_leader.go的buildWithArgs/removeStore/resetStore、updateConfig/
deleteConfig/ReloadConfig及utils.go/getKeyRanges和pauseAndResumeLeaderTransfer推导。
keyrange.go确认持久化边界为十六进制JSON。没有裸跑依赖failpoints的Go测试。

HTTP方法留在适配层。领域更新接收明确id及类型化range参数；Go JSON解码产生
`[]any`，不会满足handler的`[]string`断言，这个传输转换差异在领域说明中明确保留。
本任务不增加HTTP服务。JSON对象顺序无语义；null map/list归一为空领域集合，
空range列表序列化为[]。不归一化错误或暂停/持久化差异，没有时钟或随机业务输入。

## 测试与故障

`task_078_behavior_cases`：11/11冻结args案例，包括缺id、默认全域、前导零、
u64上界与溢出、正负号、逗号多id拒绝、非法转义、奇数尾项、多个ranges、
query空格/无效UTF8字节，以及持久化codec roundtrip。

`task_078_updates_and_faults`：真实MemoryStore+StorageEndpoint增改/重载，缺store、
新store解析失败回滚、持久化I/O失败时新/已有store移除及OUT恢复、保留IN暂停、
旧持久化快照保留、读失败、重新加载恢复，以及等待事务时取消释放新暂停。
故障只在MetaStore read/transact I/O边界注入，不mock调度或配置结果。

`task_078_delete_and_reload`：删除写失败保留内存和暂停、缺项、普通回调失败恢复
内存/OUT但存储保持空、NotFound不恢复、非末项删除和末项成功回调、畸形JSON/
map key/字段类型读失败、membership暂停差分、重复reload不增引用、缺store诊断
仍发布新配置、缺少持久化key保留旧状态，以及Clean恢复OUT。

`task_078_reset_remove_and_schema`：remove的succ/last，reset缺store仍恢复map，
缺失/null/未知字段schema行为，以及字节key的确切十六进制JSON。
`task_078_existing_update_parse_failure`：已有store解析失败保留map和旧存储但恢复OUT，
显式空ranges重置全域，Clean最终恢复暂停。
`task_078_retained_ranges_reparse`：5/5固定案例验证Go getRanges/buildWithArgs再次
解码保留范围的+/%、非法转义、NUL/无效UTF8及空列表默认全域。

[red-api](logs/task_078/red-api.log.gz)记录API缺失的编译失败，退出101，不当作行为失败。
[red-behavior](logs/task_078/red-behavior.log.gz)临时去掉已有store的持久化失败移除，
同一测试在`!stores.contains_key(id)`断言失败，3通过/1失败，退出101；正确实现已恢复。
初次实现的reload测试用了非十六进制输入；从Go keyrange.go复核后修正测试数据，
[focused-initial](logs/task_078/focused-initial.log.gz)保存该设计修正前失败，不算生产修复。

## 验证与审计

每个Cargo阶段由[run.py](logs/task_078/run.py)原子mkdir动态领取仓库共享槽位，
记录任务、会话与PID。所有命令、退出码和实际CARGO_TARGET_DIR保存在同名JSON。
实际使用 **槽位1**，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
正常/失败/中断等待子进程退出后释放锁，保留缓存。Rust修改后先运行cargo fmt --all。


| 最终命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_078 -- --nocapture` | 0 | 6通过；11/11 args、5/5 retained-range固定案例及独立故障验证 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 46通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，仅既有cpu_usage弃用警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1595通过，0失败，1既有忽略 |

[counts](logs/task_078/counts.json)由实际test result行汇总，包含2项文档测试。
唯一既有忽略为tso_processes独立进程测试，未新增忽略。
早期全仓运行保留为before-reparse日志；最终命令覆盖补充的retained-range实现。
[日志编码](logs/task_078/log-encoding.json)记录压缩前SHA256及gzip编码。

[审计](logs/task_078/audit.json)确认开始/结束1101个Go/模块文件（含untracked清单）
内容哈希和Git状态一致，两份只读plan.md哈希不变，git diff --check退出0。
Cargo.toml/Cargo.lock未改变。所有生产/测试及相对文档链接已复核；仅任务78相关
文件提交，其它编号任务既有删除/修改保留。槽位1锁已释放，编译缓存保留。
