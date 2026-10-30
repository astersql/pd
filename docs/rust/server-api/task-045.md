# 任务045：Leader 管理 API

状态：已完成（专属独立etcd部署边界）。

基准：`ec980d5b09d1771e0f8f864143ff6e5ab00fae53`。批次44的专属独立etcd成员所有权、运行时成员管理与只读oracle证据已核对。本任务保持Go、模块文件和plan.md只读。

## 接线和行为边界

[HTTP适配](../../../pkg/server/http_leader.rs)接通GET `/pd/api/v1/leader`、POST `/pd/api/v1/leader/resign` 和POST `/pd/api/v1/leader/transfer/{next_leader}`。查询复用注册表与线性一致leader租约快照；成功JSON字段与Go一致，没有leader时为null。转移按真实成员名称解析身份，不存在返回500及`no valid pd to transfer etcd leader`；单成员辞任和自转移保持现有任期。

[宿主实现](../../../pkg/runtime/leader_admin.rs)复用批次44明确声明的独立etcd专属所有权，核对backend cluster、正反向绑定、注册表和撤销状态，复用HTTP健康探测确认目标PD可达，再在现有Leadership任期事务中验证后调用真实etcd MoveLeader。探测后比较注册表revision，避免期间改名/改址后仍提交旧目标。共享后端未配置专属能力时拒绝转移，PD ID不冒充etcd ID。etcd/Raft不进入领域crate，未新增依赖或平行服务。名称解析与绑定验证使用同一MetaStore。

[角色worker](../../../pkg/runtime/member_role_worker.rs)和[启动装配](../../../pkg/runtime/node.rs)让专属部署只在本机对应etcd成员成为共识leader时竞选PD，失去共识领导权后取消旧term、join其workers并撤销自身租约，再允许新leader恢复服务。租约和任期事务仍是持久写入的fencing条件。短暂共识切换的Unavailable允许重试；租约清理重试窗口为2秒，成员监控连续读取失败2秒后停止，已撤销成员立即停止。请求每次准入仍检查成员状态、共识角色及任期，没有在失败窗口开放写入。普通共享后端不启用新增竞选重试策略。

MoveLeader与元数据验证不是跨系统原子事务；取消或响应丢失可能发生于共识已转移之后，由实际共识状态与角色worker收敛，不承诺回滚。独立etcd仍由部署/宿主启动，不宣称Go embedded-etcd进程拓扑等价。超时受宿主连接配置限制，错误文本按Go `ErrEtcdMoveLeader`及底层cause映射；未虚构精确相同的超时延迟。

## 来源覆盖

- [wire-inventory](wire-inventory.md)：HTTP-122、HTTP-123、HTTP-124全部对应上述三条路由。
- [source-inventory](source-inventory.md)：任务45行；`server/api/member.go` GetLeader、ResignLeader、TransferLeader；`pkg/member/member.go` ResignEtcdLeader/MoveEtcdLeader；`server/server.go`共识leader与PD leader对齐、退位和重新竞选行为。
- [surface-inventory](surface-inventory.md)：router.go 304–306的三个注册行。审计后端仍归任务81，本任务不把路由接通当作审计验收。

## 失败与修复

[server测试](../../../pkg/server/http_leader_test.rs)首次真实TCP运行1项失败、退出101：旧路由返回200空body，解析leader JSON失败（EOF）。测试成功编译，不是以编译失败代替行为失败。

三节点真实etcd测试发现旧角色循环在MoveLeader期间收到Unavailable后关闭节点；增加有界租约清理重试、专属部署的角色重试和成员监控容错后修复。另一次测试在leader键已发布但服务尚未完成初始化时立刻辞任，得到`not leader`，因此测试有界等待真实服务发布后再进行下一次操作。初始化中丢失任期也按专属部署的可重试竞选失败处理。

独立部署补充测试发现PD已关闭而其专属etcd仍存活时，原实现错误返回200（期望500，退出101）。复用已有真实HTTP ping探测后拒绝该目标，保留原PD租约和共识leader。HTTP探测继承现有传输边界：不降级HTTPS，未配置TLS传输的HTTPS目标明确拒绝，不宣称TLS接线已完成。探测不能与远端进程生命周期原子绑定；探测后发生的故障仍由真实共识RPC和任期worker处理。

停机目标测试最初使用包含停机端点的负载均衡MetaStore连接，观察到真实元数据Unavailable而非MoveLeader错误。最终保持存储入口节点存活、停止另一个非leader专属成员，隔离本任务要验证的目标不可用场景；没有修改存储错误为成功。生产仍保留元数据错误原样返回。

完整Go错误文本差分再次失败、退出101：Rust返回`[PD:etcd:ErrEtcdMoveLeader]etcd move leader error:Timeout expired`，Go返回`[PD:etcd:ErrEtcdMoveLeader]etcdserver: request timed out, leader transfer took too long: etcdserver: request timed out, leader transfer took too long`。补齐超时与Go cause格式映射后逐字比较通过，不仅检查500或错误前缀。

## 真实验证场景

