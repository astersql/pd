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

## 任务 2：字段存在性元数据（批次 81）

日期：2026-10-24（Asia/Shanghai）。已核验前置批次 80 提交 `86e383fba`
及本文任务 1 的红绿灯、660 passed / 0 failed / 1 既有 ignored 证据；
该提交为本轮 HEAD，canonical configutil crate、默认调整函数及测试注册均存在，
准入满足。只读计划与来源主题保持原样，仅根 AGENTS.md 适用。

### 行为与 Go 依据

在 canonical crate 新增 [config_metadata.rs](../../pkg/utils/configutil/config_metadata.rs)
的 `ConfigMetaData::{from_defined_paths,is_defined,child}`，
[独立测试](../../pkg/utils/configutil/config_metadata_test.rs) 在
[lib.rs](../../pkg/utils/configutil/lib.rs) 显式注册。
无新增依赖、Cargo target 或锁文件变更。调用方提供通过配置 table 可达的字段路径，
元数据拥有路径及其 table 前缀；值不参与存在性判断。不解析 TOML、不读文件、
不实现未知字段诊断；未来 parser adapter 必须排除穿过 scalar/array 值的路径，
不能直接把所有 TOML Keys 当成可遍历路径。空 table 须显式提供自身路径。
Arc 共享不可变索引，每个 Child 拥有独立路径，父视图销毁后子视图继续有效。

只读 configutil.go 相对规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223`
无差异。以下为源码推导，**不是 Go/Rust 运行差分**：

| Go 来源 | Rust 验证 |
| --- | --- |
| configutil.go:42–49 `IsDefined`：nil 返回 false，追加单个 key 查询 | Default、空输入、缺失项均 false；false/0/空字符串字段存在时保留其值，缺失字段应用默认值 |
| configutil.go:53–60 `Child`：复制原路径，追加全部 segments，共享 metadata | 多层与链式 Child 相同；父、子、兄弟视图不互相污染；空 Child 等价；子视图独立存活 |
| go.mod 锁定 BurntSushi/toml v1.5.0，模块缓存 meta.go:28–52 `IsDefined` | table 前缀存在；缺失路径、scalar 下级不存在；segment 大小写敏感、点是字面字符；空字符串 key 和 Unicode key 可查询 |

相邻 Go configutil_test.go 只有 TestMain(goleak) 与 TestPrintConfigCheckMsg，
不含 failpoint 注入。复用批次 80 的只读沙箱配置，执行
`sandbox-exec -f /tmp/pd-b75-go-readonly.sb env GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/utils/configutil -count=1`，
退出 0（0.956s；1 个 Test 函数、2 个子场景）。这仅证明既有打印测试通过，
不声称 Go metadata 获得运行覆盖。本任务不涉及 codec，无需 Go 编码输出。

### 测试驱动与验证

最小可编译入口先返回空元数据，保留全部真实断言，聚焦命令实际退出 101：
**1 passed / 4 failed / 0 ignored**。实际失败为显式 false 被默认值覆盖、
根 table 缺失、子路径 value 缺失、字面点 key 缺失，不是符号/环境失败。
补齐路径索引后，同一命令、相同五个测试与断言退出 0：
**5 passed / 0 failed / 0 ignored**。纯配置存在性无需 MemoryStore/etcd 专用入口。

所有 Cargo 命令均从仓库根执行，通过原子 mkdir 动态领取 **槽位 1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁记录计划、任务、会话和持有 shell PID；EXIT/INT/TERM 清理自己的锁，保留缓存。
Cargo 和 fixture 继承 `GOFLAGS=-mod=readonly` 与上述沙箱：拒绝写 Go/模块文件，
拒绝运行会重新解压 Go 源码的 proto build script，复用既有缓存。
workspace 的 etcd fixture 仅构建现有只读 Go helper，没有启停 failpoint。

| 命令（仓库根目录，继承上述环境） | 退出码 / 结果 |
| --- | --- |
| `cargo fmt --all` | 0，两次 Rust 修改后分别执行；未修改其他在途源码 |
| `cargo test --locked -p astersql-pd-configutil --lib config_metadata -- --nocapture`（红灯） | 101，1 passed / 4 failed / 0 ignored |
| 同一聚焦命令（绿灯） | 0，5 passed / 0 failed / 0 ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0，仅既有 server cpu_usage deprecated warning |
| `cargo test --workspace --locked` | 0，89 个测试/文档 target 合计 665 passed / 0 failed / 1 既有 ignored |
| `git diff --check` | 0 |

本任务新增的三个 Markdown 相对链接均指向实际文件。已审查实现、五个独立测试、
显式注册及证据文档；其他任务既有变更不纳入提交。
Cargo 与全部 fixture 子进程已结束，自己的槽位锁已释放，缓存保留。
开始/结束 Go 与模块路径及 SHA-256 共 2816 项完全一致（含既有 target 中的 Go
源码），相关 Git 状态为空；未执行 make、failpoint、Go 生成/格式化/依赖整理。
plan.md 与来源主题未修改。

状态：**已完成**。仅完成根/嵌套字段存在性与独立子路径视图，不宣称完成 TOML
解析、未知项诊断或完整 Config 迁移。

## 任务 3：未知配置项诊断（批次 82）

日期：2026-10-24（Asia/Shanghai）。已核验前置批次 81 的提交 `335000abd`
为当前 HEAD，本文任务 2 的 5 项红绿灯、665 passed / 0 failed / 1 既有 ignored
证据及 canonical metadata 实现、测试注册均存在，准入满足。仅根 AGENTS.md 适用。
只读计划及来源主题保持原样。

### 行为与 Go 依据

在 [config_unknown.rs](../../pkg/utils/configutil/config_unknown.rs) 增加
`ConfigMetaData::{with_undecoded_keys,check_undecoded}`，
[独立测试](../../pkg/utils/configutil/config_unknown_test.rs) 在
[lib.rs](../../pkg/utils/configutil/lib.rs) 显式注册。
已有 metadata 仅增加 Arc 共享的有序未知项快照，并在构造和 Child 中最小连线；
无新增依赖、crate、Cargo target 或锁文件变更。

解析器适配层提供已经格式化的字段路径（Go TOML Key.String 显示形式）；
本纯值工具逐字保留路径，不从字段存在性猜测未知项，不新增 TOML parser 或 key
codec。带引号的点、空 key、Unicode 和转义文本只是透传输入，不宣称完成其编码。
`with_undecoded_keys` 消费当前视图，替换该快照；其他 clone/child 保持原快照。
`check_undecoded` 返回 Option<String> 供调用者收集警告，不安装日志或中止配置流程。

只读 configutil.go 相对规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223`
无差异。下列为源码推导，**不是 Go/Rust 运行差分**：

