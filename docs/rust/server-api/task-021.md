# 任务021：批量 ID 分配

状态：已完成。基准 `e2b3782ed1f05eb7f7ed9bda5e251296a4b87ffb`，依赖任务020及任务1–4清单/只读监督入口已核验。默认 classic PD，外部 MetaStore 过渡架构。Go、Go模块及只读 plan.md 均未修改。

## 实现与来源

[source-inventory.md](source-inventory.md)任务21行、[wire-inventory.md](wire-inventory.md) AllocID owner=21 行由本证据覆盖；[surface-inventory.md](surface-inventory.md)没有独立 owner=21 配置/指标/CLI 行。`handleAllocIDNonBatch` failpoint 属于90/91/92盘点，本次不启用、不改写 Go；HTTP base-alloc-id 仍归任务75。

只读核对 `server/grpc_service.go:703–745`、`pkg/id/id.go:113–137`：RPC count=0按1分配；响应 id 为批次最后一个 ID；仅分配数量大于1时响应 count 非零；allocator 错误返回成功 RPC 中的 UNKNOWN header，id/count 为0；有效上限和预留窗口由领域 allocator 管理。

- [grpc_alloc_id.rs](../../../pkg/server/grpc_alloc_id.rs)实现请求归一化、既有角色校验、取消及错误 header；[grpc.rs](../../../pkg/server/grpc.rs)和既有 SyncGrpc 路径转发到该处理器。
- [PdServices](../../../pkg/server/lib.rs)新增批量接口；[RoleServices](../../../pkg/runtime/member_role_worker.rs)每次取得当前 term，再调用 [Application](../../../pkg/runtime/node.rs)的同一个 canonical allocator。没有循环调用单个ID接口来伪造批量原子性，没有复制领域算法。耗尽错误在宿主边界映射为 Go RFC 文本。
- [Allocator::alloc_count](../../../pkg/id/allocator.rs)已有窗口互斥、持久预留、取消和耗尽能力，本任务未修改。单个ID宿主的默认批量接口仅接受1；实际生产宿主覆盖批量接口。Cargo仅添加已有workspace id/member测试依赖，Cargo.lock无外部包版本变化。

## 行为证据

[grpc_alloc_id_test.rs](../../../pkg/server/grpc_alloc_id_test.rs)显式注册两个 `task_021_` 测试：

1. 真实 etcd + PdNode + TCP/tonic 验证5个批量ID、零值/单个、跨1000预留窗口的1005个ID；12个并发17-ID批次无交叠；持久边界为2000；错误cluster请求被拒绝；同监听器HTTP ping成功。保持持久身份重启后返回2002；把真实持久边界设为MAX-1并再次重启，批量越界返回UNKNOWN、零id/count、RFC错误文本，持久边界MAX，后续单个分配仍报错。
2. 复用真实 Member/Leadership/Allocator；MetaStore测试包装只控制真实外部IO时机，不伪造成功值。只在写入 `/alloc_id` 的事务边界停止真实etcd，验证UNKNOWN及无持久窗口；重启后重试分配2成功。第二个事务边界暂停时取消handler，返回Unavailable，持久边界仍1000；已消耗ID不复用，重试单个返回1001。取消后入站请求被拒绝；leader显式resign。此项直接调用协议处理器，TCP路径由第一项和差分覆盖。

初始聚焦命令实际运行1项，退出101：`batch AllocID must be served`断言看到Unimplemented。不是编译错误。开发中修正了fixture命令名、Rust测试可变绑定、存储Result导入、指标预期，以及把故障门从leader只读校验事务收窄至真正的ID预留写事务；这些中间失败保存在压缩日志中，最终验证使用修正后的测试。

后台调度器会从同一个ID池预留128个ID，所以不同RPC批次之间允许出现空隙；批次内连续且不重复。测试保留并发区间交叠检查；没有要求RPC独占全局ID池。

[grpc_alloc_id_differential_test.rs](../../../pkg/runtime/grpc_alloc_id_differential_test.rs)通过任务4监督入口 `--case alloc-id`启动已有Go二进制和真实Rust节点。实际双方0/1/5/1005请求的末ID均为1/2/7/1012；最后2个ID请求Go返回1014、Rust返回1142（后台调度预留128），比较count、非交叠和单调性质。双方真实重启后均返回2003/count3，证明放弃旧窗口。将各自真实etcd的持久边界设为MAX-1000后再次重启，请求1001，双方均返回 `[1,"[PD:idalloc:ErrIDExhausted]id exhausted",0,0]`；还比较错误cluster状态码及HTTP ping 200。只归一化随机cluster ID和不相关ID消费者造成的批次间空隙；不把Rust额外的MAX-1、事务故障或取消案例宣称为Go同场景差分。

已有Go二进制是 `bin/pd-server`，build info为go1.26.8，无VCS revision；不宣称来自当前HEAD。沿用已有只读fixture构建入口，不执行make/failpoint/Go格式化/模块整理。监督报告 `compat-readonly-1793258408626959000.json` 退出0，readonly_unchanged、inputs_unchanged、work_directory_cleaned、process_group_cleaned均为true。

## 验证

所有Cargo阶段依次原子mkdir动态领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。锁记录任务/计划/会话/PID；阶段结束释放自己的锁，保留编译缓存。默认features，没有额外target-dir。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_021_ -- --nocapture` | 101 | 1行为失败 |
| 最终同一聚焦命令 | 0 | 2通过、0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 125单元+1集成通过 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all`、`cargo fmt --all -- --check` | 0 | 格式化 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case alloc-id` | 0 | 1通过、0忽略 |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 1981通过（含2 doc-tests），16忽略不计成功 |

开工/结束完整Go保护集合均为1983文件（包括ignored/untracked和缓存目录），每文件SHA-256、存在性及Git状态完全相同；Go/模块Git状态为空。plan.md与HEAD字节相同。Python AST、文档链接与git diff --check均通过。仅有既存cpu_usage字段废弃警告，没有禁用检查。

[压缩证据](task-021-evidence.json.gz)保存全部命令及输出、失败/通过测试摘要、Go守卫、监督报告和最终代码哈希。验证进程已结束；自己的槽位锁已释放，缓存保留。只提交本任务变更，保留原有编号任务删除；任务021在报告证据后删除，不改plan.md。

压缩证据SHA-256：`14f95c3d788de17272f60243c58ea5fa8c9d4abcba4fc89227cef67fcc66f49c`。
