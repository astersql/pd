# 任务052：Store状态管理HTTP

基准 `4baef3a9a000d59c09bbe03a57d96ff31096fdaf`。依赖[任务051](task-051.md)已完成；Go/模块文件及 plan.md 全程只读。

## 范围与来源

[适配器](../../../pkg/server/http_store_state.rs)接通 DELETE `/pd/api/v1/store/{id}`、POST `/pd/api/v1/store/{id}/state` 和 DELETE `/pd/api/v1/stores/remove-tombstone`。复用同一 listener、leader 转发、请求取消、活动 Coordinator、canonical cluster 和 MetaStore；没有新增服务或领域副本。

- [source-inventory](source-inventory.md)任务52行：`server/api/store.go` 的 DeleteStore、SetStoreState、responseStoreErr、RemoveTombStone。
- [wire-inventory](wire-inventory.md)：HTTP-063、064、070。
- [surface-inventory](surface-inventory.md)：router.go:226/227/235 注册行；审计仍归81，本任务不宣称审计等价。
- 状态机来源：`server/cluster/cluster.go` 的 RemoveStore、checkTikvReplicaBeforeOfflineStore、UpStore、RemoveTombStoneRecords、RemoveStoreLimit；`server/cluster/scheduling_controller.go::getEvictLeaderStores`；`pkg/core/store.go` 状态选项。

修正任务51对 HTTP-073 `/stores/progress` 的归属：编号任务54明确包含“limit与progress接口”，因此归回54并继续标缺证据。本任务没有实现或声称完成 progress，也没有修改任务54、plan.md 或来源史诗。

## 行为及持久化

[领域校验](../../../server/cluster/store_lifecycle.rs)保持幂等判断、Removed/Destroyed 拒绝顺序、TiKV 副本数和所有剩余目标被 evict-leader 排除的检查。TiFlash 不参加副本数检查。`force` 按参数存在性解释，`force=false` 同样强制下线；state 仅接受大小写无关 Up/Offline。非法 ID、缺失 Store、已移除 Store、非法 state 分别保留输入错误 JSON、404、410 和400。

[宿主实现](../../../pkg/runtime/store_lifecycle.rs)通过活动调度命令队列和原有资源锁串行化状态写入、注册、心跳、配置及后台状态检查。使用 leader-fenced MetaStore，先持久化 metadata，再在无 await 区间发布状态选项，保留最新心跳统计。Offline 保存原限额，随后尽力持久化 remove-peer unlimited；Up 恢复原限额，保留 Go 的两个独立尽力配置提交及部分失败行为。限额值使用现有 storelimit 类型和常量；新增的 Cargo 引用是已有 workspace crate。

清理跳过仍有 region 的 tombstone，但继续删除其他可清理 Store，最后返回 failed stores。限额删除按 Go 重试，成功后再删除 metadata、共享缓存及 rolling store stats。后台状态 worker 的清理命令在释放资源锁后发给同一 Coordinator；重新核对当前状态，避免清理已经恢复的 Store。没有后台私有 runtime。

## 验证方法

[独立网络测试](../../../pkg/server/http_store_state_test.rs)使用真实 TCP、tonic、HTTP、真实 etcd 与同一 MetaStore，覆盖未 bootstrap、非法输入、正常/强制/幂等下线、恢复、不可逆状态拒绝、非 TiKV、部分清理失败、删除后 RPC 查询、限额恢复、重启恢复及后台 tombstone 限额清理。独立[领域边界测试](../../../server/cluster/store_lifecycle_test.rs)补充所有剩余 Store 被驱逐、强制绕过和校验不提前发布状态的验证。

存储故障在真实事务中附加不存在的 lease，错误由 etcd 产生。metadata 写失败后内存与原始持久条目不变；tombstone 删除失败后记录仍可读并可重试。限额写失败保留已提交状态；Up 的 add-peer 配置提交失败后 remove-peer 恢复仍成功。真实 follower 转发写入 leader；事务边界暂停后取消节点，HTTP 退出、shutdown/join 完成且持久条目不变。没有以 mock 调用次数作为证据。

