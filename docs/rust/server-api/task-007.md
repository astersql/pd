# 任务007：驱动多成员角色切换

状态：已完成（本任务多成员角色切换边界）。

基准HEAD：`7deb70393`。已读取只读计划、任务1–6证据以及source/wire/surface清单；任务6持久恢复、任务5共享cluster和任务4只读oracle入口均已交付。主题6准入沿用任务1的核验，当前新增服务行为以本任务实测为准。固定kvproto commit仍为`f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

## 实现与边界

- [member_role_worker.rs](../../../pkg/runtime/member_role_worker.rs)将listener与leader term分离。已有leader时以follower启动；线性一致快照后从下一revision使用canonical LeaderWatch，避免campaign冲突到订阅之间漏掉删除；watch错误/compaction后重新读快照。LeaderWatch由member直接重新导出，没有复制领域选举实现或引入新依赖。
- [node.rs](../../../pkg/runtime/node.rs)只在赢得campaign后创建本届恢复缓存、Allocator、Oracle和heartbeat workers。失租时先撤下服务入口、取消该届scope、join所有cluster workers并释放该届lease/collector，再进入follower/选举循环。listener保留至宿主取消或不可恢复错误。恢复和每届TSO初始化沿用任务6的续租/错误清理。
- RoleServices每次请求重新取得当前term并线性验证；旧请求保留的Allocator/Oracle继续使用原来的immutable Leadership。TSO和ID的缓存窗口不绕过原有事务fence。Application额外拒绝本地过期term。
- [cluster_host.rs](../../../pkg/runtime/cluster_host.rs)在宿主直连heartbeat入口也校验leadership；恢复和调度持久化使用TermStore，由原Leadership::transact原子比较leader value、lease和mod_revision。失租后的旧worker不能提交写入。此受限存储只开放canonical pipeline所需read/transact；branch/raw/admin写入口明确拒绝，不伪装成完整MetaStore能力。
- `PdNode::try_cluster()`在follower无leader cache时返回None；原`cluster()`保留leader调用方式并记录panic前提。宿主必须在角色可能变化时使用try_cluster。缓存句柄在其term退出后停止资源；不承诺旧cache句柄自动变成下一届cache。
- [node_test.rs](../../../pkg/runtime/node_test.rs)将旧的失租退出断言更新为失租换届且保留listener；不是删除错误检测。
- 只读supervisor增加`--case member-role`。现有Rust harness将显式extra参数置于默认参数后，让Go join节点可覆盖name与initial-cluster；没有生成、编译或修改Go文件。

清单映射：source-inventory“7 驱动多成员角色切换”整行及最小runtime角色前置对应本证据；wire-inventory各方法共同“5–7集群角色”只解除本任务的多成员启动、fencing和换届前置；surface-inventory共享宿主取消、listener和Registry仅覆盖本任务生命周期。GetMembers完整成员列表/leader字段、follower转发、角色请求错误码、完整HTTP health契约仍归任务18/19等owner；现有HTTP health表达本地leader readiness，不能据此宣称follower健康输出已与Go一致。本任务未扩展这些独立协议面，也未宣称embedded-etcd拓扑等价。

Go只读映射：server/server.go::leaderLoop的CheckLeader→Watch→重新campaign，以及campaignLeader的start/stop服务届次；pkg/member的Leadership/lease比较和watch契约；server/grpc_service.go::AllocID/Tso的本地角色准入。Rust复用已有Member/LeaderWatch/Leadership、Allocator/Oracle和ClusterHost，不创建平行领域实现。

## TDD与实际差分

初始测试可编译并实际运行，第二节点在已有leader时start返回`leadership is held by another term`，清理第一节点后在`follower must start under existing leader`断言失败：退出101，1失败/0通过。没有把编译错误当成缺陷证据。

最终task_007_behavior使用同一真实外部etcd启动三个Rust PdNode，经真实TCP/tonic验证leader分配ID/TSO、两个follower拒绝本地写；取消一个阻塞watch的follower后原leader继续分配；原leader优雅退出后另一节点取得新lease并提供递增ID/TSO；旧cache句柄拒绝访问。真实revoke当前lease后等待新的持久revision和服务恢复，避免以调度延迟假定旧节点必然暂时可见。所有Registry清空、父取消token隔离、最终leader key删除。

独立task_007_stale_scheduling_transaction_is_fenced先通过真实etcd提交一笔调度写，再revoke旧lease、让另一term campaign；旧TermStore事务返回错误且拒绝键持久不存在，新term仍validate成功。验证实际存储副作用而非mock调用数。测试不推进墙钟；lease故障由真实revoke触发，poll sleep只等待可观察状态。

readonly_member_role_differential在只读supervisor中运行现有Go二进制：三成员通过真实join组成一个Go embedded-etcd集群，直接调用follower的ID/TSO失败；停止原leader后剩余两成员保持quorum并接管；ID/TSO严格递增；通过同一MetaStore读/revoke新leader lease后以新revision和RPC分配确认恢复。Rust使用同一外部etcd的三节点场景。对比的是角色拒写、换届和单调性等性质；随机ID、时间与拓扑不作字节等价断言。

两侧实际归一化结果一致：

```json
{"members":3,"follower_rejects":true,"id_increases":true,"tso_increases":true,"handover":true,"revoke_recovers":true}
```

中间修正：Rust类型推断/Result别名编译错误；旧lease-loss回归仍期待终止listener，按本任务新语义更新。首次最终差分已通过（1有效测试/0忽略），随后复用canonical LeaderWatch并增加本地expiry准入，最终版本再重跑监督器。

## 验证记录

所有Cargo过程从仓库根、默认features执行，原子动态领取的实际槽位均为1：`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。每阶段记录任务/会话/PID，最终锁释放；缓存保留。最终进程清单没有本任务槽位二进制或Go PD进程存活。

