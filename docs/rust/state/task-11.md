# 任务11：存储统计容器生命周期

实现前 HEAD `0fd29e47074ed4a12a93229de252403c6f3964d8`；Go基准
`69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，store.go/store_test.go无基准差异。
已读两份只读plan、根规则、Copilot、ADR、依赖基线和移植指南，无更深规则。
批次10及累计1–9证据见[task-10.md](task-10.md)，复用其真实RollingStoreStats。

## 实现前 oracle 与契约

既有TestFilterUnhealthyStore：新建cluster，1–5均为当前心跳并Observe空报告；
快照长度5。1心跳改为24小时前、2设Tombstone、3从cluster删除；过滤后仅4、5。
[只读运行输出](task11/go-oracle.out)。命令：
`GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics -run '^TestFilterUnhealthyStore$' -count=1 -v`。
调用链NewBasicCluster/NewStoreInfo/PutStore/Clone/DeleteStore仅内存；Observe、
movingaverage和core查询聚合已在批次10审查。core注入点位于region桶/子树，
本测试不进入。TestMain仅goleak与WaitForEtcdConnections，不创建etcd或写源码；
go test不运行generate，readonly/off禁止模块修改与下载，不运行make或failpoint命令。

源码推导（不冒称新增Go运行差分）：GetOrCreate只初始化缺失项；重复删除无操作；
Observe/Set/region输入均可重建删除项。GetStoresLoads复制12维值，缺store近期CPU为0。
过滤missing、is_removed、is_unhealthy、physically_destroyed；健康恢复本身不复活缓存，
后续报告重新创建空历史。健康判断复用core，显式宿主时钟，严格大于600秒。
Go informer仅GetStoresLoads。Rust返回独立值快照及RollingStoreStats深拷贝，
不泄露可变缓存引用；单个RwLock保护容器及历史，快照/删除/观测互斥。

## 实现与测试驱动证据

`pkg/statistics/stores.rs` 新增 StoresStats、StoreKindLoads、StoreStatInformer；
lib.rs 显式注册生产和独立测试文件，无manifest/lock/feature变更。
先构造可编译容器接口及真实rolling接线，保留filter为空；
[红灯](task11/red.out)：4行为测试实际执行，2 passed/2 failed，退出101。
Go场景实际[1,2,3,4,5]期望[4,5]；边界场景实际[1,2,3,4]期望[1,4]。
快照与并发测试初次即通过，不为制造失败破坏已有正确逻辑。
随后接入core健康谓词，未mock统计、状态或计算结果。

新增源码推导扩展测试覆盖：空集、600秒严格边界、物理损毁、健康Removing、
恢复后不自动重建、重复删除、重建清空旧历史、深拷贝历史/值快照修改隔离、
region输入独立写率、缺失CPU返回零、4线程各100轮观测/快照/删除。
Go原有运行场景过滤后[4,5]与Rust一致；扩展场景不冒称Go运行输出。
生产说明见[stores.rs.md](../../../pkg/statistics/stores.rs.md)。

## 资源与只读约束

所有Cargo调用经[槽位包装器](task11/slot.sh)原子mkdir动态领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner含计划/任务/会话/PID；命令组退出后finally清理自有锁，信号路径终止并wait
子进程后释放，缓存保留。无target-dir覆盖，Rust修改后先cargo fmt --all。
无范围外Rust格式化差异，两份指定plan保持只读。

已审查pkg/proto/build.rs：固定ZIP校验SHA256后仅在槽位OUT_DIR解压/生成。
workspace Etcd夹具只读go build既有scripts/rust-etcd-fixture/main.go到槽位，
WAL/日志/ready也写槽位；Drop EOF关闭、等待，必要时kill并wait，无failpoint或generate。
Cargo子进程继承GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off。
本任务纯内存容器，无后台任务、网络或持久化依赖；workspace回归覆盖真实MemoryStore
与EtcdMetaStore，不为本任务新增后端或运行时。

## 最终验证

[验证脚本](task11/validate.sh)、[完整日志](task11/validation.out)及
[metadata](task11/metadata.out)保存确切命令/退出码。

| 命令 | 退出码 / 有效测试 |
| --- | --- |
| 上述Go oracle命令 | 0 / 1 passed |
| `cargo fmt --all` | 0 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_11_stores -- --nocapture` | 0 / 4 passed，0 failed，0 ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0 / 16 passed，0 failed，0 ignored |
| `cargo check --workspace --locked` | 0，仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0 / 1031 passed，0 failed，1既有ignored |
| `python3 docs/rust/state/task11/readonly.py` | 0 / 1101个Go/模块文件集合及SHA256一致，Git状态干净，文档链接与diff检查通过 |

workspace既有ignored为tso_processes，不计入任务证据；本任务4项行为全部执行。
Go/模块[起点哈希](task11/go-before.json)及[最终核对](task11/readonly.out)
aggregate SHA256均为 `ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
包含未跟踪文件；没有新增Go文件，也没有Go/模块文件变更。
[资源核对](task11/resources.out)：无槽位1运行二进制，slot-1.lock已释放，缓存保留。
用户已有计划变更不纳入提交；两份指定plan未修改；日志仅清理行尾空白。

状态：已完成，无待回归项；只验收本容器生命周期，不代表整个统计主题完成。
