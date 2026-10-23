# 存储门面实施证据

## 任务 1：装配默认存储门面（批次 54）

日期：2026-10-23（Asia/Shanghai）。

### 准入与 Go 依据

核验批次 53 提交 `225ccd69d`、`docs/rust/theme3-存储端点.md` 任务 34 节及
canonical endpoint 的 batch_ops 实物：双后端 4 passed、workspace 602 passed /
0 failed / 1 既有 ignored，前置准入满足。已阅读根 AGENTS.md、Copilot 指令、
ADR-001、porting-guide、执行技能和只读 plan.md；无更深 AGENTS.md。

`pkg/storage/storage.go` 的 Storage 嵌入 kv.Base 和 endpoint 各业务接口；
`memory_backend.go` / `etcd_backend.go` 均将同一底层 KV 交给 StorageEndpoint，
加密 manager 为 nil。这三个来源文件相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。
不迁移 region source 切换、加载状态、bootstrap 判定或本地后端。

检查已有 TestMain、TestBasic、TestGCStateJSONUtil、etcd fixture 与调用路径后，
确认所选用例无需 failpoint 注入且只写临时 etcd 数据及编译产物。运行：

```text
GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage -run '^TestBasic$' -count=1 -v
GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage/endpoint -run '^TestGCStateJSONUtil$' -count=1 -v
```

均退出 0，各 1 个顶层测试通过，日志 `/tmp/pd54-go.log`、`/tmp/pd54-go-json.log`。
整包 `go test -mod=readonly ./pkg/storage -count=1` 未执行，含需注入的
TestLoadRegionsExceedRangeLimit、TestSaveTimestampWithTimeout、TestRegionReload；
Go 只读约束优先，未启停 failpoint。相关门面行为由选中用例与只读源码证明。

本任务不新增 codec。复用 TestGCStateJSONUtil 实际写入并读取的 int64(1) →
`"1"`（hex 31）字节断言，与 Rust facade endpoint 写入后从 KV 读取的 b"1"
比较；反向 KV 写入 b"2" 后 endpoint 解码为 2。以上为现有 Go 用例运行的
字节证据及对应 Rust 比较，不把源码推导当成新增跨语言运行工具的输出。

### 实现与行为红绿灯

canonical `astersql-pd-storage` 新增 `facade.rs::Storage`，公开 `new`、`base`、
`endpoint`、`endpoint_mut`。构造时克隆宿主提供的同一个 Arc<dyn MetaStore>，
分别交给既有 Base 与 StorageEndpoint；不复制 KV、事务、codec 或业务方法，
保留 MetaStore 及已有 KV 重导出。显式访问器保留 Base 与 endpoint 不同的 load
返回语义，mutable endpoint 支持既有 region scan 游标。依赖方向仅
storage → endpoint → kv；新增依赖复用 workspace 路径，不新增第三方依赖。
Cargo.lock 只新增 storage 对 endpoint 的依赖边。

独立 `facade_test.rs` 在 lib.rs 显式注册，内存与真实 etcd 共用同一 contract。
最小可编译装配先让 endpoint 使用另一 MemoryStore：指定聚焦命令退出 101，
0 passed / 2 failed / 0 ignored；两个入口均在 endpoint 保存后 KV 读值的真实
断言失败，实际 []，期望 [49]。红灯日志 `/tmp/pd54-red.log`。

改为传入同一 Arc 后，原测试不削弱断言，同一聚焦命令退出 0，
2 passed / 0 failed / 0 ignored，日志 `/tmp/pd54-green.log`。
共用合同验证 endpoint→KV 与 KV→endpoint、宿主原 store 可见、多个门面共享、
callback txn 提交可见与 callback 错误回滚、坏 JSON 分类、缺失/空值区别、
删除、真实空键错误传播、可变 endpoint 访问及释放一个门面不关闭宿主 store。
真实 etcd 使用既有 `kv/etcd_fixture_test.rs`，随机 loopback 地址和独立数据目录，
drop 关闭并等待进程退出；本故事不包含断连、lease 或到期行为。

### Cargo 槽位与最终基线