| Go 来源 | Rust 覆盖 |
| --- | --- |
| configutil.go:64–77 `CheckUndecoded`：nil/空返回 nil，按列表用逗号空格连接 | 无 meta、已知字段且无未知项、清空已有未知项；精确警告前缀与分隔符 |
| 同函数不引用 m.path、不排序或去重 | Child 仍报告全局未知项；嵌套路径、非字典顺序、重复项、重复查询及父子独立存活 |
| BurntSushi/toml v1.5.0 meta.go:82–91 `Undecoded` 按原文顺序枚举 | 保留输入顺序；parser 已格式化的特殊路径原样保留 |
| server/config/config.go:360–362 将诊断追加 WarningMsgs | 警告可收集；字段存在性、clone/Child 在诊断后仍可使用 |

经审查 configutil Go 测试只有 TestMain(goleak)、TestPrintConfigCheckMsg，
没有 failpoint 注入。执行
`sandbox-exec -f /tmp/pd-b75-go-readonly.sb env GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/utils/configutil -count=1`，
退出 0（0.935s；1 个 Test 函数、2 个子场景）。这只证明既有打印测试通过，
不声称 Go CheckUndecoded 已获运行覆盖。不涉及新 codec，无需新增 Go 字节输出。

### 测试驱动与验证

最小可编译入口先返回 None，保留原始五项真实断言；聚焦红灯退出 101：
**1 passed / 4 failed / 0 ignored**。实际失败为嵌套/顺序、特殊路径、Child 全局
诊断返回 None，以及警告收集为空；并非缺符号或环境失败。
实现有序快照与警告拼接后，同一命令、同一测试与断言退出 0：
**5 passed / 0 failed / 0 ignored**。纯配置值无需 MemoryStore/etcd 专用入口。

每阶段从 1–10 原子 mkdir 动态领取共享 **槽位 1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁记录计划、任务、会话与持有 shell PID；EXIT/INT/TERM 清理自己的占用。
Cargo 和 fixture 继承 GOFLAGS=-mod=readonly 与已有只读沙箱，禁止写 Go/模块文件
及运行会解压 Go 源码的 proto build script，复用既有缓存。fixture 只构建现有
只读 Go helper，没有运行 make、启停 failpoint、生成或格式化 Go、依赖整理。

| 命令（仓库根目录，继承上述环境） | 退出码 / 结果 |
| --- | --- |
| `cargo fmt --all` | 0，两次 Rust 修改后分别执行，未修改其他任务源码 |
| `cargo test --locked -p astersql-pd-configutil --lib config_unknown -- --nocapture`（红灯） | 101，1 passed / 4 failed / 0 ignored |
| 同一聚焦命令（绿灯） | 0，5 passed / 0 failed / 0 ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0，仅既有 server cpu_usage deprecated warning |
| `cargo test --workspace --locked` | 0，89 个测试/文档 target 合计 670 passed / 0 failed / 1 既有 ignored |
| `git diff --check` | 0 |

已审查本任务生产实现、五项测试、metadata 最小连线、显式注册及本文差异；
新增三个 Markdown 相对链接均指向实际文件。开始/结束 Go 与模块路径及 SHA-256
共 2816 项完全一致，相关 Git 状态为空。全部 Cargo/fixture 子进程结束，自己的
槽位锁已释放，缓存保留。其他任务已有删除/修改保持原样、不纳入提交；只读
plan.md 与来源主题无变更。

状态：**已完成**。仅完成有序未知字段诊断与警告语义，不宣称 TOML 解析、
字段路径编码或完整 Config 已迁移。
