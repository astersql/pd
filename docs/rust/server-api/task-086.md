# 任务086：宿主Prometheus指标与查询/Push证据

基准 `7acd43bec`；2026-11-01。依赖[任务085](task-085.md)及任务1–4已核验。协议仍固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。原始清单行、运行输出、命令、退出码、只读快照及进程清理记录见[压缩证据](task-086-evidence.json.gz)。

## 实现与来源边界

[service_metrics.rs](../../../pkg/server/service_metrics.rs)从宿主Registry导出text 0.0.4，在同一HTTP/gRPC listener提供GET/HEAD `/metrics`，不创建默认Registry、私有runtime或平行服务。注册失败逐项撤销本次注册；[node.rs](../../../pkg/runtime/node.rs)在任何diagnostic/member持久化绑定前完成注册。router及宿主等待的worker共同持有scope，退出仅注销scope自己的collector。宿主collector数值与生命周期保持宿主所有。

| 来源 | 接线与验收 |
| --- | --- |
| `pkg/basicserver/metrics.go` | info使用真实启动时间/版本/hash；maxprocs与memory_quota由宿主提供，验证schema及透传，不把测试配额冒充OS探测 |
| `server/metrics.go` | 23个family；TSO、region query/lookup错误、region/store/bucket heartbeat测量接入既有真实领域路径；name/help/type/labels及histogram buckets逐项比较 |
| `pkg/utils/metricutil` | 宿主等待的Push worker、delimited protobuf、job/instance分组编码、BasicAuth、错误重试、禁用配置、30s超时及取消 |
| HTTP-147–150 | GET/POST query/query_range共用既有router及service middleware，真实上游HTTP/HTTPS/mTLS及安全错误 |
| `pkg/runtime/node.rs` | 真实启动、冲突回滚、重复启动、取消、宿主collector保留；显式绑定真实etcd member才发布raft状态 |
| 其余共享来源 | source/surface中所有owner86原始行已归档；Registry透明导出领域已注册collector，不在本任务复制领域指标实现；Pyroscope及独立MCS能力不在此宣称 |

`server/metrics.go`的23项加info共24个family：其中20项在相同RPC trace后直接比较实际Go/Rust exposition，另外4项（审计2项、并发及raft）分别通过HTTP及绑定etcd验证。另用共同HTTP探针比较info、审计2项及宿主进程2项共5个family。成功与错误RPC的16项counter/histogram count/sum观察一致；耗时绝对值、构建信息、启动时钟不要求相等。

MCS专属TSO proxy/forward指标保留Go构造时冷状态与固定forward标签，不代表独立MCS服务已经实现。Go实际调用点显示forward_fail属于独立调度服务转发；双节点PD互转发错误的真实反例验证这些counter不误增。外部etcd的term/appliedIndex/committedIndex来自显式绑定的maintenance/status，不将外部MetaStore拓扑说成Go嵌入式etcd。

所有外部caller归并anonymous/unknown，服务/方法/事件使用固定集合；store address/id只来自可信membership，变更地址或删除store会清理旧系列。缓存child handles，成功region请求沿用1%采样，错误全记。metadata沿用Go构造的零sentinel。墙钟回退通过100ms宿主监控及可控时钟测试。并发每15s采集实际区间峰值，HTTP admission guard延续到响应body结束/丢弃。

## 查询与Push传输

[http_metric.rs](../../../pkg/server/http_metric.rs)保留GET/POST方法、原始query与body；清除hop-by-hop及转发身份头，按Go行为协商/解压gzip，成功仅输出JSON/cache头，剥离上游cookie及trailers，错误返回安全502 JSON。[metric_transport.rs](../../../pkg/server/metric_transport.rs)验证全部DNS结果后固定拨号地址，拒绝loopback/link-local/multicast/保留网段及metadata目标；不走环境代理、不追随重定向。请求自己轮询连接driver，超时或取消释放socket，无后台连接任务。

查询TLS使用启动时CA与逐握手更新的证书/密钥；独立`with_telemetry_config`可在HTTP listener后连接HTTPS/mTLS上游，保留原listener策略。Push使用Go默认客户端对应的系统根，不错误继承查询客户端身份。[metric_proto.rs](../../../pkg/server/metric_proto.rs)适配已固定Go client_model v0.6.1的全部普通metric类型，复用prost，不启用prometheus可选protobuf feature。

[协议fixture](../../../scripts/service-metrics-fixture.py)是真实TCP/TLS受控上游，独立解析Push protobuf及验证dummy BasicAuth；不是Prometheus服务器或领域服务的替代实现。[共享探针](../../../scripts/service-metrics-probe.py)和[只读supervisor](../../../scripts/service-metrics-oracle.py)运行empty/http/denied/https/bad-ca/bad-host六组Go/Rust案例及RPC差分；包含503后成功Push、取消socket EOF、gzip、重定向/错误安全化、并发峰值2（上限10）、上限2时第三流429及取消后再准入。

既有Go二进制SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`，go1.26.8/without_dashboard/darwin-amd64，无VCS revision；不宣称来自当前HEAD。RPC长流观测指标后取消，TSO错误统一核对最终gRPC status，兼容Go在headers前或message阶段报错的位置。Go全Registry的runtime/embedded-etcd/独立MCS额外family不与Rust外部MetaStore拓扑强行等同。

## 验证

所有Cargo及其子进程通过原子mkdir动态领取slot-1，绝对`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；`CARGO_BUILD_JOBS=4`、`RUST_TEST_THREADS=4`，默认features。每阶段结束清理自有PID记录与锁，保留缓存。

最初运行因外部`sudo rm -rf ./target`退出1，仅算环境失败。随后取得可编译运行的真实失败：缺导出行为、注册顺序造成持久化残留、缺RPC测量均退出101，修复后通过。一次主动中断的旧包回归退出130，不算通过。完整workspace首轮在既有ID生命周期断言收到129而非1退出101，保留原始失败，不隐藏或放宽断言；最终回归结果见下表及证据。

| 确切命令（全部继承上述槽位） | 退出码 | 有效测试数 |
| --- | --- | --- |
| `cargo test --locked -p astersql-pd-server --lib task_086_ -- --nocapture` | 0 | 10通过，0忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_086_ -- --nocapture` | 0 | 2通过，0忽略 |
| `cargo test --locked -p astersql-pd-ratelimit -- --nocapture` | 0 | 30通过 |
| `python3 scripts/service-metrics-oracle.py`（`PD_COMPAT_GO_BIN=$PWD/bin/pd-server`） | 0 | 6组传输+1组RPC；7项Rust测试通过，20项schema比较 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 281通过，39项既有ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据检查 |
| `cargo fmt --all`；`cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 编译检查 |
| `cargo check --workspace --locked` | 0 | 编译检查 |
| `cargo test --workspace --locked`（最终重跑） | 0 | 2191通过，76项既有ignored，0失败 |


全仓1101个Go/模块文件的存在性、SHA-256及Git状态前后完全一致（含untracked），未运行改写Go的make/failpoint命令。`plan.md`与基准字节相同，SHA-256 `a59eba65612e814ada50274525cbb41a81df792f2d2966c2e60a2deae5d84615`。保留用户原有prompt.md差异；任务完成后仅删除本编号任务文件。无遗留测试进程、私钥临时目录或自有槽位锁。
