# 任务089：加入失败重试与恢复

状态：已完成。

基准 `a23375105`；2026-11-01。任务088及任务1–4的清单、只读oracle入口已核验；协议继续固定 kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。原始日志、Go进程输出、只读快照及owner89来源行见[压缩证据](task-089-evidence.json.gz)。

## 行为与接线

[恢复实现](../../../pkg/runtime/join_resume.rs)在现有外部MetaStore中以单次事务写入member及`join-pending/<member_id>`，比较member、pending及removed-member不存在，不覆盖原成员或删除墓碑。继续由canonical `Member::bootstrap`核验不可变身份。[预检](../../../pkg/runtime/join_preflight.rs)仅允许持有完全一致待恢复记录的成员复用名称/ID；名称、ID、client或peer改变不能借用恢复凭据，其他成员冲突和join目标校验保持有效。没有凭据的已登记身份仍按fresh重复加入拒绝。

[宿主](../../../pkg/runtime/node.rs)在服务future首次poll确认监听有效后，以值比较事务删除凭据，再发布ready。初始化失败保留原记录供重试；不携带join的既有restart路径也会完成相同凭据，避免成功启动后仍可被误判为失败恢复。成员记录的值及revision在恢复过程中均不重写；重复加入不会生成新成员。

builder调用方丢弃或超时会通过作用域guard取消初始化，宿主executor上的各初始化I/O边界观察取消并释放listener；已经交付的PdNode继续由原有shutdown/drop拥有worker。取消远程操作不等于撤销已提交的存储事务：如果登记已提交，其原子pending凭据支持下一次恢复。保留现有Member选主错误回收和prepare_term取消后的resign清理路径，不用整个初始化future的强制丢弃绕过清理，也不创建额外续租任务。

## 真实测试与差分

[独立Rust测试](../../../pkg/runtime/join_resume_test.rs)通过公开builder、真实TCP监听器及同一真实etcd MetaStore运行：真实collector冲突发生于成员登记之后，验证失败端口可重绑、成员和凭据持久化；携带同一join和不携带join分别恢复，GetMembers RPC返回2成员、HTTP ping返回200，原member条目（含revision）不变，ready后凭据消失，重复fresh join拒绝。改变peer的恢复请求拒绝。另以真实TCP gate阻塞etcd HTTP/2握手，确认调用方超时，初始化释放listener；放行同一真实连接后读请求成功，完整`/pd/`状态无新增，proxy任务终止并等待。

[只读Go oracle](../../../scripts/join-resume-oracle.py)复用既有`bin/pd-server`，SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`。build info为go1.26.8/without_dashboard/darwin-amd64，无VCS revision，不能声称由当前HEAD构建。通过现有etcd HTTP gateway真实MemberAdd登记一个尚未启动的peer，制造1/2 quorum状态，再以相同peer启动携带join的Go PD。日志确认为previously failed to start after join，恢复后HTTP成员列表的member ID与MemberAdd返回完全一致；重复名称进程非零退出，成员列表完全不变。全部子进程回收、临时数据目录删除，Go文件守卫一致。

差分比较的是“已登记未启动→同身份恢复→成功后重复加入拒绝”的实际状态和副作用；Go的Raft member ID与Rust的PD member ID各自保持原值，不将二者归一为同一个身份。Rust没有执行或伪造embedded-etcd MemberAdd、非线性成员列表或数据目录恢复；其登记事务提交即可见，无需复制Go的MemberList 20次轮询。Go的3条`addMemberFailed`注入未启用，缺成员/未成功登记时由实际预检和原子事务拒绝；不声称已运行该failpoint路径。主题10的内嵌拓扑/持久性等价性仍保留边界，不由本任务解除。

## 来源映射

[source-inventory](source-inventory.md)的任务89映射、node公开入口/cluster identity持久化及两类spawn、join.go的PrepareJoinCluster/3条failpoint、pkg/member下13个manifest文件行均逐项归档。node入口由本次初始化取消与恢复测试及完整runtime回归覆盖；member的不可变登记和选主复用原实现，Go election/member/participant/metrics及Rust leader_priority/member_info不另建实现，其既有测试随workspace回归。join.go的失败登记恢复、重复拒绝有真实Go进程证据；嵌入式拓扑和failpoint边界如上明确保留。wire清单仅共享主题10边界，surface清单无独立owner89行为行。

## 失败与验证

初次聚焦命令编译运行后退出101：0通过、1失败，失败断言为`failed registration must resume same identity: missing data or join a duplicated pd`。不是编译失败。修复后1项通过，补取消及无join恢复测试时又发现成功restart遗留pending，退出101（2通过、1失败）；宿主按实际pending状态完成清理后最终聚焦3项通过。

所有Cargo阶段依次原子尝试1–10，实际均领取slot-1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，持锁shell记录task/session/PID并以EXIT trap清理自己的owner及锁目录；缓存保留。默认features，`CARGO_BUILD_JOBS=4`，包级和workspace回归设置`RUST_TEST_THREADS=4`。

| 确切命令 | 退出码 | 有效测试数 |
| --- | --- | --- |
| `cargo test --locked -p astersql-pd-runtime --lib task_089_ -- --nocapture`（首次行为失败） | 101 | 0通过、1失败 |
| 同一命令（无join凭据清理失败） | 101 | 2通过、1失败 |
| 同一命令（最终） | 0 | 3通过、0忽略 |
| `env RUST_TEST_THREADS=4 cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 71通过、36项既有忽略 |
| `cargo fmt --all`；`cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据检查 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 编译检查 |
| `cargo check --workspace --locked` | 0 | 编译检查 |
| `env RUST_TEST_THREADS=4 cargo test --workspace --locked` | 0 | 2200通过、76项既有忽略 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_JOIN_RESUME_REPORT=/tmp/task089-go.json python3 scripts/join-resume-oracle.py` | 0 | 1组真实未启动成员恢复、1组重复拒绝 |

最终Go只读守卫：开始/结束1101个Go/模块文件（含ignored/untracked）的存在性、内容SHA-256及Git状态完全一致。plan.md字节与基准HEAD一致。全部测试、fixture和oracle子进程结束，slot-1无残留产物进程，自有owner/锁目录已删除，缓存保留；用户原有prompt.md变更不纳入提交。Python AST及`git diff --check`通过。
