# 任务019：集群信息与成员查询

状态：已完成（classic PD、既定外部MetaStore过渡架构）。

基准：`a5ab7ab72b7c`（完整HEAD及输入哈希保存在压缩证据中）。依赖任务1–4清单、任务18证据已核验。只读plan.md、Go源码及模块文件保持原样；保留编号任务1–18的既有删除。

## 来源与实现

- [source-inventory.md](source-inventory.md) 的任务19、`pkg/member/member_info.rs` owner=7/19/44/45/89/98行；[wire-inventory.md](wire-inventory.md) 的GetClusterInfo/GetMembers owner=19行。本任务实现classic PD discovery；[surface-inventory.md](surface-inventory.md)没有独立owner=19的配置、指标或CLI行。
- Go行为来源：`server/grpc_service.go` GetClusterInfo、GetMembers、getMembersErrorResult、findLeadersInMembers；`server/server.go` loadMembers；`server/cluster/cluster.go` GetMembers。Go gRPC成员只含name/member_id/client_urls/peer_urls，发布的binary_version/deploy_path/git_hash和leader_priority等仍为空/零；`pkg/member/member_info.rs`管理的是独立发布键，由HTTP管理任务44等继续处理。
- [grpc_members.rs](../../../pkg/server/grpc_members.rs)从宿主同一MetaStore读取既有Member::bootstrap持久注册，解析协议在适配层完成。成员与PD leader按同一revision读取，不依赖本地term、健康检查或bootstrap状态，不把本机Identity当作成员全集。PD leader按ID映射回成员列表，缺失时保留None；不缓存列表。
- [grpc.rs](../../../pkg/server/grpc.rs)和[SyncGrpc](../../../pkg/server/sync_transport.rs)注册GetClusterInfo，classic默认模式返回PD_SVC_MODE、空tso_urls及正确cluster header。两个discovery入口均接受缺失/错误请求cluster ID，允许follower读取；关闭时Unavailable/server not started。GetMembers的存储/解码失败返回成功RPC中的UNKNOWN错误header，不误报成transport失败。
- [RoleServices](../../../pkg/runtime/member_role_worker.rs)通过已有Weak MetaStore引用提供discovery存储，不延长存储生命周期，也不创建第二个服务或复制选举逻辑。
- 为取得真实后端leader，最小扩展[MetaStore](../../../pkg/storage/kv/meta_store.rs)的可选、后端无关`consensus_leader_id`能力；默认不暴露ID。EtcdMetaStore通过已有带超时的client.status读取当前真实leader，再由适配器按ID在PD成员中匹配。领域层没有protobuf、transport或etcd类型。

### 拓扑边界

Go的PD节点同时是embedded-etcd成员；Rust过渡模式的外部etcd进程是独立节点，ID不对应PD注册成员，因此etcd_leader为空。不能把PD leader或外部etcd地址填进去冒充同一PD成员。真实fixture测试另将实际etcd leader ID注册为成员，验证“匹配时返回对应成员”分支；没有固定leader替身。保留任务1/ADR-001对主题10拓扑等价性的限制，不声称实现embedded-etcd替代。MCS API模式及独立TSO服务发现属于计划范围外的服务；HTTP成员管理完整契约仍归任务44，本任务没有改写其路由。

## 行为证据

[grpc_members_test.rs](../../../pkg/server/grpc_members_test.rs)显式注册两个task_019_测试，使用真实etcd、MetaStore事务及TCP/tonic：

1. 两个真实PdNode共享存储，leader/follower均返回完整两成员；缺失/错误cluster ID均可发现，GetClusterInfo模式与header正确。关闭首节点后，第二节点成为真实PD leader，持久注册仍保留两个成员；同一listener的HTTP ping返回200。
2. 多client/peer URL原样返回；gRPC不暴露HTTP发布字段。读取前后MetaStore entries完全相同。真实etcd状态ID与成员匹配时返回正确etcd_leader；损坏成员protobuf或关闭etcd后，返回UNKNOWN错误header，成员和leader不产生部分响应。leader值引用列表外ID时返回None。关闭真实listener并取消宿主后，直接调用两个同一gRPC handler均返回Unavailable；停止检查不依赖已关闭后端。