各阶段 `/tmp/pd54-cargo.sh` 从根目录依次以原子 mkdir 动态领取槽位，均为
**槽位 1**，绝对路径
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 含 plan=storage-facade/task=1/batch=54/session=default-facade/PID；
准备 PID=8056、红灯 PID=8170、绿灯 PID=8317。EXIT/INT/TERM 设置清理，
阶段结束清理自己的 owner/锁并保留缓存。fixture 继承相同 target 及上述只读
Go 环境；先审查既有 fixture Go build/启动链，无 Go 源码生成或模块写入。

最终基线 PID=8487，日志 `/tmp/pd54-baseline.log`，metadata 保存于
`/tmp/pd54-metadata.json`。命令及退出结果：

| 命令 | 退出码与有效测试数 |
| --- | --- |
| `cargo fmt --all` | 0；Rust 修改后先执行，未波及其他文件 |
| `cargo check --workspace`（同步依赖锁） | 0；仅新增一条既有 crate 依赖边 |
| `cargo test --locked -p astersql-pd-storage --lib facade -- --nocapture`（红灯） | 101；0 passed / 2 failed / 0 ignored |
| 同一聚焦命令（绿灯） | 0；2 passed / 0 failed / 0 ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；既有 cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；83 target，604 passed / 0 failed / 1 既有 ignored |
| 上述两个只读 Go 命令 | 均 0；各 1 个顶层测试 |
| `git diff --check`、`git diff --cached --check` | 均 0 |

全部 Cargo、fixture 进程结束，自己的 slot-1 owner 与锁已释放，缓存保留。
开始/结束 1101 个受跟踪 Go/模块文件路径及 SHA-256 完全一致，Go/模块 Git 状态
为空，无新增 Go/模块文件。审查 facade.rs、独立测试、lib.rs、crate manifest、
Cargo.lock 与本节证据；无根 manifest、plan.md 或来源主题修改。
用户已有观测工具计划修改及前序端点编号任务删除均保留、排除本次提交。
状态：已完成；批次 54 默认存储门面故事及所需验证证据满足。

## 任务 2：区域存储源切换（批次 55）

日期：2026-10-23（Asia/Shanghai）。状态：已完成。

### 准入与只读 Go 证据

核验批次 54 提交 `6a4880071`、本文件任务 1 的证据及 canonical Storage：
双后端 facade 2 passed，workspace 604 passed / 0 failed / 1 既有 ignored，
依赖已满足。阅读执行技能、只读 plan.md、ADR-001、porting-guide 与仓库规则；
无更深 AGENTS.md。`storage.go`、`storage_test.go` 相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。

Go `NewCoreStorage` 初始使用默认源；`TrySwitchRegionStorage` 对非 core 返回 nil，
对 core 原子选择源并返回请求的 endpoint；`RetrieveRegionStorage` 无视当前选择，
core 返回注入源，普通 facade 返回自身。`LoadRegion`、`LoadRegions`、`SaveRegion`、
`DeleteRegion` 选择区域源，嵌入的普通 KV、事务及其他 endpoint 始终使用默认源。
本任务不实现 TryLoadRegionsOnce、AreRegionsLoaded、Flush/Close 或本地数据库。

已检查 TestMain、TestTrySwitchRegionStorage、TestTryGetLocalRegionStorage、
mustSaveRegions 及 memory/LevelDB 只读调用链：选中用例不依赖 failpoint 注入，
只写编译缓存和临时 LevelDB 数据。实际运行：

```text
GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage -run '^(TestTrySwitchRegionStorage|TestTryGetLocalRegionStorage)$' -count=1 -v
```

退出 0，2 个顶层测试通过，日志 `/tmp/pd55-go.log`。
整包 `go test -mod=readonly ./pkg/storage -count=1` 未执行：含需注入的
TestLoadRegionsExceedRangeLimit、TestSaveTimestampWithTimeout、TestRegionReload；
遵守 Go 只读硬约束，不启停 failpoint，不把所选用例冒称整包验证。

无新增 codec。复用 `pkg/proto/wire-fixtures.json` 已有 Go region_heartbeat 输出及
`docs/rust/theme3-存储端点.md` 批次 26 的既有运行证据：输入
`metapb.Region{Id:99,StartKey:[]byte{0,1},EndKey:[]byte{255}}`，
Region 子消息 hex `0863120200011a01ff`。测试从既有 JSON 原始 hex 提取子消息，
校验 tag/length，比较默认源真实 KV 保存字节，未以 Rust 编码构造预期。
其余状态断言依据 Go 源码与本轮运行测试，不宣称新增逐场景跨语言运行差分。

