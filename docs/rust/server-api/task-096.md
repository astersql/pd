# 任务096：宿主嵌入隔离

基准 `73a6fb009`；依赖 [任务095](task-095.md) 的关闭装配及证据已满足。
复用 `PdNodeBuilder` / `PdNode`，生产行为已有，无需新增平行隔离管理器或
`embedded_isolation.rs`。新增独立 [测试](../../../pkg/runtime/embedded_isolation_test.rs)，
在 [lib.rs](../../../pkg/runtime/lib.rs) 显式注册。

## 来源与范围

[source-inventory.md](source-inventory.md) 第134行任务96、994行ADR、1339行
node_test.rs 来源全部对应本次测试；共享 node.rs 生命周期/worker 行沿用095。
[wire-inventory.md](wire-inventory.md) 无任务96专属协议，复用 GetMembers、AllocID
和 health 的既有协议归属。[surface-inventory.md](surface-inventory.md) 中宿主
listener/Registry共享来源适用，不增加配置、CLI或指标；标记主题10的嵌入etcd拓扑
仍不在本任务验收范围。前三份清单的历史manifest哈希不是本次运行通过数。

只读Go参考 `server/server.go::Close` 的节点停止、后台join与客户端释放；
ADR-001的宿主Handle、子取消令牌、显式Registry边界是Rust专属契约。
现有 `node.rs` 使用传入Handle启动初始化及服务、子token、显式Registry；
`PdNode::shutdown` 仅取消自身并join。生产库源码扫描禁止全局安装/私有runtime的
常见调用，扫描集合、表达式与结果随压缩证据保存；这是源码核查，不冒充动态差分。

## 行为证据

`task_096_behavior`：一个单线程Tokio宿主、同一真实etcd MetaStore、两个独立
cluster namespace、两个真实TCP listener、两个宿主Registry，使用同一个Handle和
父token。两节点同时就绪并分配ID。关闭A且重复关闭后：A停止准入、端口可重绑、
leader key删除；B真实AllocID继续递增、错误cluster返回FailedPrecondition、HTTP health
为true，leader revision不变、持久alloc_id仍存在；宿主ping任务继续响应，父token
未取消。A的Registry只剩宿主collector，B仍保有节点指标。关闭B后两个Registry均
保留宿主collector，全局Registry与开始相同；最终MetaStore强引用计数为1，宿主任务join。

`task_096_failed_start_and_host_cancel`：在上述双节点存活期间，用B的Registry启动
第三节点，实际collector冲突导致启动失败；失败listener关闭且新namespace无持久写入，
A/B仍可分配ID。关闭A后B与宿主继续服务，最后由父token取消B并join；相同资源守卫通过。
不模拟网络、不使用mock调用次数；未引入人工时钟或故障返回值。

首轮可编译测试退出101、0通过/2失败，失败在同步curl阻塞单线程宿主的HTTP检查。
这是测试自身错误，不作为生产缺陷或TDD红灯证据。改为spawn_blocking后现有生产行为
通过；任务允许已有行为仅补验证，因此不人为修改生产代码制造失败。

只读supervisor新增 `--case embedded-isolation`，显式运行默认忽略的
`task_096_readonly_differential`。已有Go二进制启动两个独立PD进程，在关闭A后验证
A停止准入，B的ID继续递增、HTTP健康和错误cluster状态；再执行两套Rust宿主场景。
Go进程有各自embedded-etcd，Rust双实例共享外部MetaStore的不同cluster namespace；
仅对照独立节点的可观察关闭/存活契约，不宣称Go同宿主Handle/Registry或拓扑等价，
不把源码推导当作Go运行结果。Go二进制build info/hash随报告保留，不声称由当前HEAD构建。

## 验证记录

所有命令从仓库根执行，默认features，`CARGO_BUILD_JOBS=4`、`RUST_TEST_THREADS=4`。
各阶段原子mkdir动态领取 **slot-1**，记录任务/会话/PID；实际
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
阶段结束后删除自己的owner记录并rmdir释放锁，保留编译缓存。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-runtime --lib task_096_ -- --nocapture` | 0 | 2通过、1默认忽略；该忽略差分另行执行 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 82通过、38默认忽略 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据，stdout保存到临时文件 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 包级检查 |
| `cargo check --workspace --locked` | 0 | workspace检查 |
| `cargo test --workspace --locked` | 0 | 2221通过、78默认忽略，含2项有效文档测试 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 当前Rust进程产物 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server" bash scripts/run-pd-compat-readonly.sh --case embedded-isolation` | 0 | 1通过、0忽略；含真实Go双进程和两套Rust双实例场景 |

对照入口首次退出101，原因是新case和supervisor均添加`--nocapture`，libtest拒绝重复参数，
实际行为测试0项；移除case里的重复参数后重跑通过。该入口错误不作为生产行为失败。
Python AST、证据文档链接和`git diff --check`检查通过。
生产Rust库627个文件的常见全局安装/私有runtime调用扫描0命中，详见原始表达式及文件集合。

[压缩证据](task-096-evidence.json.gz) 保存完整命令/输出/退出码、Go二进制哈希与build info、
supervisor只读守卫及进程/目录清理结果、源码摘要和本次清单行。
Go/模块文件（包含ignored/untracked）共1101个，开始/结束的路径、SHA-256和Git状态
完全相同；只读plan.md哈希不变。保留原有任务94/95删除及prompt.md变更，未纳入本任务提交。
所有自有测试进程及槽位锁已释放，缓存保留；无Go/模块文件修改。

状态：已完成。
