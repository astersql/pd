# 任务075：ID恢复管理HTTP

状态：已完成。

基准 `5e64dfffee6dc68cf73353631ce84d7562502095`；依赖[任务074](task-074.md)和[任务004](task-004.md)已核验。协议固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。Go及模块文件、plan.md只读。

## 来源、边界与实现

[source-inventory.md](source-inventory.md):113为本任务摘要，owner包含75的全部91行已逐行核查：192–201、237、267、346–399、497–499、2493–2512、2574、2610。[wire-inventory.md](wire-inventory.md):268的HTTP-136为本次行为。[surface-inventory.md](surface-inventory.md)没有独立owner=75行。

这些来源存在整文件共享归属：192–201、267、346–399、2493–2512、2610属于TSO控制/worker/failpoint/哈希，继续由35/36/76及独立MCS领域验收；497–499是73的缓存管理。237/2574仅其中recoverAllocID与本任务有关。没有将TSO或缓存管理的共享来源算作本次新增覆盖，也未修改来源史诗。审计与限流仍由81/82验收。

只读Go oracle：[admin.go](../../../server/api/admin.go)的recoverAllocID先解码JSON对象，要求非空字符串id，用十进制uint64解析；然后检查snapshot-recovering标记、leader，调用[server.go](../../../server/server.go)的RecoverAllocID和[id.go](../../../pkg/id/id.go)的SetBase。未bootstrap也可恢复；没有标记返回403；非法字段/数字返回400；MAX本身在分配器层返回500及PD耗尽错误。

[HTTP适配器](../../../pkg/server/http_admin_recover_id.rs)复用共享JSON/整数解析、标记读取和成员发现；[路由](../../../pkg/server/http.rs)沿用已有请求上下文、取消和leader转发。通过[PdServices](../../../pkg/server/lib.rs)的窄接口，[RoleServices](../../../pkg/runtime/member_role_worker.rs)获取当前活动term，[Application](../../../pkg/runtime/node.rs)调用同一持久Allocator::set_base。与RPC共享分配窗口锁、leadership事务及MetaStore；没有另建分配器或在transport层复制领域算法，也没有依赖/feature/协议变更。

兼容语义：Go显式SetBase允许降低或重复base，因此管理恢复可能重用此前ID；本任务保留该行为。“防回退”证据对应普通分配及恢复后的持久预留窗口在重启后不回退，不声称显式管理员重置仍全局单调。已有领域层的uint64边界算术契约未改写。

## 失败与通过

[独立测试](../../../pkg/server/http_admin_recover_id_test.rs)在lib.rs显式注册，全部task_075_前缀。初始测试可编译运行，但HTTP尚未接线，请求返回`(200, "", "application/grpc")`，预期400；退出101，1失败、0通过、1忽略。未用编译错误充当行为失败。

真实TCP HTTP、tonic和真实etcd覆盖：缺字段/null/数字id/空字符串、符号语法、超过uint64、未标记403、未bootstrap成功、MAX耗尽、显式降低base、随后RPC从指定base+1分配；32个并发RPC分配无重复；持久边界按1000窗口保存；普通分配在重启后越过旧预留窗口；真实第二节点转发恢复且leader的后续RPC观察到新base。

真实事务故障：在分配器实际事务追加不存在lease的写入，使etcd拒绝整笔事务，HTTP500且持久边界不变；随后正常恢复及RPC成功。标记读取故障由实际未来revision请求产生HTTP500，分配器边界保持。取消测试在真实分配事务IO边界暂停，shutdown取消请求、退出worker，核对HTTP500、持久边界不变，并重启确认可继续分配；没有用mock调用次数验收。

[只读supervisor](../../../scripts/http-admin-recover-id-oracle.py)复用任务4的Go文件守卫、环境约束与进程组清理，仅使用已有bin/pd-server，不构建Go服务、不运行Go测试或failpoint。双实现比较九组HTTP状态、原始正文和Content-Type（仅大小写归一），另核对成功恢复、显式降低base的RPC副作用、重启后越过预留边界，以及真实第二成员转发后的分配结果。

扩展差分的首次运行因测试先停止二成员etcd中的第二成员，再执行bootstrap而失去多数派，断言失败于共享prepare的bootstrap响应。修复测试顺序为单节点bootstrap及重启验证在前、加入第二成员及转发验证在后；服务实现不因此变更。原始失败日志一并保留。