### 实现与红绿灯

在 canonical Storage 原位扩展：`facade.rs` 仅增加可选 StorageEndpoint 与 AtomicBool
的字段/默认初始化；`region_switch.rs` 实现 new_core、retrieve_region_storage、
try_switch_region_storage 及四种区域操作。切换使用 SeqCst 原子读写；切换返回引用
直接对应请求的源，不因并发的后续切换返回另一源。base/endpoint/endpoint_mut
保持默认源语义；扫描复用每个 endpoint 自己的游标，不增加加载状态或私有 runtime。
注入的是宿主装配的 canonical StorageEndpoint，不增加平行 RegionStorage 实现；
两路 MemoryStore 仅为真实可注入存储的测试装配，不宣称生产本地后端。

独立 `region_switch_test.rs` 在 lib.rs 显式注册，region_switch_memory 与
region_switch_etcd 共用 contract：初始默认、true/false 往返和重复选择、返回源及
retrieve 身份、同 ID 两源独立内容、区域点读/写/删/扫描、普通 JSON 与 KV callback
事务留在默认源、坏 protobuf 的 found 标志/错误传播、非 core 拒绝切换并仍能 CRUD、
取消扫描。etcd 入口使用真实外部 etcd 默认源和独立 MemoryStore 注入源；memory
入口使用两个独立 MemoryStore。Go fixture 既有 build/启动链不写 Go/模块文件，
继承只读 Go 环境；随机 loopback 端口、独立数据目录，Drop 等待进程结束。
本任务不引入断连、lease 到期或时钟测试。

最小可编译接口先保留默认路由：首个辅助红灯在返回引用身份断言失败。
保留该断言、调整状态断言顺序后，聚焦命令在两个后端均实际得到
`local_view.load_region(99) == None`，期望 start_key=[2] 的 RegionMeta，
退出 101，0 passed / 2 failed / 0 ignored（`/tmp/pd55-red-state.log`）。
补全切换原子写入后，同一测试无削弱、无忽略，退出 0，2 passed / 0 failed /
0 ignored（`/tmp/pd55-green.log`）；最终重跑同样通过。

首轮 workspace 基线遇到新增测试和 facade_test 各自注册同一 fixture helper，
独立静态计数器产生相同 PID/序号目录，区域 etcd fixture 提前退出（退出 101，
日志 `/tmp/pd55-baseline.log`）。在本任务内修复：lib.rs 统一注册既有 helper，
两组测试仅导入这一模块，共用唯一计数器与 Go binary 缓存；不改 helper 源码。
随后重新执行完整基线。

### 槽位、验证与差异审查

所有 Cargo 阶段通过 `/tmp/pd55-cargo.sh` 从仓库根依次原子 mkdir 动态领取，
实际均为 **槽位 1**，绝对路径：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录 plan=storage-facade/task=2/batch=55/session=region-switch/PID；
准备与辅助红灯 PID=13350，状态红灯 PID=13563，绿灯 PID=13727，首次基线
PID=13892，修复后最终基线 PID=15202。EXIT/INT/TERM 设置释放自己的 owner/锁；
阶段结束先等全部 Cargo 与 fixture 子进程退出，保留编译缓存。

依赖仅复用 workspace 已有 core/keypath 路径、将已有 tokio-util 提升为生产依赖；
Cargo.lock 仅新增 core/keypath 两条依赖边。Rust 修改后先 cargo fmt --all，未波及
其他任务 Rust 文件。最终基线日志 `/tmp/pd55-baseline-final.log`，metadata 输出
`/tmp/pd55-metadata.json`。

| 确切命令 | 退出码与有效测试数 |
| --- | --- |
| `cargo fmt --all` | 0 |
| `cargo check --workspace`（首次同步 Cargo.lock） | 0；随后审查只含两条依赖边 |
| `cargo test --locked -p astersql-pd-storage --lib region_switch -- --nocapture`（状态红灯） | 101；0 passed / 2 failed / 0 ignored |
| 同一聚焦命令（最终绿灯） | 0；2 passed / 0 failed / 0 ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；83 target，606 passed / 0 failed / 1 既有 ignored |
| 上述只读 Go 命令 | 0；2 个顶层测试通过 |
| `git diff --check`、`git diff --cached --check` | 均 0 |

