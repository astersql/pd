# 任务12：负载历史采样与回收

实现前 HEAD `29f3d43a4d03a9de382ece4bc6be8b88f4f4aed6`；Go 基准
`69ba16e6d47dfe59a21498c21641e1c1eaa726e0`。两个 store_load Go 文件无基准差异。
已读两份只读 plan、根规则、Copilot、ADR、依赖基线与移植指南；无更深规则。
前置批次11及累计依赖证据见 [task-11.md](task-11.md)。

## 实现前 oracle

[Go 只读运行输出](task12/go-oracle.out)：
`GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics -run '^TestHistory(Loads|GCDoesNotReaddRemovedStore)$' -count=1 -v`，退出0，2 passed。
审查 store_load.go、store_load_test.go、store_hot_peers_infos.go、store_collection.go：
仅内存、time.Now 与 core 状态/collector 查询，无需 failpoint 注入。
TestMain 仅 goleak 与 WaitForEtcdConnections（等待瞬态连接），无 Go/模块写入，
不运行 make/generate，readonly/off 禁止模块更新与下载。

运行输入：300s/0 首次加 [1,2,3,0] 得容量10；随后10次添加，各物理槽位均为该向量。
1ms/1ms 容量1；1ms/1s、0/1s、0/0 的历史均空。
GC 场景：Serving store1 经 SummaryStoresLoad(Read,Region) 加入；Tombstone 后 GC 清除；
再次汇总 removed store 不重建历史。Go 断言全部通过，未新增或改动 Go 工具。

源码推导扩展契约（单独标识，非新增 Go 运行差分）：间隔严格小于时拒绝、等于时采样；
间隔0且duration非0固定容量10，与传入duration无关；非零间隔容量向下取整。
返回 [dim][物理槽位]，初次采样填充零，绕回不排序。四维 byte/key/query/CPU。
按 store/RW/Leader/Region/Witness 隔离；GC 仅保留 !IsRemoved 的 store ID，
Removing/physically_destroyed 仍保留。配置相同保留数据，任一改变清空所有历史。
Go Add(ID) 本身不检查移除，防重建在汇总调用方；Rust 提供 add_store 复用 core 谓词，
后续汇总应使用此入口。无调度汇总实现重复引入本任务。

## 测试驱动证据

lib.rs 注册 store_history 与独立 store_history_test；使用既有 core 与 statistics-utils。
必要新增直接依赖 core-constant（已有 workspace dependency），在槽位内先
`cargo update --offline -p astersql-pd-statistics` 更新 Cargo.lock，退出0，未更新第三方版本。
`cargo fmt --all` 后运行规定过滤器，[红灯](task12/red.out)：退出101，实际4项行为均失败。
容量断言 left 0/right 10；环快照 left 四个空向量/right 四个[1,0]；
隔离 left []/right [100,0]；GC流程首次采样非空断言失败。
最小接口编译成功，非缺符号或零测试；随后实现真实采样、环、清理与metadata guard。

实现为 caller-owned 同步领域值；使用有界游标避免累计计数溢出，深拷贝历史快照。
本任务不引入后台任务、存储或网络，故无本任务 cancel/join；workspace 基线仍运行
既有真实 MemoryStore 与 EtcdMetaStore 契约。已审查 proto/build.rs 的固定ZIP校验及
OUT_DIR生成，Etcd fixture的只读Go build、输出目录、EOF关闭与Drop wait/kill回收。
所有子进程继承 GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off。

## 最终验证

[槽位包装器](task12/slot.sh)每阶段按1–10原子mkdir领取，本次各阶段均实际领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录计划、任务、会话与PID；finally先终止并wait仍运行的子进程组再释放自有锁，
正常、失败与信号退出均清理锁；缓存保留，无 target-dir 覆盖。

[验证脚本](task12/validate.sh)、[完整输出](task12/validation.out)、
[metadata](task12/metadata.out)记录确切命令与退出码。

| 命令 | 退出码 / 有效测试 |
| --- | --- |
| 上述 Go oracle 命令 | 0 / 2 passed |
| `cargo fmt --all` | 0 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_12_store_history -- --nocapture` | 0 / 4 passed，0 failed，0 ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0 / 20 passed，0 failed，0 ignored |
| `cargo check --workspace --locked` | 0，仅既有 cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0 / 1035 passed，0 failed，1既有ignored |
| `python3 docs/rust/state/task12/readonly.py` | 0 / 1101个Go/模块文件集合与SHA256一致，Git状态干净，链接与diff检查通过 |

workspace 忽略项是既有 tso_processes，不计本任务证据。真实 MemoryStore/EtcdMetaStore
测试通过；资源已退出。[资源核对](task12/resources.out)确认无槽位二进制运行、锁释放。

复核时加强了间隔测试：在12秒被拒绝、9秒倒退被拒绝后立即检查仍为[1,0]，
13秒恰好间隔3秒后检查[1,2]，防止后续绕回掩盖错误；生产代码未改变。
随后在新动态槽位阶段依次执行 `cargo fmt --all`、`cargo fmt --all -- --check`、
上述聚焦命令、包级命令，[最终复验](task12/final-focused.out)整体退出0，4与20项通过。

[Go/模块起点哈希](task12/go-before.json)和[最终核对](task12/readonly.out) aggregate SHA256
均为 `ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`；
包含未跟踪文件，没有新增Go文件或模块差异。[起点Git状态](task12/go-status-before.out)为空。
无范围外Rust格式化变化；两份只读plan未修改，用户既有计划变更不纳入本任务提交。

状态：已完成，无待回归项。仅验收历史采样边界，后续汇总复用 add_store 的移除保护。