首次红灯：`cargo test --locked -p astersql-pd-server --lib task_052_ -- --nocapture` 实际运行1项，因缺 DELETE 路由导致 Content-Type 断言失败，退出101；不是编译失败。后续编译错误和测试准备问题单独保留在原始记录，不作为红灯证据。宿主负载升高曾使空 Offline Store 在断言前被真实后台 worker 清理；最终场景通过真实 region 心跳保留待迁移副本，再显式移走副本验证清理，不关闭 worker 或修改 Go。

[只读差分入口](../../../scripts/http-store-state-oracle.py)复用任务004守卫与已有 `bin/pd-server`，两端执行相同 HTTP/tonic 场景，并分别重启原有持久目录后验证状态。逐项比较状态码、JSON、RPC错误和状态字段；限额统一为同值 float64，无时钟字段或错误被忽略。Go binary build info 无 VCS revision，不宣称来自当前 HEAD；外部 etcd 不冒称 Go embedded-etcd 拓扑等价。

## 最终检查

默认 features；最终阶段使用 `GOMAXPROCS=2 CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=4`。每个 Cargo 阶段动态原子领取槽位并设置绝对路径；实际使用槽位1 `/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，中间检查阶段另用槽位2 `/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。下表最终命令均使用槽位1；槽位2的中断检查不计作最终通过。

| 命令 | 退出码 | 有效测试 |
|---|---:|---|
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据检查 |
| `cargo fmt --all -- --check` | 0 | 格式检查（修改后已先执行 `cargo fmt --all`） |
| `cargo check --locked -p astersql-pd-server` | 0 | 类型检查 |
| `cargo check --workspace --locked` | 0 | 全 workspace 类型检查 |
| `cargo test --locked -p astersql-pd-cluster --lib task_052_ -- --nocapture` | 0 | 1通过，0失败 |
| `cargo test --locked -p astersql-pd-server --lib task_052_ -- --nocapture` | 0 | 3通过，0失败，1默认忽略（差分另行显式执行） |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 181通过，0失败，13默认忽略 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-store-state-oracle.py` | 0 | 显式执行 `cargo test --locked -p astersql-pd-server --lib task_052_differential -- --ignored --nocapture`；1通过，0失败，0忽略；32项两端观察一致 |

`cargo test --workspace --locked` 最终退出0：2074通过、0失败、50默认忽略（含doc-tests）。状态：已完成。

前期 package 回归曾出现既有 `grpc_alloc_id_test::task_021_behavior` 的128号段时序差异；当前源码最终 package 回归该项通过，未修改旧测试。前期 workspace 300秒监督上限分别中断编译及执行；后一次已完成428通过、0失败、1忽略，但不算全量通过。移除人为上限后重新完整执行。期间真实服务启动/HTTP超时及重启端口 TIME_WAIT 失败均保留原始输出；测试通过保留真实副本、设置重启 listener 的 `SO_REUSEADDR` 修正准备流程，没有关停后台状态检查来规避行为。

开始/结束 Go只读守卫覆盖1101个文件（含未跟踪文件）；存在性、SHA-256及Git状态完全一致。最终验证源码快照覆盖1250个Rust/Cargo文件，验证后无源码变动；`plan.md`与基准一致。新增Python脚本AST、证据Markdown链接与 `git diff --check` 均通过。


原始输出与快照：[task-052-evidence.json.gz](task-052-evidence.json.gz)，SHA-256 `95c06d7748429f1a34539a4133b011fe8c661cecc677523bd4e5de975e4f2284`。包含首次红灯、所有阶段命令、最终全量输出、两端差分观察及Go binary build info。验证进程已结束，两个实际使用槽位的占用记录已释放，编译缓存保留。
