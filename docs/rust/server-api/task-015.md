# 任务015：驱动集群指标采集 worker

状态：已完成（HTTP成员健康采集、宿主任期接线及Registry生命周期）。

基准 HEAD：`8c8f356d4c00640a8cf2d67c2d590f4c587c0730`。依赖任务14的[接线与验证证据](task-014.md)及任务1–4清单/只读入口已核查；协议固定 kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

## 来源与边界

- source-inventory.md 的任务15整行、`state_pipeline.rs`共同owner行、`cluster.go:484`启动行：复用同一宿主/MetaStore和任期取消令牌，没有建立第二个集群缓存。StatePipeline保持心跳/统计领域组合职责，成员protobuf转换只在runtime适配层进行。
- Go `server/cluster/cluster.go:907–929,2332–2362,2891–2944`：10秒后首轮、逐成员逐URL、GET `/pd/api/v1/ping`、`PD-Allow-follower-handle: true`、每URL三秒超时、HTTP200为健康。失败URL回退；成员读取失败不覆盖既有指标；停止时清理健康指标。`pkg/schedule/coordinator.go`不负责本job启动，其调度统计采集不合并进本job。
- surface-inventory.md 的healthStatusGauge声明及注册行：保留`pd_cluster_health_status{name}`的Gauge定义、帮助文本与值语义。其他15/86/101共享来源行仍归其实际owner，本次不宣称完成全量Prometheus导出。
- wire-inventory.md无该内部worker专属RPC；本次补齐其依赖的ping路由，通过真实HTTP观察健康，通过共享MetaStore读取成员身份，不新增管理接口。

Rust沿用现有`/pd/<cluster-id>/rust/members/`持久成员身份来源；Go使用embedded-etcd MemberList。差分只比较同一单成员场景的实际健康指标，不宣称两种成员发现拓扑等价。宿主尚未装配Store进度Manager，因此没有进度对象可供本worker重置；不注册空进度指标，也不重置别的owner的指标。HTTPS/TLS配置及通用HTTP重定向属于后续传输配置范围，本次明确验证plain HTTP ping端点。

`ClusterMetricsWorker`在宿主executor运行，10秒周期使用Skip策略；成员读取和HTTP都在同一可取消future内。停止取消并join，drop也发送取消；collector注册守卫负责正常退出及panic时注销自己的family，保留Registry中其他owner。成员名的Gauge句柄缓存在周期任务内；保持Go对旧成员名series的保留语义，任期结束统一注销。没有全局Registry、私有runtime、后台连接驱动或Go文件写入。

## 行为证据

先建立可编译入口、真实HTTP ping测试和空探测实现。要求的聚焦命令退出101，实际1失败/0通过/0忽略，断言`real HTTP healthy member must update registry`，实际0、期望1；未将编译错误算作失败验证。此前一次测试编译因Prometheus protobuf Gauge访问器误写为`value()`失败，修正为`get_value()`后才取得上述行为证据。

最终聚焦测试使用可控时钟和真实TCP/HTTP，验证10秒前无样本、无效URL回退、健康1→HTTP503后0、停止注销；成员读取错误、重复注册失败及无关collector保留；请求途中取消后socket EOF；三秒超时后socket EOF并发布0。runtime测试使用真实etcd MetaStore与真实listener，实际读取已持久化成员并探测ping，Registry为1；shutdown后Registry为空。读取成员与探测都没有持久化写副作用，沿用既有成员注册事务。

只读dual监督入口启动已有Go `bin/pd-server`，真实GetMembers RPC取得集群ID，再真实Bootstrap初始化集群，等待10秒job后通过HTTP `/metrics`观察`pd_cluster_health_status{name="compat"} 1`，与Rust真实场景相同。只归一化集群ID/端口/时间原点。Go同故障/取消差分未运行；故障、超时及取消结果来自本次Rust真实网络测试，不冒充Go差分。

## 验证记录

所有Cargo阶段均从仓库根动态原子mkdir领取槽位1，记录任务、会话及PID；实际`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，没有target-dir覆盖。阶段结束释放本任务锁、保留缓存。

| 确切命令 / 阶段 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始`cargo test --locked -p astersql-pd-cluster --lib task_015_ -- --nocapture` | 101 | 1行为失败/0通过/0忽略 |
| 最终相同聚焦命令 | 0 | 4通过/0忽略 |
| `cargo test --locked -p astersql-pd-cluster -- --nocapture` | 0 | 65通过/0忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_015_ -- --nocapture` | 0 | 1通过/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all`、`cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-cluster` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建，不计行为 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case cluster-metrics` | 0 | 1真实双实现场景通过/0忽略 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked` | 0 | 1961通过（含2项doc-test）/10忽略 |
| Python AST、文档局部链接、`git diff --check` | 0 | 静态 |

原始日志、完整dual监督报告、实际argv/退出码/测试摘要、源码SHA-256及Go守卫快照见[压缩证据](task-015-evidence.json.gz)。Go既有二进制SHA-256为`b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`；build info没有VCS revision，不声称来自当前HEAD。

初始工作树Git状态无Go差异，初始源码守卫与结束守卫1101个非target Go/模块文件路径和SHA-256完全相同，包含untracked/ignored文件。为补全target缓存内的Go文件，另核对任务14最后一次只读报告的1983文件完整快照与本任务dual开始快照完全相同；本任务dual前后完整1983文件及Git状态也完全相同。没有将仅1101文件的初始守卫冒充全仓快照。dual的readonly_unchanged和inputs_unchanged均为true，相关进程组及独占工作目录清理通过。

未修改plan.md、Go文件、Go模块或Cargo.lock；保留任务1–14原有删除，不提交其他任务变更。