全部 Cargo 与 fixture 子进程退出，自己的锁已释放，缓存保留。
开始/结束 1101 个 Go/模块文件路径与 SHA-256 完全一致，Go/模块 Git 状态为空。
已审查上述实现、测试、注册、manifest、lock 与本节证据。plan.md、根 manifest
及来源主题均未修改。保留用户既有观测工具计划修改与前序任务文件删除，
不包含在本任务提交中。批次 55 的行为与验证证据满足。

## 任务 3：区域只加载一次（批次 56）

日期：2026-10-23（Asia/Shanghai）。

### 准入与只读 Go 依据

核验批次 55 提交 `59c710558`、本文件任务 2 的证据与 canonical Storage：
区域切换双后端 2 passed、workspace 606 passed / 0 failed / 1 既有 ignored，
依赖准入满足。已阅读执行技能、只读 plan.md、ADR-001、porting-guide 与仓库
说明；无更深 AGENTS.md。storage.go、storage_test.go 相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。

Go `TryLoadRegionsOnce` 在非 core 时直接委托 LoadRegions；core 的状态是
unloaded/fromEtcd/fromLeveldb。默认源每次扫描，成功才覆盖为 fromEtcd；本地源
仅在 unloaded 时扫描，成功改为 fromLeveldb。任一来源的失败均保留原状态，
切换本身不重置状态。因此默认先成功再切本地时，跳过本地扫描，且
`AreRegionsLoaded` 返回 false；切回默认又返回 true。源码中的 mutex 保护
加载和状态读取；Rust 复用现有扫描接口的独占 &mut self，跨 await 保持借用，
安全调用方不能同时扫描、切换或查询，无需新增 runtime 或异步锁依赖。

检查 TestMain、TestLoadRegionsToCache、TestLoadRegions、TestTrySwitchRegionStorage
及 memory/endpoint 调用链后，确认所选用例无需 failpoint 注入，仅写编译缓存。
实际运行：

```text
GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage -run '^(TestLoadRegionsToCache|TestLoadRegions|TestTrySwitchRegionStorage)$' -count=1 -v
```

退出 0，3 个顶层测试通过（`/tmp/pd56-go.log`）。整包指定 Go oracle 未运行：
TestRegionReload、TestLoadRegionsExceedRangeLimit 等依赖注入，不能在 Go 只读
条件下执行。未启停 failpoint，未把上述源码规则或选中用例冒称完整 Go 运行差分。
本任务没有编码实现变更；复用 endpoint 的既有 region codec、既有 Go fixtures
及任务 2 的真实 Go Region 字节证据；完整基线同时回归既有编码比较测试。

### 实现、失败时序与红绿灯

新增 `region_load_once.rs`，在 canonical Storage 上提供
`try_load_regions_once` / `are_regions_loaded` 与私有三态 RegionSource。
facade.rs 只增加状态字段和初始化，region_switch.rs 的 new_core 重置状态为
Unloaded（对应 Go 新 core）；切换与直接 load_regions 均不修改加载状态。
Go AreRegionsLoaded 对非 core 的类型断言会 panic，Rust 同样明确 core 前置条件。
无 manifest、Cargo.lock、依赖或 codec 变更。

独立 region_load_once_test.rs 在 lib.rs 显式注册，两个测试入口使用同一 contract。
region_load_once_memory 使用两个独立 MemoryStore；region_load_once_etcd 使用
两个真实 etcd fixture，默认和注入源的数据完全独立。复用已有 fixture helper，
随机 loopback 端口、独立目录、EOF/Drop 关闭等待；审查其 Go build 及启动路径
不生成或改写 Go/模块文件，继承 -mod=readonly。

六组路径隔离场景覆盖：初始未加载、本地成功后跳过新增区域及已取消调用、默认
重复增量扫描、两方向切换及来源查询、默认先成功抑制本地首次扫描、失败状态
保留、首次取消可重试、普通 facade 重复委托、空扫描成功、直接扫描不设置状态。
错误时序：本地先交付 ID=1，再遇到 ID=2 的坏 protobuf [255]；返回 Region 错误，
状态仍未加载、游标=2；修复真实存储中的 ID=2 后只回调 ID=2 并标记本地成功。
取消时序：调用前取消 token，真实读完成后 endpoint 返回 Cancelled，无 callback；
未加载状态保留，换用新 token 可成功。已有本地/默认成功后的默认取消保留原
来源。无网络断连、lease 或时钟到期场景，本故事无需这些故障。