初始失败测试可编译并实际运行1项：`must discover the full registered membership`，实际1、预期2，退出101。不是编译错误或零测试。

### Go只读差分

[grpc_members_differential_test.rs](../../../pkg/runtime/grpc_members_differential_test.rs)通过任务4监督入口`--case members`启动已有Go二进制的两个真实PD节点，Rust侧启动两个真实PdNode和共享外部etcd。双方核对缺失/错误header、两成员、leader属于列表、正确响应cluster ID、PD_SVC_MODE和空tso_urls；全部gRPC默认字段也逐项断言。

双方成员名都显式配置为compat/request-follower并比较；随机ID及listener端口按成员身份关系/HTTP loopback URL形态归一化。两个URL数组均检查长度和形态；多URL的原样保留由server真实存储测试单独断言。etcd_leader的embedded/外部差异单独明确断言，不能算该字段原值相等。

增强差分还通过真实TCP转发器冻结上游流量、保留下游连接，不修改Go、不使用failpoint。Go用已有advertise-client-urls配置使内部etcd客户端经过转发器，外部PD RPC仍访问真实listener；等待现有1秒成员缓存TTL过期。Rust用同一真实etcd数据上的连接和相同server router。双方各取消一个实际在途的GetMembers（50ms客户端timeout丢弃future），然后下一请求实际等待后端超时：均为成功RPC的UNKNOWN错误header、无成员/leader部分结果，GetClusterInfo仍然成功。Rust前后持久entries相等；转发任务全部abort并join，listener及Go进程正常回收。

错误消息保留双方各自真实后端cause并写入日志（Rust为MetaStore Unavailable，Go为ErrEtcdMemberList/DeadlineExceeded）；比较的是transport成功/错误header类型及响应字段，不宣称两个不同后端的错误字符串逐字相等。损坏Rust注册数据是额外真实边界证据，不冒充Go具有相同Rust注册键。监督报告保存原始输出、二进制及实际调用链哈希，检查Go文件不变及进程/目录回收。已有Go二进制build info无VCS revision，不声称来自当前HEAD。

## 验证记录

所有Cargo阶段均从1–10依次原子mkdir动态领取到槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。锁记录计划、任务、会话与持有PID；阶段退出后释放自己的锁，保留缓存。未使用其他target-dir。默认features，协议仍固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

| 确切命令（仓库根目录） | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_019_ -- --nocapture` | 101 | 0通过/1行为断言失败/0忽略 |
| 最终相同聚焦命令 | 0 | 2通过/0失败/0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 121单元+1集成通过/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all`、`cargo fmt --all -- --check` | 0 | 格式化/静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 1975普通+2doc-tests通过，共1977；14忽略不计通过 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case members` | 0 | 显式运行1通过/0忽略（含成功、真实故障及取消） |

全workspace使用最终生产实现。随后仅加强测试的名称/URL/拓扑断言、HTTP ping和真实TCP故障场景；再次执行格式化、聚焦测试、server check及完整增强差分并通过。没有重复扩展无关验证。

原始命令日志、完整Go守卫、最终来源哈希、两轮监督报告、槽位执行脚本保存在[压缩证据](task-019-evidence.json.gz)。最终监督报告`compat-readonly-1793177021448788000.json`退出0，readonly_unchanged、inputs_unchanged、work_directory_cleaned及command.process_group_cleaned均为true；先前成功路径报告1793176001437460000也全部通过。

开始/结束保护集合1983文件（含ignored/untracked）路径及SHA-256完全相同，Go/模块Git状态均为空；排序JSON文件集合摘要`300a4b0fdad52dd6b67f5daa919a1720ca47c97ecc7a81d364f6896f12d094a2`。plan.md与HEAD原字节一致；Cargo.lock、Go源码和模块文件没有本任务差异。Python AST、Bash语法、文档链接、git diff --check均验证。

所有验证子进程已结束，自己的槽位锁已释放。没有本任务遗留etcd数据目录；保留既有`etcd-contract-23371-3`、`etcd-contract-35880-1`，没有删除其他任务缓存。只提交本任务实现、测试与证据；编号任务19在报告证据后删除，既有编号任务1–18删除保持原样。


证据压缩包SHA-256：`89040eea39657fca7f194c17a4494cae1c67070f8c0d4b31e5121e97de5b67de`。
