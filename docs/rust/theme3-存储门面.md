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