最小可编译接口先直接委托扫描且状态恒未加载：聚焦命令退出 101，两个后端均在
成功交付本地区域后 `storage.are_regions_loaded()` 的真实状态断言失败，
0 passed / 2 failed / 0 ignored（`/tmp/pd56-red.log`）。实现状态迁移和跳过规则后，
同一测试未削弱断言、未忽略，退出 0，2 passed / 0 failed / 0 ignored
（`/tmp/pd56-green.log`）。

### Cargo 槽位与最终验证

所有 Cargo 阶段均由 `/tmp/pd56-cargo.sh` 从根目录按顺序原子 mkdir 动态领取，
实际为 **槽位 1**，绝对路径
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录 plan=storage-facade/task=3/batch=56/session=region-load-once/PID；
红灯 PID=19912、绿灯 PID=20118、最终基线 PID=20263。
设置 EXIT/INT/TERM 清理自己的 owner/锁；所有 Cargo 与 fixture 子进程退出后
释放锁，保留缓存。Rust 修改后先 cargo fmt --all，未改动其他任务 Rust 文件。

最终基线日志 `/tmp/pd56-baseline.log`，metadata `/tmp/pd56-metadata.json`。

| 确切命令 | 退出码与有效测试数 |
| --- | --- |
| `cargo fmt --all` | 0；分别在红灯和生产修改后先执行 |
| `cargo test --locked -p astersql-pd-storage --lib region_load_once -- --nocapture`（红灯） | 101；0 passed / 2 failed / 0 ignored |
| 同一聚焦命令（绿灯） | 0；2 passed / 0 failed / 0 ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；83 target，608 passed / 0 failed / 1 既有 ignored |
| 上述只读 Go 命令 | 0；3 个顶层测试通过 |
| `git diff --check`、`git diff --cached --check` | 均 0 |

全部 Cargo 与 fixture 进程退出，自己的 slot-1 owner/锁已释放，缓存保留。
开始/结束 1101 个 Go/模块文件路径与 SHA-256 完全一致，Go/模块 Git 状态为空。
已审查实现、独立测试、模块注册与本节证据；无其他 Rust、manifest、锁文件、
plan.md 或来源主题变更。保留用户原有观测工具计划修改及前序编号任务删除，
排除本次提交。状态：已完成；批次 56 的行为与验证证据满足。

## 任务 4：区域刷新关闭传播（批次 57）

日期：2026-10-23（Asia/Shanghai）。

### 准入与 Go 依据

核验批次 56 提交 `edcb7c7c3`、本文件任务 3 及 canonical Storage 的加载状态实物：
双后端 2 passed、workspace 608 passed / 0 failed / 1 既有 ignored，准入满足。
阅读执行技能、只读 plan.md、ADR-001、porting-guide、仓库及 Copilot 说明；无更深
AGENTS.md。storage.go、region_storage.go 相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。

Go `coreStorage.Flush` / `Close` 无条件转发给非 nil regionStorage，不读取
useRegionStorage，不关闭默认 Storage，不缓存关闭状态、不吞掉错误，也不额外先
Flush 再 Close。`RegionStorage.Flush` / `Close` 直接返回 backend 结果；
`endpoint.StorageEndpoint.Flush` / `Close` 为 nil 返回的空操作。
因此重复调用的成败由注入资源决定，门面不能自行实现幂等成功或错误重映射。

检查已有 TestMain、TestRegionStorage、TestTryGetLocalRegionStorage 及其 endpoint、
LevelDB、测试辅助调用链；选中用例无需 failpoint 注入，仅写临时 LevelDB 目录与
编译缓存。实际运行：

```text
GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage -run '^(TestRegionStorage|TestTryGetLocalRegionStorage)$' -count=1 -v
```

退出 0，2 个顶层测试通过（`/tmp/pd57-go.log`）。TestRegionStorage 包含真实批量
保存后 Flush 才能扫描到两条区域的断言及 Close 成功；此处只复用既有测试，不新增
Go oracle。整包 `go test -mod=readonly ./pkg/storage -count=1` 未运行，因其中
TestRegionReload、TestLoadRegionsExceedRangeLimit 等要求注入；不启停 failpoint，
不将选中测试与源码推导冒称整包或逐场景跨语言运行差分。本任务无 codec 变更，
不新增字节编码预期，既有 codec 比较由 workspace 基线回归。

