# 任务008：装配Store心跳服务

状态：已完成（StoreHeartbeat真实runtime接线边界）。

基准HEAD：`8066229f9`。已读取只读plan.md、任务文件、前三份source/wire/surface清单与任务1–7证据。任务7已交付角色接线及term fencing；任务5已将Application和ClusterHost心跳服务装配，任务8的默认None缺陷在当前版本已不存在。按任务要求补当前证据，不删除已有接线来制造失败。

## 行为与源码映射

- source-inventory.md任务8整行、store_heartbeat_adapter.rs的StoreHeartbeatService/apply_store_heartbeat来源，以及server/grpc_service.go共享来源中的StoreHeartbeat（927–1010）对应本任务。共享文件其它方法/worker/failpoint行继续由各自owner验收，不因文件共有而扩大本任务契约。
- wire-inventory.md的RPC-StoreHeartbeat行：真实runtime成功/错误header、共享状态和取消在新增独立测试验证。角色和事务fence由当前包级/全量运行的task_007测试覆盖。
- surface-inventory.md中StoreHeartbeat相关转发指标及安全/限流归90/91/92；本任务没有新增指标或配置，不解除其它owner准入。
- Go只读oracle为server/grpc_service.go::StoreHeartbeat/checkStore、server/cluster/cluster.go::HandleStoreHeartbeat。协议仍固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。
- 真实调用链：PdNodeBuilder → RoleServices → 当前Application/ClusterHost → canonical apply_store_heartbeat。proto转换和错误header保留在server适配层。无需新建wire_store_heartbeat.rs或复制领域实现。

## 独立验收

[wire_store_heartbeat_test.rs](../../../pkg/runtime/wire_store_heartbeat_test.rs)在lib.rs显式注册。task_008_behavior通过真实etcd MetaStore启动PdNode，并经TCP/tonic发送请求：

- 连续报告capacity 100/200与available 50/100；响应cluster_id=808、无错误、state=Serving；同一host snapshot立即观察最新值。
- store 999返回UNKNOWN、`store 999 not found`；tombstone store 3返回STORE_TOMBSTONE、`store is tombstone`；两者header保留cluster_id。
- 缺失stats返回UNKNOWN transport错误，错误cluster ID返回FAILED_PRECONDITION；上述错误不更改store 1的200或store 3的0。
- 宿主取消后join worker；旧host心跳与snapshot拒绝，TCP listener关闭，Registry清空，MetaStore引用释放。

时间值没有人为推进或等值比较；测试不依赖固定墙钟。StoreStats更新是共享缓存行为；没有声称心跳统计本身重启后持久恢复。Go的周期SaveStoreMeta与unsafe recovery/DR控制器等完整域行为不在本次接线扩展，现有unsupported边界保留。任务7的真实lease revoke/旧term事务拒写提供当前事务故障回归，不用mock计数替代状态。

首次新增测试有snapshot key类型误用（u64误当StoreMeta），编译退出101；修正后1个实际行为测试通过。编译错误不作为行为失败证据；当前既有接线正确，未人为制造失败。

## 验证记录

所有Cargo命令从仓库根运行、默认features；原子mkdir动态领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。持有记录包含计划/任务/会话/PID；所有进程结束后释放自己的锁，保留缓存。

| 确切命令 | 退出码 | 有效测试数 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-runtime --lib task_008_ -- --nocapture` | 0 | 1通过/0忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 11通过/3 oracle忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all -- --check` | 0 | 格式 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked` | 0 | 1936通过/4 oracle忽略 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 真实二进制 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case cluster-host` | 0 | 1通过/0忽略 |
| `git diff --check` | 0 | 差异检查 |

只读监督器实际重跑existing Go PD与Rust场景，二者capacity=100、missing_store_error=true、healthy=true一致；同一场景还确认region=7/version=2/size=2、持久region与过期错误一致。这些附带region结果是相邻回归，不作为StoreStats持久化承诺。精确header/state/错误字符串/非法请求与取消由新增Rust真实网络测试核对；不把未比较的Go错误字符串或取消行为说成字节差分通过。监督器exit=0、inputs_unchanged=true、readonly_unchanged=true、work_directory_cleaned=true；无本任务Go PD/slot二进制残留。

Go只读守卫开始/结束完整扫描1983文件（含ignored/untracked）：路径、每文件SHA-256与Git状态一致。排序JSON摘要`300a4b0fdad52dd6b67f5daa919a1720ca47c97ecc7a81d364f6896f12d094a2`；未修改任何Go或模块文件，未执行make/failpoint/生成器。只读plan.md和原有任务1–7删除保持不变。

原始日志、准确argv/退出码、源码哈希、监督器报告和完整Go前后清单见[压缩证据](task-008-evidence.json.gz)，SHA-256：`06edf7ee33262dc0ccfa95389d227625665306f009b4209beb03d4485ed7cc72`。本任务仅增加独立验收和证据，无生产逻辑变更。
