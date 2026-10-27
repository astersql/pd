# 任务146：标签领导者配置传输适配

依赖145的提交 `caf7b22d9`、[task-145](task-145.md) 及其最终聚焦/包级/workspace
验证日志已核实。当前 canonical LabelScheduler 已有真实过滤和 operator 管线，
本任务仅增加其局部 HTTP 入口与独立验收测试，不新增依赖、feature 或完整路由。

## 契约来源

按用户后续授权，以只读 Go 源码/测试推导及真实 Rust 行为/故障验证代替缺失的
Go 运行 oracle。本任务没有运行 Go 测试，不声称取得 Go/Rust 运行差分。
[provenance](logs/task_146/provenance.json) 固定来源 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、三份 Go 来源文件、oracle-index、
依赖145证据及[本任务 fixture](fixtures/task_146.json) 的 SHA256。
三份 Go 来源文件与该 commit 逐字节相同；fixture 标注 `runtime_go_oracle=false`。

`label.go` 的 `labelScheduler` 嵌入 `BaseScheduler`，没有专属 `ServeHTTP`，
`base_scheduler.go` 的继承实现只执行 `fmt.Fprintf(w, "not implements")`。
因此任意方法、相对路径与输入均返回200、纯文本 `not implements`，没有换行、JSON、
Allow 或错误码 header。Go net/http 对短文本的类型推导为
`text/plain; charset=utf-8`；本地适配器显式设置该类型。没有动态配置更新、请求解码、
持久化或对应的500失败响应。HEAD在这里验证直接handler输出，线上body抑制由宿主负责。

只读 `scheduler_test.go:TestRejectLeader` 针对reject-leader标签、断连/待定peer、限流
及真实leader转移，不覆盖HTTP状态/JSON/保存错误；未将其测试名称冒充HTTP运行证据。
当前oracle-index的运行用例不提供本任务的HTTP专属运行oracle。
本任务明确采用源码推导授权，不更改oracle-index含义。

## 实现与测试驱动

入口 [serve](../../../pkg/server/schedule_admin_label.rs) 使用 canonical LabelScheduler
的共享引用；[边界说明](../../../pkg/server/schedule_admin_label.rs.md) 描述继承契约。
不读取请求流，不获取StorageEndpoint，不依赖任何可用元数据后端；因此保存失败不可达，
不为不存在的保存分支构造假的500案例。真实MemoryStore中预存的配置在请求前后逐字节
比较，证明没有副作用。领域层不引入传输依赖。

[red日志](logs/task_146/red.json) 的相同聚焦命令实际运行3项测试，错误候选返回501，
3项均在期望200的行为断言失败，exit101。实现正确继承响应后同一组测试通过。
这项红测证明测试能识别错误行为，没有用缺包/编译失败或零测试代替行为失败。

[独立测试](../../../pkg/server/schedule_admin_label_test.rs) 显式注册在server/lib.rs：

- `task_146_behavior_cases`：19个固定方法/路径/输入案例，各在nil、空、非空range配置
  上运行，共57个有效组合。包括GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS/TRACE/CONNECT/
  自定义方法、list、未知路径、delete路径、查询参数、尾斜线、null、错误JSON和非法range。
  逐字节检查body、Content-Type、缺失Allow/错误码、非JSON响应及配置编码不变。
- `task_146_unread_body_faults`：POST/GET/DELETE/HEAD共4个边界检查，将外部请求流设为
  一旦poll就失败；入口仍成功响应且配置不变。证明读取失败或卡住的请求体无法影响此入口。
- `task_146_schedule_after_unsupported_update`：真实BasicCluster、ProviderSnapshot、
  RuleManager、StorageEndpoint/MemoryStore与LabelScheduler。标签匹配且范围命中、
  标签匹配但范围未命中、标签未配置三组场景各在POST前后运行，共6次真实决策。
  尝试用POST清空/扩大range均无效；命中场景产生region1、MEDIUM、LEADER、单步1→2，
  两种heartbeat命令均指向store2，confver变化0；其余场景无操作。
  检查内存编码与预存配置字节均不变，不只检查调用次数。

固定时钟、排序后选择第一个候选及不洗牌仅控制允许的时间/随机边界；没有忽略调度错误。
HTTP body无归一化；不主张验证完整server路由、HEAD线上编码或所有Go HTTP隐含头。

## 验证运行环境

[阶段脚本](logs/task_146/verify.py) 原子mkdir动态领取共享1–10槽位，记录任务/会话/PID，
设置绝对CARGO_TARGET_DIR，等待全部子进程退出后核对owner并释放，缓存保留。
[Go拦截器](logs/task_146/reuse-go/go) 拒绝所有实际Go命令；只把既有SHA256为
`57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5`
的etcd I/O工具硬链接至Rust测试请求位置，必要时只读返回已缓存且固定哈希的kvproto归档。
[复用记录](logs/task_146/go-reuse.jsonl) 保留实际请求与来源/目标哈希。
没有构建Go、注入failpoint、创建新Go oracle或更改Go模块。

## 最终结果：已完成

[完整命令/退出码/原始日志](logs/task_146/final.json) 与
[有效测试数汇总](logs/task_146/final-summary.json)：

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 检查通过 |
| `cargo test -p astersql-pd-server --locked --lib task_146 -- --nocapture` | 0 | 3通过、0失败、0忽略 |
| `cargo test -p astersql-pd-server --locked` | 0 | 89通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 检查通过 |
| `cargo fmt --all -- --check` | 0 | 检查通过 |
| `cargo check --workspace --locked` | 0 | 检查通过 |
| `cargo test --workspace --locked` | 0 | 1889通过、0失败、1忽略 |

聚焦3项测试覆盖57个HTTP组合、4个不可读取请求流检查和6次真实调度决策。
workspace已有1项忽略保持原状；本任务无忽略、零测试或跳过必需检查。

所有阶段实际动态领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；
阶段子进程全部结束，自己的锁已释放，保留编译缓存。
CARGO_BUILD_JOBS=4、RUST_TEST_THREADS=2，默认features。

[审计](logs/task_146/audit.json) 比较[开始快照](logs/task_146/baseline.json)：
1101份Go/模块文件集合、内容哈希和Git状态完全不变；两个只读plan不变，
Markdown链接有效，`git diff --check`退出0。
Cargo.toml/Cargo.lock无差异，局部注册仅server/lib.rs。
仅提交本任务入口、测试与证据，其它任务已有删除均排除；验收后删除146编号任务，
不改写来源主题或任一级plan。