### 实现与行为红绿灯

`region_lifecycle.rs` 原位扩展 canonical Storage，新增宿主 IO 边界
`RegionLifecycle`、保留原始类型的 `RegionLifecycleError` 和
`Storage::new_core_with_lifecycle` / `flush` / `close`。
同步生命周期方法与 Go 对齐，不创建 runtime、线程或后台任务；每次调用直接返回
宿主结果。普通 facade 和已有 new_core 的 StorageEndpoint 保持 Go 空操作语义。
facade.rs 仅添加可选 Arc 生命周期字段及初始化；region_switch.rs 在重新装配
new_core 时清除原附加资源的生命周期，防止新 endpoint 误用旧资源；lib.rs 注册
模块并重导出接口。无依赖、manifest、Cargo.lock 或 endpoint crate 变更。

独立 region_lifecycle_test.rs 显式注册，region_lifecycle_memory 与
region_lifecycle_etcd 共用 contract。内存入口使用两个独立 MemoryStore；etcd
入口使用两个真实外部 etcd。复用已有 fixture，审查 Go build/启动链不生成源码、
不写模块文件，继承 GOFLAGS=-mod=readonly；随机 loopback 端口、独立数据目录，
EOF/Drop 关闭并等待进程退出。

仅生命周期外部 IO 边界使用窄 FileLifecycle 实现：待刷字节经真实 File 写入，
Close 释放文件句柄，不模拟任何 MetaStore 操作或事务结果，也不以调用次数验收。
覆盖默认源选择时仍刷新附加资源、切换后刷新、重复刷新、不选本地时关闭附加
资源及尾部刷新、关闭后两种选择下重复 Flush/Close 的 BrokenPipe 原始 io::Error
类型及消息传播、默认 KV 和两个区域 endpoint 仍可用、普通 facade/普通 core 的
重复空操作、替换 endpoint 后清除旧生命周期。关闭成功后再调用发生文件已关闭
错误，门面继续转发，终态文件句柄为空、文件内容保持 default/local/close，未
清除区域源或加载状态。无网络断连、lease 或时钟到期场景。

最小可编译接口先返回成功但不委托：聚焦命令退出 101，两后端均在真实文件内容
断言失败，实际 []，期望 b"default"，0 passed / 2 failed / 0 ignored
（`/tmp/pd57-red.log`）。实现直接委托后，同一测试没有修改、削弱或忽略，
聚焦命令退出 0，2 passed / 0 failed / 0 ignored（`/tmp/pd57-green.log`）。

### Cargo 槽位与验证

每阶段 `/tmp/pd57-cargo.sh` 从根目录依次原子 mkdir 动态领取，实际均为
**槽位 1**，绝对路径
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录 plan=storage-facade/task=4/batch=57/session=region-lifecycle/PID；
红灯 PID=24942、绿灯 PID=25114、完整基线 PID=25248。设置 EXIT/INT/TERM 清理
自己的 owner/锁，等待全部 Cargo 与 fixture 子进程结束再释放，保留缓存。
Rust 修改后先 cargo fmt --all，未格式化其他任务 Rust 文件。

完整基线日志 `/tmp/pd57-baseline.log`，metadata `/tmp/pd57-metadata.json`。

| 确切命令 | 退出码与有效测试数 |
| --- | --- |
| `cargo fmt --all` | 0；红灯及生产修改后均先执行 |
| `cargo test --locked -p astersql-pd-storage --lib region_lifecycle -- --nocapture`（红灯） | 101；0 passed / 2 failed / 0 ignored |
| 同一聚焦命令（绿灯） | 0；2 passed / 0 failed / 0 ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；83 target，610 passed / 0 failed / 1 既有 ignored |
| 上述只读 Go 命令 | 0；2 个顶层测试通过 |
| `git diff --check`、`git diff --cached --check` | 均 0 |

全部 Cargo 与 fixture 子进程结束，自己的 slot-1 owner/锁已释放，缓存保留。
开始/结束 1101 个 Go/模块文件路径与 SHA-256 完全一致，Go/模块 Git 状态为空。
审查生产接口、最小字段/连线、独立测试及本节证据；无依赖、其他 Rust 文件、
只读 plan.md 或来源主题变更。保留用户既有观测工具计划修改与前序编号任务删除，
排除本次提交。状态：已完成；批次 57 的行为与验证证据满足。

