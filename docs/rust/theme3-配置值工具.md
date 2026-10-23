# 配置值工具验证证据

## 任务 1：默认值调整（批次 80）

日期：2026-10-24（Asia/Shanghai）。前置批次 79 提交 `4921f503b` 为本轮起点，
已核验 [前置证据](theme3-元数据观察工具.md) 的任务 6：双后端红绿灯、
655 passed / 0 failed / 1 既有 ignored，以及 ttl_put 实现和显式测试注册。
该提交为当前 HEAD 祖先，批次 80 准入满足；未以编号任务文件删除代替完成证据。
仅根 AGENTS.md 适用。已读 ADR-001、移植指南、只读计划及任务的槽位规则。

### 行为、Go 依据及边界

新增 canonical `astersql-pd-configutil`，实现于
[config_adjust.rs](../../pkg/utils/configutil/config_adjust.rs)，
[独立测试](../../pkg/utils/configutil/config_adjust_test.rs) 由
[lib.rs](../../pkg/utils/configutil/lib.rs) 显式注册。
复用 canonical typeutil 的有符号纳秒 Duration 与 ByteSize，没有平行类型、codec、
存储、runtime、文件读取或外部 IO。新增 crate 只依赖已有 typeutil；
Cargo.lock 只新增本地 configutil package，根 manifest 只新增 workspace member。

只读 [configutil.go](../../pkg/utils/configutil/configutil.go) 相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。
以下规则来自本轮源码审查，不冒充 Go/Rust 运行差分：

| Go 符号 / 行号 | Rust 符号与验证场景 |
| --- | --- |
| AdjustString / 126 | adjust_string：空串替换；空默认原样赋值；空白及 Unicode 保留 |
| AdjustUint64 / 133 | adjust_uint64：0 替换；1、u64::MAX 保留 |
| AdjustInt64 / 140 | adjust_int64：仅 0 替换，包括负默认；负数、正数及 MIN/MAX 保留 |
| AdjustInt / 147 | adjust_int：宿主位宽 isize；0 替换、负默认；负数、正数及 MIN/MAX 保留 |
| AdjustFloat64 / 154 | adjust_float64：正负零替换；正负非零、正负无穷及 NaN payload 位模式保留；负零默认原样赋值 |
| AdjustDuration / 161 | adjust_duration：MIN、负数、零均替换；正数及 MAX 保留；负/零/正默认不再校验 |
| AdjustByteSize / 168 | adjust_byte_size：0 替换；1、u64::MAX 保留 |
| AdjustBool / 183 | adjust_bool：false 取默认，true 保留；全部四种组合 |

函数只处理值，不推断字段是否存在。字段存在性/未识别项属于后续任务。
AdjustPath 依赖进程工作目录，命令行 Adjust 依赖 flagSet；均不在本任务纯值默认范围。
没有新增编码，不需要新增 Go 字节 fixtures，也未编写或改动任何 Go oracle。

已有 configutil_test.go 仅有 TestMain(goleak) 和 TestPrintConfigCheckMsg（2 个子场景），
无需 failpoint 注入。经只读源码审查后，在拒绝写 Go/模块文件的沙箱中执行
`go test -mod=readonly ./pkg/utils/configutil -count=1`，退出 0（包耗时 0.915s）。
这证明相邻既有打印测试通过，不声称 Go 默认调整函数获得运行覆盖。
未调用 make、failpoint、生成器、Go 格式化或依赖整理。

### 测试驱动证据

先建立可编译的最小函数入口及全部原始断言，再运行聚焦命令：
退出 101，**0 passed / 5 failed / 0 ignored**。分别实际观察到：
空串未改为 fallback、整数 0 未改为 7、浮点 0 位未改为 2.5、
Duration::MIN 未改为 -5、false 未改为 true；均为真实值断言失败，非缺 crate/符号。

按 Go 条件补全最小实现后，同一命令和同一组断言退出 0，
**5 passed / 0 failed / 0 ignored**，覆盖全部上述场景，未削弱或忽略测试。
本任务为纯值函数，因此不强造 MemoryStore/etcd 入口。

### 验证命令与资源

每阶段原子 mkdir 从共享槽位 1–10 动态领取，本轮实际均为 **槽位 1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁内记录计划、任务、会话和持有 shell PID；EXIT/INT/TERM 清理自己的 owner/锁，
Cargo 及 fixture 子进程全部结束后才释放，保留缓存。所有 Cargo 子进程继承
绝对 target、`GOFLAGS=-mod=readonly` 和 `/tmp/pd-b75-go-readonly.sb`：
拒绝写 Go/模块文件，并拒绝执行会解压 Go 源码的 proto build script，复用既有生成缓存。
fixture 复用现有只读 Go 工具，未创建 Go 源码。

| 命令（仓库根目录，继承上述环境） | 退出码 / 结果 |
| --- | --- |
| `cargo fmt --all` | 0，每次 Rust 修改后先执行，没有格式化其他在途源码 |
| `cargo check --workspace`（新增 crate 后同步 lock） | 0，lock 仅新增本地 package |
| `cargo test --locked -p astersql-pd-configutil --lib config_adjust -- --nocapture`（红灯） | 101，0 passed / 5 failed / 0 ignored |
| 同一聚焦命令（绿灯） | 0，5 passed / 0 failed / 0 ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0，仅既有 server cpu_usage deprecated warning |
| `cargo test --workspace --locked` | 0，89 个测试/文档 target 合计 660 passed / 0 failed / 1 既有 ignored |
| `git diff --check` | 0 |

5 个 Markdown 相对链接均有效。已审查本任务生产/测试、显式注册、manifest、lock
和本文差异；其他任务既有编号文件删除及配置盘点修改保持原样、不纳入提交。
全部 Cargo/fixture 子进程已结束，自己的槽位锁已释放，缓存保留。
开始/结束 Go/模块路径集合与 SHA-256 共 2816 项完全一致（含既有 target 中的
Go 源码），相关 Git 状态为空。只读 plan.md 和来源主题未修改。

状态：**已完成**。仅完成八个纯值默认调整函数，未宣称整个 configutil 包、
路径 IO、命令行、安全配置、字段存在性或完整 Config 已迁移。
