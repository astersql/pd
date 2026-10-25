# 任务028：构建器领导者择优

状态：已完成。前置批次27提交 `d2308c1f9`，证据 task-027.md 已核实：
真实 canonical Builder::build_simple、OpStep 与 Planner 可用，44个包测试通过。
本任务将已有 leader/替换评分集中到 [builder_leader.rs](../../../pkg/schedule/operator/builder_leader.rs)，
由 builder_simple 的局部子模块接线，未增加 crate、依赖或 feature。
生产 API 仍为 `Builder::build_simple`；内部 `select_target_leader`、`rank`、`best`、
`replace` 在真实 Planner 状态上运行。领域说明见 [builder_leader.rs.md](../../../pkg/schedule/operator/builder_leader.rs.md)。

修正 `allow_leader_at(peer, force, current_leader)`：规划中的当前 leader 豁免
store/placement 检查，原 leader 转出后重新执行准入；角色/witness 始终优先检查。
prepare 阶段和公开 allow_leader 仍使用 origin leader。其余评分按原实现复用并验证，
没有改写 Go 已有混合降级规划边界，joint/witness 规划保留后继任务范围。

固定 [fixture](fixtures/task_028.json) 为17项输入及步骤/错误输出：current 对负载优先、
expected leader、follower 排除、count/size 负载、完全并列保留首个 store、显式目标、
SetLeaders 排序/重复/过滤和构建时不重滤、force down/非法目标、nearest/Unicode、
old-peer 替换、混合 promote/demote/add 和缺失目标错误。
另一个真实 Planner 测试覆盖3个五维 leader 评分、5个六维 plan 评分和2个比较，
明确验证 keep-voter、age、up、位置最小分数、目标偏好、新 peer 惩罚及并列规则。
第三个异步测试覆盖4项 peer 准入断言及最终 leader 选择、snapshot 源选择，使用
真实 StoresInfo/StoreStateFilter，无 store I/O mock。17项 fixture 检查每个步骤、
kind、TransferLeader.to_stores、typed error/首错、region metadata 与输入所有权。

Go 契约依据 source_commit `836fdc7cf3ef34f7eabc9d051418995bb97efabe` 的只读
builder.go（setTargetLeaderIfNotExist、prefer*、planReplaceLeaders、comparePlan、
labelMatch、allowLeader）及 builder_test.go 的 TestBuild/TestRecord 源码断言。
TestMain/TestOperatorBuilderTestSuite 仅是入口，未当成案例覆盖。
fixture 保存当前两份源码 SHA256、原始输入/期望；没有时间或随机归一化。
oracle-index.json 已有运行案例不覆盖该 builder 故事。遵照用户最新授权采用
`source_assertion` / `go_execution=false`，没有 Go oracle 执行或运行差分声明。

[red.json](logs/task_028/red.json) 保存四次行为回退：恢复 origin-leader 豁免、
反转 count、忽略 nearest、并列选后者，均由相同 task_028 测试触发断言失败，
退出101，非缺 API 或零测试。first-green.out 保留重构接线时遗漏 import 的编译失败；
它不作为行为失败证据。变异脚本每次先 cargo fmt --all，以 finally 恢复生产字节。
没有存储写入，因此不引入不相关持久化故障；非法 peer/缺失 store/拒绝 leader
是真实错误路径。allocator 故障继续由包级 task027 回归覆盖。

全部 Cargo 阶段经 slot-run.py 原子领取实际槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁记录任务/会话/PID，正常与信号中断均等待子进程退出后仅释放自己的锁，缓存保留。
[validation.json](logs/task_028/validation.json) 保存确切 argv、退出码和有效测试数量，
相应 validation-N.out 保存 stdout/stderr。workspace etcd 测试复用前置任务保存的
原生 Go I/O fixture，Go 拦截器只允许复制哈希核实的既有二进制，拒绝实际 Go 调用；
[provenance](logs/task_028/fixture-reuse-provenance.json) 和 go-reuse.jsonl 仅为外部
etcd 测试边界证据，与本任务 builder oracle 无关。

| 验证命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_028 -- --nocapture` | 0 | 3通过、0失败、0忽略 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 47通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1410通过、0失败、1忽略 |

聚焦3测试通过，其中17个固定场景、3个leader和5个plan评分、2个比较、
4项准入及2项Planner选择断言。全包47通过，workspace1410通过、0失败、1既有忽略。
最终清理测试中从task027复制但本fixture不可达的执行边界分支后，再次运行
cargo fmt --all、task028、全包和fmt check；[final-focused.json](logs/task_028/final-focused.json)
记录最终结果（生产代码未变，因此不重复外部etcd回归）。

[verification.json](logs/task_028/verification.json) 核对1118个只读文件，其中1101个
Go/模块文件和17份本计划plan.md：内容均未变化，受保护Git状态为空；
四次变异恢复生产哈希一致，git diff --check 退出0。fixture SHA256为
`674cf9858a25fe141845ad76eaefb31389606098e8ac876b076bed11663548f8`。
所有Cargo子进程退出后释放自有槽位锁并保留缓存。完成差异审查后只提交本任务
生产、测试、说明、fixture和日志；原有其它任务文件删除不纳入。不修改只读计划或
来源主题，编号任务标记完成并按授权删除，后继批次从本证据与提交核验依赖。