## 任务 5：回归门面契约与依赖方向（批次 58）

日期：2026-10-23（Asia/Shanghai）。状态：已完成。

### 准入、验收边界与 Go 证据

核验批次 57 提交 `07147a8a6`、本文任务 4 的双后端结果和 workspace
610 passed / 0 failed / 1 既有 ignored，以及生命周期实现与测试实物，准入满足。
`storage.go`、`storage_test.go` 相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。
本任务是已有实现的独立回归关口，无生产变更，TDD 红灯不适用。

检查 TestMain、8 个相关既有 Storage 测试、相邻 helper 和 Memory/LevelDB 调用链，
以及 endpoint TestGCStateJSONUtil / etcd 测试装配：选中用例无需注入，仅写编译缓存、
临时 LevelDB/etcd 数据，不写 Go 或模块文件。实际运行：

```text
GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage -run '^(TestBasic|TestLoadStores|TestStoreWeight|TestTryGetLocalRegionStorage|TestLoadRegions|TestLoadRegionsToCache|TestTrySwitchRegionStorage|TestRegionStorage)$' -count=1 -v
GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage/endpoint -run '^TestGCStateJSONUtil$' -count=1 -v
```

均退出 0，分别 8 和 1 个顶层测试通过；日志 `/tmp/pd58-go.log`、
`/tmp/pd58-go-json.log`。覆盖元数据 CRUD、store 加载/权重、区域扫描、普通门面
重复加载、区域源选择/获取、Flush 后可见及 Close、JSON 字节断言。

条件命令 `go test -mod=readonly ./pkg/storage -count=1` 未执行，整包含
TestLoadRegionsExceedRangeLimit（withRangeLimit）、TestRegionReload（slowLoadRegion）、
TestSaveTimestampWithTimeout（slowTxn）等需源码注入的用例；不启停 failpoint，
不将筛选运行声称为整包通过。验收边界是门面映射及双后端回归，
本任务复用已有后端故障行为，不重新移植这些机制。

核查 `docs/rust/theme3-存储键值.md` 批次 19 回归关口：其同样记录整包条件未满足，
复用历史 `/tmp/pd-task9-go.log` 的 KV 4.186s / storage 28.756s 通过输出并完成
真实双后端回归。该历史日志明确包含恢复 failpoint 提示，但无 -v，不能补造计数，
也不当作本轮运行差分。检查 `/tmp/pd-meta25-{storage,arm64,arm64-debug}.test`
已有二进制；arm64 二进制 nm 检查无 failpoint.Eval 证据，未确认其注入版本/来源，
因此不拿它裸跑故障测试或充当故障路径证据。未生成任何 Go 工具或注入产物。

三个未运行 Go 故障场景逐项核验：

- 范围限制属于 endpoint/KV：本轮 workspace 实际执行 region_scan_memory /
  region_scan_etcd，对 10001 个区域跨真实 10000 页边界；etcd_adaptive_and_io_failure
  用 220 个 24KB start key 构造超过 4MiB 的真实响应并验证缩页恢复，未模拟核心结果。
  门面区域路由的双后端 contract 则验证 load_regions 委托正确源。
- 超时重载的门面相关契约是错误传播、加载状态保留及重试：region_load_once 两后端
  实际验证预先取消无回调、状态未加载、换 token 重试；坏区域导致中途错误/游标 2，
  修复后只交付区域 2。endpoint region_scan 两后端在首回调取消后仍交付当前页
  1..10000，游标 10001，重试仅补 10001；不宣称复现 Go 的 5 秒延迟注入。
- slowTxn 时间戳超时属于既有 endpoint/KV，不由 facade 另行实现：本轮 workspace
  实际命中 timestamp_fenced_save_memory_races / etcd_races 的取消错误与真实未写入
  终态，及 txn_cancel 的真实 etcd 代理故障测试。门面共用同一个 store，不复制
  timestamp 或事务算法；当前验收无需新增 Go 超时差分工具。

### 映射、字节与实际故障覆盖