- server默认测试：真实单节点HTTP查询、未知目标、自转移和唯一成员辞任；错误及no-op前后leader键/lease保持相同。
- [runtime测试](../../../pkg/runtime/leader_admin_test.rs)：三个真实独立etcd voters和三个PD，HTTP转移/辞任后核对etcd Status、PD leader、不同lease及递增revision；持有旧ClusterHost执行写入被拒绝。分别停止非leader目标的etcd（保留PD监听以确保经过真实MoveLeader RPC）和PD服务（保留其etcd），两种请求均失败，当前共识leader与PD任期不变。
- runtime默认故障/取消测试复用[事务gate](../../../pkg/runtime/member_admin_fault_test.rs)：向真实etcd提交超过限制的事务，HTTP500且无持久写入；在真实领导验证事务前暂停后取消宿主，请求失败、节点join、共识leader未被改变。不以mock调用次数证明结果。
- [只读Go supervisor](../../../scripts/http-leader-oracle.py)：仅启动现有`bin/pd-server`，真实三节点join、GET/self/missing/transfer/resign/dead操作；用etcdctl读取真实leader键lease。Rust与Go比较完整成功/错误JSON，leader字段集合完整保留；仅各自核对不同部署的ID、name和URL，不要求随机身份值相同。没有新建或运行Go源码生成器。

Go binary版本与SHA-256、每轮原始响应、lease读数、退出码和清理记录均保存在压缩证据中；已有binary没有VCS revision，不能据此宣称来自当前源码HEAD。之前失败轮次也保留，不计入最终通过。

server全包首轮162通过/1失败/6忽略，暴露相邻task019在新leader恢复中关闭宿主的竞态：`cluster recovery cancelled`字符串绕过已有类型化取消识别。将恢复及初始化的取消统一为现有MemberError::Cancelled，并增加确定性的已取消恢复测试。后续窄回归还观察到领域包装后的`Storage(Store(Cancelled))`，因此初始化被宿主取消时，必须先成功清理租约，再统一报告类型化取消；清理失败和未取消时的错误继续传播。工作区首轮还观察到无关的storage-endpoint并发取消测试首批提交数量不稳定（0而非120），保留失败并单独复核，最终回归限制测试线程数以降低真实etcd进程争用，不修改该测试断言或存储实现。

## 最终验证

所有阶段按1–10原子领取，实际均使用槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
每阶段记录任务/计划/会话/PID；命令及子进程继承该绝对路径。进程已退出，自有锁释放，编译缓存保留。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 实现阶段格式化 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 工作区元数据 |
| `cargo fmt --all -- --check` | 0 | 最终格式检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 最终server静态检查 |
| `cargo check --workspace --locked` | 0 | 最终工作区静态检查 |
| `cargo test --locked -p astersql-pd-server --lib task_045_ -- --nocapture` | 0 | 1通过/0失败/0忽略 |
| `RUST_TEST_THREADS=4 cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 164通过/0失败/6默认忽略，含integration target |
| `PD_ETCD_BIN=/tmp/pd-task043-etcd-v3.5.15/etcd cargo test --locked -p astersql-pd-runtime --lib task_045_ -- --include-ignored --nocapture` | 0 | 3显式运行通过/0失败/0忽略 |
| `cargo test --locked -p astersql-pd-server --lib task_019_behavior -- --nocapture` | 0 | 关闭竞态修复后1通过 |
| `cargo test --locked -p astersql-pd-storage-endpoint --lib batch_ops_inflight_cancel_etcd -- --nocapture` | 0 | 无改动窄回归1通过 |
| `PD_ETCD_BIN=/tmp/pd-task043-etcd-v3.5.15/etcd cargo test --locked -p astersql-pd-runtime --lib task_044_remove_real_dedicated_member -- --ignored --nocapture` | 0 | 实现阶段相邻回归1显式通过 |
| `RUST_TEST_THREADS=4 cargo test --workspace --locked` | 0 | 最终2053通过/0失败/43默认忽略，含compile-fail doctest |

默认忽略不计成功；新增独立部署用例已经在上面的显式运行和下面的Go supervisor中运行通过。最终差分命令：

```sh
PD_COMPAT_GO_BIN="$PWD/bin/pd-server" \
PD_ETCD_BIN=/tmp/pd-task043-etcd-v3.5.15/etcd \
python3 scripts/http-leader-oracle.py
```

该命令在已领取的槽位1环境内执行，退出0，内含`cargo test --locked -p astersql-pd-runtime --lib task_045_real_transfer_and_fencing -- --ignored --nocapture`：1通过/0失败/0忽略。最后一次重新启动真实Go节点采集的记录为`target/rust-slot-1/http-leader-oracle-1793324915932056000.json`，未只回放历史Rust结果。完整GET/self/missing/transfer/resign/dead响应、真实lease观察、Go构建信息、官方独立etcd二进制指纹和子进程清理记录均归档。

最终1101个Go/模块文件（含ignored/untracked）的路径集合、原字节SHA-256及受保护Git状态与开始完全一致；plan.md哈希不变。1223个Rust/Cargo文件的最终验证前后哈希一致。文档链接、Python语法、`git diff --check`通过；没有遗留测试/oracle进程或自有槽位锁。

[压缩原始证据](task-045-evidence.json.gz)保留全部失败与通过的确切命令、退出码、测试输出、Go观测及只读守卫。SHA-256：`d28692e27c743ec1ab825ae44e2c9bdab65e38bc4d5f09d21b0d9991845f8163`。
