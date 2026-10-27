# 任务126：批量拆分管理响应

依赖125的提交 `990b3a0ce` 及[证据](task-125.md)已核实，聚焦5项、包级40项和
workspace1812项通过；canonical Handler、Coordinator::get_region_splitter、SplitRetry、
真实准入控制器及checker pending owner均可复用。本任务不修改两级plan.md或Go/模块文件。

用户明确授权以只读Go源码/测试推导与Rust真实行为/故障验证替代缺失的Go运行oracle。
`oracle-index.json` 中本任务入口为 `source_assertion`，无已冻结运行结果。本任务未运行
Go测试、二进制或failpoint命令，未声称Go运行差分。[provenance](logs/task_126/provenance.json)
记录来源commit `836fdc7cf3ef34f7eabc9d051418995bb97efabe`、来源哈希、fixture哈希及
Go二进制/运行输出为null。handler.go、region_splitter.go及两份只读测试与来源字节一致。

生产路径是[split_admin.rs](../../../pkg/schedule/handler/split_admin.rs)，提供
`Handler::split_regions`、`Handler::start_split_attempt`、`SplitRegions::advance/response`、
`SplitRegionsResponse`和`SplitAdminError`。[领域说明](../../../pkg/schedule/handler/split_admin.rs.md)
说明宿主驱动方式：入口先bootstrap、完整解码后返回拥有状态的请求；宿主使用真实
coordinator开始尝试、派发准入结果并提供当前region快照/时钟/取消标志；仅Done产生
最终响应。重试、观察、退避和pending转发全部复用canonical实现，无后台任务/私有runtime。

[fixture](fixtures/task_126.json)基于只读Go公式与控制流冻结9行输入/响应，前8行在实现前
编写，第9行随后补充二进制键覆盖；全部期望独立于待测Rust输出。Go空keys提前返回nil，
序列化为regions-id:null；非空未观察到结果返回[]。ID集合仅规范化Go map的顺序，
不忽略任何ID、错误、步骤、百分比或状态差异。

| 测试 | 有效案例与观察 |
| --- | --- |
| task_126_behavior_cases | 9行：空输入、负retry、大小写hex完整观察、2/3观察=67%、无观察、重复键、空hex键、既有边界、含NUL二进制键；比较完整JSON形状、USEKEY、原始范围/epoch/decoded keys、完成前无响应 |
| task_126_hex_and_bootstrap_errors | 6行：奇数长度、非法字符、非法字符优先于奇数长度；3种未bootstrap输入；完整解码失败前无operator副作用 |
| task_126_retry_remaining_keys | 1行：m/t/t首轮部分观察，100ms退避前不能重试，第二轮只发t/t，观察后100%且ID跨尝试去重，验证真实controller Arc身份 |
| task_126_cancellation_keeps_observed_progress_and_retries | 2行：首tick前取消与部分观察后取消；保留0%/50%，取消仍允许下一次准入、不扫描新结果，最终200ms退避前不返回响应 |
| task_126_admission_fault_and_replication_suspect | 2行：已有operator触发真实AlreadyExist准入拒绝，保留原Arc且不把可见新region算成进度；副本不足转发pending ID1并返回0% |

共20个顶层表驱动/错误/故障案例（9+6+1+2+2），5项测试。装配真实BasicCluster、
RegionInfo、Coordinator、ControllerAdmission、RuleManager与MemoryStore/StorageEndpoint；
控制宿主时钟、取消和外部心跳观察快照，不模拟内部调度算法。管理动作无持久化写入，
不编造存储故障；每个TTL队列均await shutdown。

[initial](logs/task_126/initial.json)为新增本地依赖后的锁文件未更新exit101，
[wiring](logs/task_126/wiring.json)包含离线更新锁文件及缺失API编译红灯；两者仅作接线记录。
离线重新解锁曾带入无关pulldown-cmark更新，已恢复原版本/校验和，最终Cargo.lock仅增加
handler到既有splitter的路径依赖，无远程版本变化。
[implemented](logs/task_126/implemented.json)暴露测试装配副本数未显式设置，
[corrected](logs/task_126/corrected.json)修正后2项通过；[expanded](logs/task_126/expanded.json)
补齐5项通过。[coverage](logs/task_126/coverage.json)记录加精确错误断言时引用不存在枚举的
编译错误，改为实际AlreadyExist后[refined](logs/task_126/refined.json)5项通过。
这些装配/编译错误均不冒充行为红灯。

[behavior-red](logs/task_126/behavior-red.json)记录同一完整测试对错误实现的实际检测：
响应百分比固定0、空输入改为[]，exit101、2通过/3失败，断言明确检出null/[]、50/0、
100/0差异。随后恢复正确代码、格式化，再重新执行全部验证。

最终结果和原始stdout/stderr合并压缩日志见[final.json](logs/task_126/final.json)，每项包含
确切命令、退出码、日志哈希与test result。[verify.py](logs/task_126/verify.py)每阶段原子
mkdir动态领取共享槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，
owner记录任务/计划/会话/PID；Cargo及子进程继承绝对路径。正常/异常/中断均先等待或
终止自己的子进程，再删除owner并释放锁，缓存保留。构建并发4、测试线程4、
GOFLAGS=-mod=readonly；无新增feature。

| 最终命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 已格式化 |
| `cargo test -p astersql-pd-schedule-handler --locked --lib task_126 -- --nocapture` | 0 | 5通过、0失败、0忽略，20个案例 |
| `cargo test -p astersql-pd-schedule-handler --locked` | 0 | 45通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace装配通过 |
| `cargo fmt --all -- --check` | 0 | 格式检查通过 |
| `cargo check --workspace --locked` | 0 | workspace检查通过 |
| `cargo test --workspace --locked` | 0 | 1817通过、0失败、1既有忽略 |

workspace数量含2项既有compile-fail文档测试；本任务未增加忽略。仅有既有server的
proto deprecated字段warning。[只读复核](logs/task_126/verification.json)由
[check_readonly.py](logs/task_126/check_readonly.py)对比起始压缩清单：1101份Go/模块
文件（包含未跟踪路径）及两份只读plan.md的路径、内容哈希均不变，Go/模块Git状态为空。
`git diff --check`通过，Markdown相对链接已检查。所有阶段的自有锁均已释放，保留缓存。
仅本任务生产代码、测试、局部注册/路径依赖、说明与验证证据纳入提交，保留其它任务删除。
状态：已完成。