现有独立测试全部由 lib.rs 显式注册；沿用 facade_memory / facade_etcd、
region_switch_memory / region_switch_etcd、region_load_once_memory /
region_load_once_etcd、region_lifecycle_memory / region_lifecycle_etcd，
不强造 facade_regression 名称或新模块。

Go Storage 的 kv.Base 映射到 Storage::base()；其余嵌入接口通过 endpoint() /
endpoint_mut() 访问 canonical StorageEndpoint：ServiceMiddlewareStorage→
config_endpoint；ConfigStorage→config_endpoint/scheduler_endpoint；MetaStorage→
cluster_meta/store_meta/store_weights 与 facade 区域路由；RuleStorage→rule_endpoint；
ReplicationStatusStorage→replication_endpoint；GCStateStorage→gc_* 模块；
MinResolvedTSStorage/ExternalTSStorage→timestamp_values；KeyspaceStorage→keyspace_meta；
ResourceGroupStorage→resource_settings/resource_states；TSOStorage→timestamp_window/
timestamp_fenced_save；KeyspaceGroupStorage→keyspace_group_store；
MetaServiceGroupStorage→meta_service_group；MaintenanceStorage→maintenance_*；
AffinityStorage→affinity_endpoint。配置持久化不等于完整业务 Config 移植；
本地生产数据库、加密、hot-region history、durability 不计入完成范围。

metadata 核对生产依赖 storage→endpoint→kv，无反向依赖；仅 kv 定义 MetaStore，
storage 重导出，facade 使用宿主 Arc；协议编码留在 endpoint 适配器。
复用既有 Go 字节证据：facade JSON int64(1)→hex 31；region_switch_test 从
pkg/proto/wire-fixtures.json 的 Go region_heartbeat 输出提取 Region
{Id:99,StartKey:[0,1],EndKey:[255]}，hex 0863120200011a01ff，与实际保存字节比较。
不新增 codec，不将 Rust 结果反抄为期望或宣称新的 Go 运行差分。

本轮加载故障测试实际覆盖：先交付区域 1，区域 2 坏 protobuf 导致错误且保留
未加载状态/游标 2；修复存储后仅交付区域 2 并完成加载。预先取消 token 后
扫描返回 Cancelled、无 callback，使用新 token 可重试。生命周期关闭文件后
再次 Flush/Close 返回原始 BrokenPipe，文件句柄为空，内容保持
`default/local/close`，默认 KV 和区域 endpoint 仍可用。
本门面故事未新增网络断连或 lease 到期测试，不声称 Go 故障场景已完成差分。

### 槽位与本轮验证

脚本 `/tmp/pd58-cargo.sh` 从仓库根按序以原子 mkdir 动态领取 **槽位 1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录 plan=storage-facade、task=5、batch=58、session=facade-regression、
PID=28105；EXIT/INT/TERM 清理自己的 owner/锁。
fixture 既有 Go build/启动链已审查，继承 GOARCH=arm64、CGO_ENABLED=0、
GOFLAGS=-mod=readonly；只写槽位产物及隔离数据目录。全部子进程结束后锁已释放，
缓存保留。无 Rust 修改，仅格式检查，无格式化写入。

| 确切命令 | 退出码与有效测试数 |
| --- | --- |
| `cargo test --locked -p astersql-pd-storage -- --nocapture` | 0；lib 17 + etcd_contract 10 = 27 passed / 0 failed / 0 ignored；doc 0 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0 |
| `cargo test --workspace --locked` | 0；83 target，610 passed / 0 failed / 1 既有 ignored |
| 上述筛选 Go storage / endpoint 命令 | 均 0；分别 8 / 1 个顶层测试 |
| `git diff --check`、`git diff --cached --check` | 均 0 |

Cargo 日志为 `/tmp/pd58-{focused,metadata,fmt,check,workspace}.log`，
汇总 `/tmp/pd58-results.log`，实际槽位 `/tmp/pd58-slot.log`。
开始/结束 1101 个 Go/模块路径及 SHA-256 完全一致，相关 Git 状态为空，无新增文件。
仅本节证据文档提交；任务状态更正为已完成并按执行流程删除任务文件。
未修改 Rust、Cargo、只读 plan.md 或来源主题；保留其他任务的计划修改及编号文件删除，
排除本次提交。完成范围限于已准入的存储门面，不宣称整个 Go 存储包或来源史诗完成。
