# 任务145：领导者授予配置传输适配

依赖144的提交 `87d3fd46e` 与 [task-144](task-144.md) 已核实，包括通过的
聚焦、包级与workspace验证。复用真实 GrantLeaderScheduler 配置事务、BasicCluster、
StorageEndpoint/MemoryStore 与 operator 构建，无新增领域副本、feature 或全路由装配。

## 契约与准入

用户后续明确授权以 **只读 Go 源码/测试推导 + Rust 真实行为和故障验证** 替代缺失
Go 运行 oracle；本任务没有运行 Go 测试，也不声称取得 Go/Rust 运行差分。
[provenance](logs/task_145/provenance.json) 固定来源 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、Go源码、oracle-index及fixture哈希；
所采用4份Go源码与基准commit逐字节相同。
[固定fixture](fixtures/task_145.json) 标注 `runtime_go_oracle=false`。
模式为默认 Cargo features 的局部 scheduler 接口，不新增classic/nextgen开关。

`grant_leader.go:ServeHTTP` 委托专属handler覆盖BaseScheduler的不支持响应。
POST `/config`、GET `/list`、DELETE `/delete/{store_id}` 分别有专用方法；
405空体无Allow、未知路径404文本。配置缺失/类型错误400 JSON字符串；读取/解码
错误使用apiutil结构体。POST store_id按float64截断，ranges的[]string断言对
JSON数组不成立，故忽略请求ranges并重新解码已有范围；新store使用全范围。
源码推导不以改进API的理由修正这些可观察行为。

更新保存失败500，移除该store内存配置并释放OUT pause，保留旧磁盘值；
删除保存失败500保持内存，最终删除回调普通错误恢复内存和OUT pause，但磁盘
已写为空；NotFound响应404且不恢复。正常删除非最后store返回null，最后返回消息。
`TestGrantLeaderScheduler`证明已有Go测试针对真实授予与OUT pause、抑制balance行为，
并没有覆盖HTTP方法/JSON/存储故障；本任务不把该名称冒充HTTP运行oracle。

## 实现与验证设计

入口 [serve](../../../pkg/server/schedule_admin_grant_leader.rs)，
[边界说明](../../../pkg/server/schedule_admin_grant_leader.rs.md)，
[独立测试](../../../pkg/server/schedule_admin_grant_leader_test.rs)。
core-constant从dev依赖提升为生产依赖，用于缺失ID路径释放store0 OUT pause。
JSON仅归一化键顺序、缩进/尾换行；错误、字段和命令不被忽略。
读流错误的stack仅检查非空，堆栈内容不可确定；I/O错误来自真实MetaStore边界注入，
错误文本是Rust后端契约，不作为Go后端字节oracle。

- `task_145_behavior_cases`：38个固定HTTP案例；状态、全部JSON字段、Content-Type、
  Allow缺失与错误后配置保持。包括分数ID、重复键、忽略ranges、非法参数、解码错误、
  store不存在、删除解析与最终清空配置。
- `task_145_transactions_and_faults`：8组检查：新/已有store保存故障、reload成功/失败、
  删除保存故障、最终删除回调恢复、NotFound保留删除结果、已存ASCII/非ASCII范围重新解码错误。
  检查真实内存、磁盘、OUT pause与错误；保存故障后从真实磁盘重建。
- `task_145_updated_scheduling_cases`：3组HTTP更新及持久化重建，各运行真实调度，
  合计6次决策。新store的ranges被忽略，目标2/3产生region1、HIGH、LEADER、1→目标
  的单步操作；目标1已是leader，无操作。均比较持久化字节与重建配置。
- `task_145_body_read_and_clone`：完整/截断JSON后的真实流错误，以及空range克隆为null
  且不改变领域状态，共3组。

不主张涵盖Go负值/超范围float到uint64的架构相关行为、所有JSON词法错误或mux路径
canonicalization。固定案例与普通参数/故障范围均按源码逐项核实。

## 测试驱动记录

[red](logs/task_145/red.json) 初次发现生产依赖尚为dev-only，exit101，不能算行为红测。
[red2](logs/task_145/red2.json) 注册的真实测试实际运行1项，对初始错误状态501断言
期望200失败，exit101，证明相同测试可识别对应HTTP行为错误；实现完成后通过同一测试。
首轮green和green2分别识别了复制fixture带入的batch字段、误写的MetaStore故障错误
期望及缺失ID响应类型，按各自来源修正；它们不是Go运行差分。

[阶段脚本](logs/task_145/verify.py) 每阶段通过原子mkdir依次动态领取1–10槽位，
记录任务/会话/PID，设置绝对CARGO_TARGET_DIR，等全部子进程退出后核对owner再释放。
[Go拦截器](logs/task_145/reuse-go/go) 禁止实际Go命令，只将已存在且SHA256固定为
`57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5`
的etcd I/O二进制硬链接到测试请求路径；必要时只读返回固定kvproto缓存归档。
这是复用前置I/O工具，没有构建Go、生成Go、注入failpoint或产生新的Go oracle。

[unicode-red](logs/task_145/unicode-red.json) 在扩展后的故障测试中实际运行1项，
原偏移处理对 `é%xx` 产生切片越界，exit101；修复为从原始字节定位非法转义，
避免将领域层扩展输入的偏移直接应用于原字节。此前完整回归记录保留为
[pre-unicode](logs/task_145/pre-unicode.json)，不作为最终代码验证。

## 最终结果：已完成

[确切命令、退出码、槽位与原始stdout/stderr合并日志](logs/task_145/final.json)，
[有效测试数汇总](logs/task_145/final-summary.json)：

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 检查通过 |
| `cargo test -p astersql-pd-server --locked --lib task_145 -- --nocapture` | 0 | 4通过，0失败，0忽略 |
| `cargo test -p astersql-pd-server --locked` | 0 | 86通过，0失败，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 检查通过 |
| `cargo fmt --all -- --check` | 0 | 检查通过 |
| `cargo check --workspace --locked` | 0 | 检查通过 |
| `cargo test --workspace --locked` | 0 | 1886通过，0失败，1忽略 |

全部最终Rust检查在代码最后修改后执行。聚焦4项测试覆盖38个HTTP案例、8组故障检查
（范围错误含ASCII及非ASCII两例）、6次真实调度决策和3组读流/克隆检查。
workspace原有1项忽略保持原状，不以忽略本任务测试或零测试代替验收。

所有阶段实际动态领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；
子进程均已退出，自己的槽位锁已释放，缓存保留。CARGO_BUILD_JOBS=4、RUST_TEST_THREADS=2。
[审计](logs/task_145/audit.json) 核对1101份Go/模块文件集合、内容哈希和Git状态不变，
两个只读plan不变，Markdown链接有效、git diff --check退出0。
根Cargo.toml/Cargo.lock没有差异，只有server依赖位置调整及局部注册。
仅提交本任务实现、独立测试、配置和证据；其它任务既有删除排除提交。
任务验收后标记已完成并删除145编号任务，不改写原主题或任一级plan。