| 确切命令 | 退出码 | 有效测试数 |
| --- | --- | --- |
| 初始`cargo test --locked -p astersql-pd-runtime --lib task_007_ -- --nocapture` | 101 | 1行为断言失败 |
| 最终同一聚焦命令 | 0 | 2通过/0忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 10通过/3 oracle忽略（最终监督器全部补跑） |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all`、`cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 1935通过/4 oracle忽略（最终监督器全部补跑） |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 最终独立二进制 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case member-role --case cluster-restore --case cluster-host --case health-member --case tso` | 0 | 五场景各1通过/0忽略 |
| `bash -n scripts/run-pd-compat-readonly.sh`、Python AST、`git diff --check` | 0 | 静态检查 |

全量workspace通过后仅调整Go差分的Rust测试/harness：三成员关闭等待从默认8秒改用显式30秒，普通场景仍是8秒；第三成员只对Go/etcd strict reconfiguration的`etcdserver: unhealthy cluster`准入错误有界重试，其他错误仍失败。随后聚焦、runtime包级和最终五场景重新编译/运行通过，没有生产实现变更。一次8秒关闭超时、一次第三成员尚未健康的join失败、一次主动取消的中间轮次（130）均保留在原始记录；它们不计通过。并非降低Rust角色验收断言或无条件重试失败测试。

Go守卫：最初保存target以外1101项的路径/哈希；完整集合使用任务6已保存的1983项基线，再与本任务监督器每次完整before/after及最终扫描比较，归一化`./`路径后全部一致。完整集合包含ignored/untracked及target内已有Go源码；Go/模块Git状态为空。最终只读监督器的readonly_unchanged、inputs_unchanged、五个进程组清理和工作目录清理均为true。plan.md与HEAD字节一致；任务1–6已有删除不纳入本提交。

Go二进制build info没有VCS revision，不能声称来自当前HEAD。Go运行使用其真实embedded-etcd三成员拓扑，Rust使用外部etcd过渡模式；差分只验收上述角色/单调性/持久化fence性质。完整HTTP follower健康形状、成员查询和转发由各自owner验收。

完整命令、原始日志、最终源码/二进制哈希、前后Go文件哈希及监督器报告见[压缩证据](task-007-evidence.json.gz)，SHA-256：`3d8b385af731a0c3ef1d540e1c4561e146b7d85f870a5a721be4c18fdc70aeab`。
