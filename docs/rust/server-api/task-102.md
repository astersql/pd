# 任务102：最终审计的阻塞修复进展

2026-11-02；基准 `a0f492cc1`。状态：部分缺口已修复，整体准入仍阻塞。

## 本轮实质修复

- `pkg/server/http_maintenance.rs` 将四个 maintenance 方法接到已有
  `StorageEndpoint` 的原子启动、删除、单项读取与分页查询；与 runtime 的其他
  服务共用 `recovery_marker_store()` 返回的 MetaStore，不新增领域算法。
  路径、状态、字符串、冲突对象以只读
  `server/apiv2/handlers/maintenance.go` 及
  `pkg/storage/endpoint/maintenance.go` 为契约依据。
- 全局锁跨 task type 互斥；PUT 原样读取描述；无任务404、冲突409、损坏单项500；
  全量读取跳过锁和坏记录。取消使用现有请求 cancellation，错误返回 JSON 字符串。
- `ServiceMetrics` 在宿主 Registry 注册原有 Go 名称/HELP/标签的
  `pd_maintenance_task_info`，仅事务成功后置1/0，宿主退出注销。
  task_id 动态标签与 Go 契约一致，操作属于低频维护路径；保留删除后零样本，
  不擅自更改其历史指标行为。
- `http_v2_boundary` 仅保留26个尚未准入的领域方法。原任务078的拒绝测试和
  拒绝差分只检查这26项；maintenance由新增独立行为测试覆盖，未把它静默归为
  原拒绝差分通过。历史任务078证据不改写。
- `pkg/proto/build.rs` 优先消费显式 `GOMODCACHE`，避免在已有协议ZIP时仍强制
  执行不存在的 `go env`。缺省继续调用只读 Go 查询；固定版本和ZIP校验未改变。

## 行为证据与局限

独立模块 `pkg/server/http_maintenance_test.rs` 已在 lib.rs 显式注册。
`task_102_maintenance_persistence_and_conflicts` 使用真实 TCP、完整 PdNode runtime
和同一个 MemoryStore，验证空查询、创建、描述和时间、跨类型冲突、错误ID删除、
关闭并按相同成员身份重建宿主后读取、删除并释放锁、损坏JSON和8并发请求唯一胜者，
以及实际指标值和宿主关闭后的注销。该重建保留同一内存后端，**不是进程崩溃恢复或
etcd durability 证据**。未使用 mock 调用次数或固定成功响应。

修复前已编译运行，首个行为断言 `GET maintenance` 实收503、预期404，退出101；
修复后1个测试通过、0失败、0忽略。开发中重建地址变化触发 IdentityConflict，
测试改为复用同一地址，未放宽生产身份规则。

本机无可用Go工具链、etcd fixture二进制或旧pd-server oracle，未重跑Go差分。
Go来源和任务078保存的历史运行输出只是契约参考，不冒充本轮差分。
GC的15个RPC、keyspace/TSO组/meta-service组/MCS、主题10拓扑与durability，
来源manifest漂移归因及完整兼容证据仍未完成，任务102不得标为整体完成。

## 验证环境

槽位1：`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
原子mkdir领取，owner记录执行进程；仅本阶段使用，结束后释放锁并保留缓存。
工具链下载未完成，实际用已安装 `RUSTUP_TOOLCHAIN=stable`（rustc 1.99.1）。
`CARGO_BUILD_JOBS=6`；最终回归 `RUST_TEST_THREADS=1`。
`GOMODCACHE=/tmp/task102-proto-cache`，仅下载固定ZIP、不提取Go源码；校验仍为
`aff09d074d493874bfd7e3e63c766a8d93f42901ff235dde2d2ced0c7f2a615e`。
`PROTOC=$HOME/.cargo/registry/src/rsproxy.cn-e3de039b2554c837/protoc-bin-vendored-macos-aarch_64-3.2.0/bin/protoc`。
依赖镜像出错后通过static.crates.io补齐缓存并逐项核对Cargo.lock SHA-256；
未改锁文件、仓库registry配置或任何Go源码/模块文件。

确切命令、退出码和日志见 [本轮证据](task-102-evidence.json.gz)。

最终复验：metadata、fmt --check、workspace check退出0；task_102_和
task_078_cancel_and_unread_body各1测试通过，0忽略。workspace test退出101，
已执行98通过、6失败；6项均在定位Go工具链时失败，尚未启动etcd fixture，
其他crate未由该次命令继续运行。不声称全工作区测试通过。
compat包级、四crate联合回归、pd-server双实现构建/运行未执行：缺少Go
及oracle环境，且最终领域准入仍缺失。Go只读守卫1101项一致，Git状态无Go差异。
