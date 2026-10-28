# 任务009：装配Region心跳服务

状态：已完成（RegionHeartbeat真实runtime接线边界）。

基准HEAD：`ec9c034e0`。依赖批次8已提交，前三份source/wire/surface清单及task-001至008证据存在；主题6准入由task-001固定。Application → RoleServices → ClusterHost已在任务5/7装配RegionHeartbeatService；当前不存在默认None缺陷，因此补实际验收，不人为移除接线制造失败。未新建平行wire实现或复制领域算法。

## 来源与边界

- source-inventory.md的任务9整行、region_heartbeat_adapter.rs的RegionHeartbeatService、server/cluster/heartbeat_effects.rs来源行，以及共享server/grpc_service.go中RegionHeartbeat（1281起）对应本任务。其它方法继续由各自owner验收。
- wire-inventory.md的RPC-RegionHeartbeat行由本次真实双向流、epoch错误、EOF、取消及只读Go差分覆盖。角色切换和事务fencing由包级task_007回归覆盖；转发、限流、安全、调度指令流仍由各自owner承担。
- surface-inventory.md的RegionHeartbeat转发指标/安全行归90/91/92，心跳指标归15/86/101；本任务未改变配置或新增指标，不扩大其验收范围。
- 只读Go oracle：server/grpc_service.go::RegionHeartbeat、pkg/core/region.go::errRegionIsStale及server/cluster/cluster_worker.go::HandleRegionHeartbeat。固定kvproto commit `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。
- proto仅在server适配层转换；ClusterHost复用canonical StatePipeline、region树、统计、真实etcd MetaStore和host worker。无生产逻辑或依赖变更。

## 独立行为证据

`pkg/runtime/wire_region_heartbeat_test.rs::task_009_behavior`在lib.rs显式注册。真实TCP/tonic双向流上报告region 7/version 2/size 2 MiB，观察共享snapshot及同一MetaStore持久元数据。成功不发送ack。

同一流发送version 1/size 99 MiB冲突：返回cluster_id=505、UNKNOWN、`region is stale`消息前缀；树中的version 2/size 2及持久元数据不变。随后version 3/size 3 MiB仍被同流接受，EOF正常，store索引region/leader计数均1、size均3，持久epoch为3。开放第二条流后取消host，stream EOF、worker join、listener关闭、Registry清空、MetaStore引用释放；HTTP health实际返回true。

未推进或比较墙钟，不需伪时钟。初次独立测试即通过，当前既有实现已满足行为；没有编译失败或人为行为失败作为TDD证据。故障使用真实epoch拒绝和取消，事务失败及旧term拒写由task_007当前回归覆盖。

增强现有cluster_host_test.rs只读差分：在原先region/version/size/持久状态/过期错误之外，同时比较UNKNOWN错误类型及稳定消息前缀。Go与Rust元数据调试打印格式不同，未声称完整错误字符串字节相等。Go oracle来自既有bin/pd-server，构建信息无VCS revision，不声称它来自当前HEAD。

## 验证记录

所有Cargo命令默认features，从仓库根运行；原子mkdir动态领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。精确命令、退出码及完整日志见压缩证据，最终测试数在下方补记。

| 确切命令 | 退出码 | 有效测试数 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-runtime --lib task_009_ -- --nocapture`（初次及最终） | 0 | 每次1通过/0忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 12通过/3 oracle忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all -- --check` | 0 | 格式 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked` | 0 | 1937通过/4 oracle忽略 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 真实二进制 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case cluster-host` | 0 | 1通过/0忽略 |
| `git diff --check` | 0 | 差异检查 |

双实现观察均为region=7/version=2/size=2、persisted=true、stale_error=true、stale_kind=1、stale_message=true、healthy=true；只归一化集群身份和监听地址，不比较墙钟或protobuf调试打印后缀。监督器inputs_unchanged、readonly_unchanged、work_directory_cleaned均true。独立Rust测试验证更细的状态隔离、EOF及取消；不把这些未在Go场景比较的细节声称为双实现差分。

原始日志、精确argv/退出码、监督器完整报告及Go前后完整清单见[压缩证据](task-009-evidence.json.gz)，SHA-256：`6be54fd4b07eb60140a7b407e9eaec4649e98258be16250d493e34637b328aad`。开始/结束Go保护集合1983文件（含ignored/untracked），路径、内容SHA-256及Git状态完全相同；排序JSON快照SHA-256均为`eafa2dc5f0470621e39729f6ab4572c6cc35be32ae5d02034510e0e746ba4cbb`。

所有Cargo和槽位产物进程结束，自己的slot-1 owner记录及锁目录已释放，缓存保留。未修改Go/模块、只读plan.md或来源史诗；原有任务1–8删除保留且不纳入提交。按要求删除本任务文件。