Rust使用外部etcd，Go已有二进制使用embedded-etcd；不把拓扑、崩溃恢复或本地生产后端durability宣称等价。Rust底层事务/读取错误与取消验证不是Go failpoint差分，不宣称底层错误字符串一致。Go二进制build info缺VCS revision，不声称来自当前HEAD。

## 空间恢复与共享槽位

开发期重跑时磁盘耗尽，真实etcd fixture在写ready.tmp时失败（1通过、1失败、1忽略，退出101）；这不是行为失败证据。先确认本任务7个fixture PID已退出且lsof无打开引用，清理对应临时二进制，共268,897,216字节。编译缓存保留。

在独占slot-1阶段，对651组Cargo对象执行APFS透明压缩，回收6,442,967,040字节。每组先核对全部硬链接均在持有槽位内，再用ditto --hfsCompression --noclone创建压缩文件，核对长度及SHA-256后原子替换全部已知链接并重新核对inode一致；未修改文件内容、删缓存或触碰其他槽位。逐组路径、SHA-256和物理块用量随原始证据保存。

回归期间可用空间再次下降，第二次压缩尝试与Cargo integration测试触发的旧对象清理发生竞态，因旧路径消失而中止。该次不作为完整缓存校验证据；停止后server全包及workspace check均退出0。后续缓存操作只在Cargo及使用产物的进程全部停止后进行。源码未因缓存操作改变。

每个构建/验证阶段都从1–10原子mkdir动态领取，实际均获槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。记录plan/task/session/持有shell PID，Cargo、supervisor和全部子进程继承绝对路径，不使用--target-dir。默认features，验证环境CARGO_BUILD_JOBS=2、RUST_TEST_THREADS=1、GOMAXPROCS=2。Rust修改后先cargo fmt --all。

首轮workspace测试因空间再次耗尽而退出101；runtime的真实fixture连续报告failed to create WAL（该crate 14通过、43失败、36忽略），并有metadata backend unavailable。整条命令累计560通过、43失败、37忽略，原始日志保存；不将这些环境失败当作本任务行为失败。确认Cargo及槽位子进程全部退出后重新动态领取slot-1；未发现遗留临时集群可清理，随后在停机窗口对1694组未压缩对象做同样的硬链接/长度/SHA-256校验，额外回收3,896,242,176字节。两次完整可审计压缩共2345组、10,339,209,216字节，缓存保留。重新领取槽位后重跑完整workspace检查与测试。

## 最终验证

| 确切命令 | 退出码 | 有效测试数量 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-server --lib task_075_ -- --nocapture` | 0 | 2通过、0失败、1忽略 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/http-admin-recover-id-oracle.py` | 0 | 1通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 253通过、0失败、36忽略 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 2154通过、0失败、73忽略 |

server包包含252个单元测试及1个集成测试；0测试doc-test不计行为证据。本任务默认忽略的Go差分已单独显式执行通过，其他既有忽略项不宣称通过。


最终全部1315项Rust/Cargo输入在完整回归前后SHA-256一致；1101项Go/模块文件（含ignored/untracked）的存在性、内容SHA-256及Git状态与开始完全一致，plan.md字节相同。Python AST、Markdown链接、gzip/JSON往返及git diff --check检查通过。

所有本任务服务、worker、Go差分进程组及槽位产物进程均已退出，自己的slot-1锁已释放，编译缓存保留。在前述7个临时二进制之外，另核对37个fixture持有PID已退出且lsof无打开引用后清理，共1,421,313,856字节。任务文件标记完成后按要求删除；原有prompt.md变更保留在提交之外。

真实Go二进制SHA-256：`b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`，build info为go1.26.8且无VCS revision。

[压缩原始证据](task-075-evidence.json.gz)，SHA-256 `aebc5fe1c29ee7f88145a30c74fee52c97ac5ff95366a3aaf4a860899f056384`。包含初始行为失败、开发期失败与环境失败、最终确切命令/输出/退出码/有效测试数、Go只读差分与进程组清理、Go及Rust输入保护集合、来源行映射、完整缓存压缩哈希记录及临时fixture清理结果。第二次并发压缩中止日志保留，不冒充完整缓存校验证据。
