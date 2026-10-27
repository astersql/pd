# 任务147：随机合并配置传输适配

依赖146提交 `aa5a49f25`、[task-146](task-146.md) 与最终验证记录已核实；
canonical `RandomMergeScheduler`、范围配置、真实合并工厂和server依赖均已存在。
不新增crate、feature、依赖或完整server路由。

## 契约与准入

用户后续授权允许只读 Go 源码/测试推导加真实 Rust 行为/故障验证替代缺失的
Go运行oracle。本任务未执行Go测试，不声称取得Go/Rust运行差分。
[provenance](logs/task_147/provenance.json) 记录来源commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、三份Go源码/测试、oracle-index、
依赖证据与[fixture](fixtures/task_147.json) SHA256；三份Go文件与来源commit逐字节相同。
fixture标记 `runtime_go_oracle=false`。oracle-index没有本任务HTTP专属运行证据，
其含义保持不变。

`random_merge.go` 中 scheduler 嵌入 BaseScheduler，没有专属ServeHTTP，
`base_scheduler.go` 的继承方法只写入 `not implements`：任意方法、路径、请求均返回200，
纯文本无换行，没有JSON、Allow或错误码header。短文本对应Go net/http的类型推导为
`text/plain; charset=utf-8`；本地显式设置该类型。HEAD在直接handler边界检查body，
线上body抑制由宿主负责。随机合并不支持动态配置；非法输入不解码，
没有持久化路径，因而没有可注入的保存失败/500分支。

只读 `TestRandomMergeSchedule` 覆盖副本不完整时无操作、配置副本数后双操作及merge限流，
在placement rules开/关运行；不覆盖HTTP、JSON或持久化失败，不将测试名冒充运行证据。
调度源码确认邻接目标、LOW优先级、MERGE kind以及范围仅限制源区域。
本任务聚焦配置入口；更广调度错误/故障由既有task098负责。

## 实现与测试驱动

[生产入口](../../../pkg/server/schedule_admin_random_merge.rs) 为canonical scheduler提供
`serve(&RandomMergeScheduler, Request<Body>) -> Response`；
[边界说明](../../../pkg/server/schedule_admin_random_merge.rs.md) 描述领域/传输分工。
不获取存储，不读取请求流，不修改配置。没有复制scheduler业务。

[红测](logs/task_147/red.json) 同一聚焦命令实际运行3项测试，错误候选返回501，
3项均因期望200的具体断言失败，exit101；随后修正为200，同一测试通过。
不是缺API/编译失败或零测试。

[独立测试](../../../pkg/server/schedule_admin_random_merge_test.rs) 在server/lib.rs显式注册：

- `task_147_behavior_cases`：19个固定方法/路径/输入，分别在null、空、非空ranges上运行，
  共57个组合；覆盖GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS/TRACE/CONNECT/自定义方法、
  list/unknown/delete路径、查询参数、尾斜线、null、错误JSON和非法range。
  检查精确body/Content-Type、缺失Allow/错误码、非JSON以及配置编码不变。
- `task_147_unread_body_faults`：POST/GET/DELETE/HEAD共4个请求流故障边界，
  body一旦被poll即panic；入口仍返回正确响应，编码不变。
- `task_147_schedule_after_unsupported_update`：真实BasicCluster、ProviderSnapshot、
  RuleManager、StatisticsSnapshot、StorageEndpoint/MemoryStore、合并工厂。
  正向邻接、反向邻接、范围未命中3种场景，在POST前/后分别对v1/v2运行，共12次决策。
  POST试图清空或扩大范围均无效；命中生成源region2到3或1的active/passive双操作，
  LOW、MERGE、各单步、confver变化0，active heartbeat目标精确匹配，passive无命令；
  未命中无操作。真实预存配置字节与内存编码前后不变。

仅控制允许的时钟、候选选择和洗牌边界；HTTP响应无归一化，没有忽略操作差异。
不存在配置保存分支，因此不伪造持久化失败案例；MemoryStore用于检查真实持久化状态不变。

## 运行环境

[验证脚本](logs/task_147/verify.py) 原子mkdir动态领取1–10共享槽位，记录任务/会话/PID，
设置绝对CARGO_TARGET_DIR；退出/失败/中断均等待子进程结束再核验owner并释放，缓存保留。
使用默认features、CARGO_BUILD_JOBS=4、RUST_TEST_THREADS=2。
[Go拦截器](logs/task_147/reuse-go/go) 拒绝实际Go执行，仅将既有SHA256为
`57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5`
的etcd I/O二进制硬链接到测试要求的位置；协议构建如需缓存ZIP，只读核对固定哈希。
[复用记录](logs/task_147/go-reuse.jsonl) 保存实际请求与路径/哈希，没有新建Go oracle、
Go构建或failpoint注入。

## 最终结果：已完成

[完整命令、退出码和原始日志](logs/task_147/final.json)，
[有效测试汇总](logs/task_147/final-summary.json)：

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化成功 |
| `cargo test -p astersql-pd-server --locked --lib task_147 -- --nocapture` | 0 | 3通过、0失败、0忽略 |
| `cargo test -p astersql-pd-server --locked` | 0 | 92通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 检查通过 |
| `cargo fmt --all -- --check` | 0 | 检查通过 |
| `cargo check --workspace --locked` | 0 | 检查通过 |
| `cargo test --workspace --locked` | 0 | 1892通过、0失败、1忽略 |

聚焦覆盖57个HTTP组合、4个请求流故障和12次真实调度决策。
workspace原有1项忽略未更改，本任务没有忽略或跳过必需检查。
两阶段实际均动态领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
全部阶段及其子进程结束后自己的owner和锁已清理，编译缓存保留。

[最终审计](logs/task_147/audit.json) 对比[开始快照](logs/task_147/baseline.json)：
1101份Go/模块文件集合、内容SHA256和Git状态不变；两个只读plan不变；
Markdown链接有效；`git diff --check`退出0。Cargo.toml/Cargo.lock无差异，
共享入口仅server/lib.rs的本任务模块/测试注册。已有其它任务删除排除在本次提交外。
验收后删除147编号任务，不修改来源主题或任一级plan。
