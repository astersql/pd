# 任务094：Ready状态门

基准 `2bcc51476`；依赖任务93及任务1–4清单/只读oracle入口已核验。

## 行为与来源

[ready_gate.rs](../../../pkg/runtime/ready_gate.rs)保存Pending/Ready/Failed启动确认。
等待future被取消不消费确认，失败保持失败，避免第二次ready绕过尚未运行的listener。
[node.rs](../../../pkg/runtime/node.rs)继续由真实listener首次poll和join确认发布启动信号，
再读取canonical RoleServices本地探针：未bootstrap允许ready；已bootstrap必须完成region恢复，
follower必须完成同步；leader额外验证当前lease。取消返回错误，失败路径仍等待拥有资源的worker退出。
不等待调度器的业务prepare阈值：Go的promotion检查是region加载条件，并非调度执行条件。
ClusterHost构造已启动必要worker，leader term仅在恢复、TSO窗口及构造成功后发布。

[source-inventory.md](source-inventory.md)任务94行、node.rs共享1/3/5/7/83–87/89/93–95/98行、
server/server.go共享服务装配及server/apiv2/handlers/ready.go的2/43/78/94行适用。
[wire-inventory.md](wire-inventory.md)的ready/readyz由43拥有HTTP契约；本任务复用，不复制路由。
[surface-inventory.md](surface-inventory.md)listener/宿主Registry的82–87/94–96共享行适用；
没有独立新配置、协议或指标。
Go `checkLeaderPromotion`：未bootstrap成功；否则AreRegionsLoaded必须成立，leader/follower不分叉。
server.go的leader promotion在createRaftCluster、allocator rebase完成后，且取消控制leader scope。
外部MetaStore不是Go embedded-etcd拓扑等价；不宣称本任务覆盖后者。

## 测试与边界

[ready_gate_test.rs](../../../pkg/runtime/ready_gate_test.rs)由lib.rs显式注册。
`task_094_behavior`使用可控Tokio时钟验证取消等待、重复等待、确认成功及确认失败的稳定结果。
首轮测试编译成功后断言失败：第二次等待提前成功，退出101，0通过/1失败。
后续尝试发现已完成oneshot不可重poll；显式Failed状态解决该问题，不把中间失败当最终结果。
`task_094_real_leader_follower_and_cancel`使用真实etcd、同一MetaStore和两个真实TCP服务，
分别覆盖未bootstrap及已持久化cluster metadata；leader/follower ready后通过真实HTTP检查
is_bootstrapped/region_loaded，通过GetMembers RPC检查leader身份，取消后ready拒绝并join关闭。
已有包级恢复、加载失败、lease失效、HTTP加载/取消及任务93启动事务故障场景作为相邻回归。

## 验证记录

动态mkdir领取slot-1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
默认features，`CARGO_BUILD_JOBS=4`、`RUST_TEST_THREADS=1`；原始日志、命令、退出码、数量及
只读守卫在最终验证后归档。

公开入口反例 `task_094_unavailable_leader_does_not_acknowledge_follower` 在真实etcd上建立
真实leader lease，leader TCP监听但不提供同步；已bootstrap的follower两次ready等待均不得成功，
随后取消返回错误且listener可重绑。以基准HEAD的完整node.rs运行同一可编译测试，
退出101、0通过/1失败，明确断言“follower must wait for the durable region view to synchronize”。
保存失败后恢复本任务node.rs，再格式化并运行最终测试；没有修改Go或人为改变后端返回值。
`task_094_cancelled_start_never_becomes_ready`检查预先取消启动的确切错误为`node is stopped`，
Registry为空且端口可重绑。

只读 `scripts/http-health-oracle.py` 启动固定已有Go二进制，采集十个真实HTTP探针，
再调用现有Rust差分测试，在真实Rust节点与etcd上比较状态、Content-Type和正文；
仅归一化health中的随机member ID和listener URL。涵盖未bootstrap ready成功、
readyz排除参数及未知路径错误；Go关闭退出码、目录清理、输入及Go只读守卫随报告保存。
bootstrapped follower同步故障和宿主Future等待取消由上面的真实Rust故障测试覆盖；
Go没有同构宿主Future入口，不声称对Go注入了该故障，也不使用需要failpoint改写的Go测试。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test --locked -p astersql-pd-runtime --lib task_094_ -- --nocapture`（初轮） | 101 | 0通过/1失败，取消等待后提前确认 |
| `cargo test --locked -p astersql-pd-runtime --lib task_094_unavailable_leader -- --nocapture`（基准node.rs） | 101 | 0通过/1失败，真实follower提前ready |
| `cargo fmt --all` | 0 | 最终格式化 |
| `cargo test --locked -p astersql-pd-runtime --lib task_094_ -- --nocapture`（最终） | 0 | 4通过、0忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture`（最终） | 0 | 77通过、36项既有忽略 |
| `cargo fmt --all -- --check` | 0 | 最终格式检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 最终元数据 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 最终包级编译 |
| `cargo check --workspace --locked` | 0 | 最终workspace编译 |
| `cargo test --workspace --locked` | 0 | 2214通过、76项既有忽略；同一生产实现，后加的取消启动和不可同步follower两项由最终聚焦/包级单独覆盖 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 当前进程产物 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_HEALTH_REPORT=/tmp/task094-oracle.json python3 scripts/http-health-oracle.py` | 0 | 真实Go/Rust HTTP差分1通过、0忽略；报告由入口保存在领取槽位中 |

初轮聚焦、workspace/Go差分及最终验证分别动态领取并释放slot-1；没有并发复用占用槽位。
最终Go/模块保护集合1101个文件（含ignored/untracked）路径与内容SHA-256不变，Go Git状态无差异，
plan.md与HEAD逐字节一致。宿主、fixture及Go oracle进程均已结束，自有锁已释放，缓存保留。
原始日志、确切脚本、来源行、源码哈希、oracle原始响应及前后守卫见
[压缩证据](task-094-evidence.json.gz)。文档链接与git diff检查通过。
用户原有prompt.md修改不纳入提交。

状态：已完成。
