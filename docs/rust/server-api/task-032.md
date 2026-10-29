# 任务032：全局配置读写

状态：已完成。基准 `c89211d3b`；批次31及任务1–4的清单、准入和只读oracle入口已核验。协议保持 kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`，默认features，无Cargo依赖变更。

覆盖 [source-inventory.md](source-inventory.md) 的任务32、grpc_service.go公开方法清单及keypath辅助来源行；[wire-inventory.md](wire-inventory.md) 的 StoreGlobalConfig/LoadGlobalConfig 两行。[surface-inventory.md](surface-inventory.md) 的通用限流/审计/指标仍归79–81/90–92；这两个RPC没有新增HTTP路由，WatchGlobalConfig归任务33。

只读Go oracle：`server/grpc_service.go:2430–2548`、`pkg/errs/errno.go:87`、`pkg/utils/keypath/resource_group.go:25`；实际调用方 `client/client.go:1375–1420`；已有Go测试 `tests/integrations/client/global_config_test.go` 的路径拒绝、literal路径、controller前缀、nested路径、payload和缺失项案例。未执行需要failpoint的Go测试，未新增Go工具。

[实现](../../../pkg/server/grpc_global_config.rs) 使用宿主已有的共享 `discovery_store`，复用已有 `controller_config_path()`，没有复制配置领域服务、创建私有runtime或worker。与Go一致，不检查raft bootstrap或leader，不使用leader term专属的cluster_config_store。Grpc与SyncGrpc均注册同一实现。

契约：空路径及 `/global/config` 归一到 `/global/config/`；其余路径必须具有该字面前缀。名称仅按字符串拼接，空名称只移除一处结尾斜杠；不清理点段、双斜杠、反斜杠或百分号。默认前缀不读取 `/global/config-sibling`。只有无名称的精确 `resource_group/controller` 请求保留旧prefix查询，结果可包含controller-other；直接请求该sibling、child或带名称请求均拒绝。无名称与空repeated字段在真实protobuf wire上相同。named模式保留输入顺序、名称、逐项错误及revision=0；prefix模式返回完整key、字节序和真实后端revision。PUT优先非空Value，否则使用原始Payload；DELETE与PUT在同一MetaStore事务提交。

[真实TCP测试](../../../pkg/server/grpc_global_config_test.rs) 验证未bootstrap即可读写、同端口HTTP ping、服务重启后的读取、直接MetaStore字节值、controller旧前缀及边界拒绝。独立缺失后端测试固定Unavailable及完整消息，并核对其先于路径校验。真实etcd拒绝超大事务后，两个写入均不存在；读取故障通过在真实backend read边界请求future revision产生，响应中间项Unknown且后续项继续返回。deadline与宿主shutdown均实际取消挂起的存储读取并验证future被drop、服务join完成。注入器只控制真实IO，不以mock调用次数作验收。

[共享差分场景](../../../pkg/server/grpc_global_config_scenarios_test.rs) 经[运行测试](../../../pkg/runtime/grpc_global_config_differential_test.rs)和任务4[只读supervisor](../../../scripts/pd-compat-readonly.py)对真实Go二进制运行。比较serialized成功响应、缺失项错误消息、拒绝路径状态/消息、DELETE副作用、literal路径、嵌套prefix及事务失败。prefix的绝对revision只在断言大于0后归零，named revision保持0；不归一化业务字节或错误。Go二进制无VCS revision，不声称来自当前HEAD；完整build info和哈希保存在监督报告。

差分先暴露了不同请求上限：Go PD默认150MiB，fixture etcd为1.5MiB；2MiB请求又触发Go etcd client的ResourceExhausted。1.6MiB在默认Go配置可成功，不能拿它与fixture的失败声称相同场景。最终通过临时非Go TOML将Go的max-request-bytes显式设为1572864，匹配fixture；文件位于supervisor自有工作目录并清理。配置对齐后，Go包装后的backend错误为Unknown/`etcdserver: request is too large`，而非原始etcd InvalidArgument。最终适配保留该错误码和消息；[MetaStore](../../../pkg/storage/kv/meta_store.rs)增加backend-neutral的RequestTooLarge分类，[etcd适配](../../../pkg/storage/kv/etcd_store.rs)只识别真实后端错误，协议类型不进入领域层。不使用前置固定大小判断伪造事务失败。

Go还会在启动及成员加入时初始化/重写resource-group controller配置，属于主题8而非任务32。最终差分读取真实Go初始化后的controller字节，将它与已知sibling通过真实MetaStore事务作为两边共同fixture，在加入成员后重新固定输入。完整controller payload仍参与serialized响应比较，没有删字段或忽略内容；oracle实际字节打印于原始日志。本任务不声称Rust实现了resource-group初始化。

初始真实服务测试可编译并在Unimplemented断言失败（101，1失败）；随后重启测试更换地址触发IdentityConflict，改为复用原listener地址。真实Go差分的五次失败保留原始日志，记录client上限差异、backend配置差异、包装后错误码差异、controller初始数据差异及加入成员时controller被重写，均不计通过。

全workspace回归通过后仅进行等值常量复用和测试导入整理；随后重新运行聚焦、server包级、格式/检查/构建及真实Go差分。全workspace日志保留在该局部重构前的检查点，不将它冒充重新运行。

## 验证

从1–10依次原子mkdir动态领取槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。owner记录计划、任务、会话和持有PID。各阶段结束后移除自己的owner和锁，缓存保留。所有Cargo及supervisor子进程继承该绝对路径，未使用--target-dir覆盖。

| 确切命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_032_ -- --nocapture` | 101 | 1个行为断言失败，非编译错误 |
| 最终同一聚焦命令 | 0 | 3通过，0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 140通过，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all`；`cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN="$PWD/bin/pd-server" PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server" bash scripts/run-pd-compat-readonly.sh --case global-config` | 0 | 1真实差分通过，0忽略；含加入成员场景 |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 2005通过，27既有/环境项忽略，不计通过 |

最终监督报告 `compat-readonly-1793277965546101000.json`：exit=0；readonly_unchanged、inputs_unchanged、work_directory_cleaned及命令process_group_cleaned均为true。五次失败差分同样通过只读与清理守卫，失败结果没有算作通过。Python AST、入口bash语法、文档链接、git diff --check及暂存diff检查通过。

开始/结束包含ignored/untracked及target的1983个Go/模块文件，存在性、逐文件SHA-256和受保护Git状态完全一致。plan.md与基准HEAD相同；没有Go或模块文件变更。自有槽位锁已释放，未遗留PD/fixture进程，缓存保留。原有编号任务1–31删除保持未暂存，任务32完成后按要求删除，不纳入实现提交。

[压缩原始证据](task-032-evidence.json.gz)包含初始失败、差分失败与通过日志、确切命令/退出码/有效测试数、全部监督报告、Go首尾守卫、源码哈希、二进制provenance和槽位执行器。SHA-256：`cd0e7739d0d1eceffa6b18be011a5225ca44862144306cf993656c9ff5225ac0`。
