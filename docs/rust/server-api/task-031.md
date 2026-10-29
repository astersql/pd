# 任务031：SplitAndScatterRegions 请求

状态：已完成。基准 `9a3c37c68`；依赖批次30及任务1–4准入已核验。协议仍固定 kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`，默认features。

覆盖 [source-inventory.md](source-inventory.md) 的任务31行与 grpc_service.go 方法清单、[wire-inventory.md](wire-inventory.md) 的 classic SplitAndScatterRegions 行。[surface-inventory.md](surface-inventory.md) 的HTTP split（67）、独立MCS（78）和通用转发指标（90–92）保持各自owner；本RPC没有单独HTTP路由。

只读Go oracle：`server/grpc_service.go:2357–2417`，`pkg/schedule/scatter/region_scatterer.go:501–575`，任务30固定的 splitter。组合方法先执行unary角色/cluster ID检查，再核对bootstrap；此顺序区别于SplitRegions。先观察split，再只scatter观察到的ID；group与retry_limit传递，skip_store_limit固定false。空split结果继续进入batch scatter并返回Unknown/`[PD:scatter:ErrEmptyRegion]empty region`；不能误用独立ScatterRegion的空ID单region分支。组合响应只有regions_id、split_finished_percentage、scatter_finished_percentage，没有failed_regions_id；Go丢弃scatter返回的失败ID列表。失败region仍可从已保留的split ID和真实region/operator状态观察，不伪造协议字段。

[实现](../../../pkg/server/grpc_split_scatter.rs) 复用已验证的split/scatter适配器、宿主term worker和canonical coordinator；[Grpc](../../../pkg/server/grpc.rs) 与 [SyncGrpc](../../../pkg/server/sync_transport.rs) 注册同一方法。不新增领域实现、worker、runtime或依赖。取消/关闭沿用两阶段既有所有权及请求丢弃清理。

[真实TCP测试](../../../pkg/server/grpc_split_scatter_test.rs) 与 [共享场景](../../../pkg/server/grpc_split_scatter_scenarios_test.rs) 验证未bootstrap、错误cluster ID、空输入/无可分region/负retry转换、真实USEKEY命令、admission不算完成、100%/100%成功、60秒观察后50%/0%的split部分成功和scatter失败、确切region ID、deadline取消和未凭空生成边界、服务关闭。新region通过同一etcd MetaStore持久化，包级测试同时检查同端口HTTP ping。

[事务故障测试](../../../pkg/runtime/grpc_split_scatter_fault_test.rs) 使用真实TCP、真实etcd和现有ReservationStore，在split边界持久化后向allocator事务注入超大写入，由真实etcd拒绝；断言Unknown错误、allocator值不变、没有新scatter operator，split metadata仍持久存在。第二次split完成后阻塞scatter allocator事务，再关闭服务，验证请求和worker退出。不是mock调用次数断言。split观察/重试时钟未改变，复用并回归任务30的可控时钟证据。

[Go差分](../../../pkg/runtime/grpc_split_scatter_differential_test.rs) 经任务4的[只读supervisor](../../../scripts/pd-compat-readonly.py)运行完全相同场景，对比序列化响应、服务端错误状态/文本、USEKEY命令、查询到的region和取消副作用；包含真实follower角色拒绝。随机cluster ID在断言真实值后归零，返回ID排序只归一化Go map迭代。Go二进制无VCS revision，不声称来自当前HEAD；provenance与原始日志保存到压缩证据。

首次真实差分的业务响应完全一致，但客户端deadline与远端HTTP/2 reset竞态产生不同诊断文本：Cancelled/`Timeout expired` 与 Cancelled/`h2 protocol error: http2 error`。此轮退出101，保留完整失败报告。最终场景分别断言并打印实际诊断，取消项比较实际status code和未产生新边界；只允许已知deadline/reset诊断，不宣称取消文本逐字相等。服务端empty-region与cluster mismatch错误仍严格核对文本。

第二轮差分验证业务结果一致后，真实Go follower返回Unavailable/`not leader`，暴露组合RPC与SplitRegions的bootstrap/角色检查顺序不同。该轮退出101；修正组合方法先执行角色校验，真实follower测试固定该错误码和文本，保留失败报告。

初始测试可编译并通过真实服务进入生成桩，断言Unimplemented失败（101，1失败）；最小实现后同一过滤命令1通过。未人为删除已有实现制造失败。

## 验证

动态原子mkdir从1–10领取槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。owner记录计划、任务、会话和持有PID；所有Cargo及supervisor子进程继承同一路径。完成后释放自有锁、保留缓存。确切命令、退出码和有效测试数见下表。

| 确切命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_031_ -- --nocapture` | 101 | 1个预期断言失败，非编译错误 |
| 最终同一聚焦命令 | 0 | 1通过，0忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_031_ -- --nocapture` | 0 | 1通过，0忽略，含真实事务故障与取消 |
| `cargo test --locked -p astersql-pd-runtime --lib task_030_ -- --nocapture` | 0 | 1通过，0忽略，可控时钟回归 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 137通过，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all`；`cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN="$PWD/bin/pd-server" PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server" bash scripts/run-pd-compat-readonly.sh --case split-scatter` | 0 | 1真实Go差分通过，0忽略，含follower |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 2002通过，26既有/环境项忽略，不计通过 |

最终Go监督报告 `compat-readonly-1793273785063821000.json`：exit=0、readonly_unchanged、inputs_unchanged、work_directory_cleaned和命令process_group_cleaned均为true。前两次失败差分也确认只读守卫及进程组/目录清理通过，失败未被计作通过。Python AST、入口bash语法、证据相对链接和git diff --check通过。

最终开始/结束含ignored/untracked及target的1983个Go/模块文件，其存在性、逐文件SHA-256及受保护Git状态完全一致；plan.md与基准HEAD逐字节相同。自己的槽位1锁已释放，缓存保留，未遗留PD或fixture进程。原有任务1–30删除保持未暂存；任务31完成后按要求删除，不纳入实现提交。

[压缩原始证据](task-031-evidence.json.gz) 包含初始失败、两次真实差分失败、最终通过日志、确切命令/退出码/有效测试数、三个监督报告与差分日志、Go首尾守卫、源码哈希、二进制provenance及槽位执行器。SHA-256：`cb9e18aa688ccadb4d8825ff98c83c181e62b1fe4234a7c9dfa3a6f1e78ed74f`。
