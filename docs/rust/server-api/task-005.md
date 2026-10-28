# 任务005：接入共享集群状态

状态：已完成（本任务共享集群状态边界）。

基准HEAD：`f2ed3762f`。已读取只读plan.md、任务5、任务1–4证据和三份清单；任务4入口及真实Go二进制存在，主题6准入范围以task-001为准。Go/proto版本不变，不修改plan.md、来源史诗或Go源码/模块。

## 实现与来源边界

- `PdNodeBuilder::with_cluster`注入canonical BasicCluster；初始化在宿主Handle上创建唯一ClusterHost。Application的Store/Region心跳适配和`PdNode::cluster().snapshot()`共享同一HeartbeatEffectsState及StatePipeline；没有另一份缓存或领域实现。
- ClusterHost复用已有region admission、HotStat、三条ConcurrentRunner、StorageEndpoint和Downstreams；同一MetaStore用于成员/TSO及region保存。Store心跳复用apply_store_heartbeat；region heartbeat接受后异步保存，不把非致命持久化副作用错误变成准入回滚。
- 关闭先停止网络准入、排空连接，再取消/join所有runner/hot/bucket worker，释放共享资源，最后释放leadership和请求指标。持有读handle不延长关闭后的MetaStore所有权；测试以Arc强引用数量验证。停止后snapshot拒绝服务。
- bucket worker现在持有其Registry/collector，在join后注销，并在Drop时清理。否则同一宿主Registry重启会发生重复注册。已有指标名、HELP、TYPE、labels及更新逻辑不变。没有全局Registry/runtime。
- 独立测试在runtime/lib.rs显式注册。node_test只将已有fixture模块开放给同级测试复用，避免同进程两份计数器造成fixture数据目录碰撞。

来源映射：source-inventory的任务5行及“Application未装配共享状态”缺口由本任务覆盖；wire-inventory各行的“5–7集群角色”只解除本任务共享状态部分；surface-inventory共同准入和listener/宿主Registry行仅覆盖本次生命周期与注销。RPC-StoreHeartbeat/RegionHeartbeat的完整owner、读查询RPC/HTTP、bootstrap事务、角色切换、全量指标与配置仍由各自后续任务验收。本任务读状态通过宿主snapshot；Go侧用GetStore/HTTP读适配观察相同结果，不把Rust尚未实现的GetStore/GetRegion路由声称为通过。

## TDD与真实差分

初始独立测试可编译并实际运行，真实TCP StoreHeartbeat得到Unimplemented：`store heartbeat state service is not configured`，断言Application须暴露canonical state service失败（退出101，0通过/1失败）。此前最小新接口的字段类型编译错误不计失败行为证据。

最终聚焦测试覆盖：注入store后心跳容量100可见；未知store错误header；region 7/version 2/2MiB心跳经真实TCP接入共享缓存，并在真实fixture etcd读到同一RegionMeta；陈旧version 1被拒且状态保持2；真实HTTP健康；关闭后读拒绝、监听器释放、宿主取消不受子取消影响；同身份/同Registry重启及宿主取消；启动bucket指标冲突时释放请求collector/leader/listener且不破坏调用方已有HotStat，清理后重启成功。相邻原有测试还覆盖lease revoke导致worker失败并停止准入。

只读入口新增`--case cluster-host`，强制dual，通过当前源码编译的runtime测试装配Rust节点并启动已有Go oracle；与既有health-member/tso共用Go哈希守卫、输入哈希、进程组/临时目录清理。不通过新Go源码或重新构建Go oracle取证。Go使用显式`[pd-server] use-region-storage=false`选取与本任务相同的etcd region持久化边界；默认LevelDB不是本任务的等价性声明。Rust注入store，Go通过已有bootstrap创建同一先决状态，bootstrap本身不计本任务差分验收。

双实现归一化随机cluster/member/listener值；比较真实容量、region ID/epoch/大小、持久化、未知store错误、陈旧region错误和HTTP健康。结果均为：

```json
{"capacity":100,"healthy":true,"missing_store_error":true,"persisted":true,"region":7,"size":2,"stale_error":true,"version":2}
```

Go成功RegionHeartbeat没有ack且流头延迟到首个响应；oracle先发送成功和陈旧请求，再等错误响应。初版等待被supervisor SIGTERM取消（130）并清理；初版Go默认region本地存储导致etcd等待超时（101），显式选择etcd后通过。这些失败不冒充产品缺陷，日志保留。Go二进制来自既有bin/pd-server，其build info无VCS revision，不能声称来自当前HEAD。

## 验证记录

所有Cargo阶段从仓库根、默认features执行。实际槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；依次原子mkdir领取，记录计划/任务/会话/PID。所有Cargo、使用槽位产物的子进程已结束；已核对无存活槽位进程，删除自己的owner记录并rmdir释放锁，保留编译缓存。


| 确切命令（均从仓库根执行） | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始`cargo test --locked -p astersql-pd-runtime --lib task_005_ -- --nocapture` | 101 | 0通过/1行为断言失败 |
| 最终同一聚焦命令 | 0 | 2通过/0忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 5通过/1需oracle忽略；下一行显式补跑 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case cluster-host --case health-member --case tso` | 0 | 3场景各1通过/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查，不计行为测试 |
| `cargo fmt --all`及`cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 1930通过/2忽略（TSO与本任务oracle，已显式补跑） |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 当前最终源码构建 |
| `python3 -B scripts/pd-compat-readonly.py --mode go --case cluster-host` | 2（预期拒绝） | 非dual前置拒绝，未启动服务 |
| `bash -n scripts/run-pd-compat-readonly.sh`、Python AST、`git diff --check` | 0 | 静态检查 |

全量测试后移除一个未使用的runtime直接依赖并把cluster-host非法mode检查前移，随后重跑聚焦、包级、workspace check、fmt、当前二进制构建和三场景只读差分均通过。没有改变生产Rust逻辑，未重复全量测试。包级第一次因两份fixture计数器碰撞失败的原日志也保留，修复共享fixture后包级及全量均通过。

原始输出、命令/退出码、来源哈希、完整Go文件清单、二进制build info及只读入口报告见[压缩证据](task-005-evidence.json.gz)，SHA-256：`2c21ba3441ba417ee7ac6f687840a5cfe87cd8f6f7090a66f6c0dadd391017da`。全量测试总数包含既有单元、反例和stand-in回归；真实Go兼容验收只计单独只读入口的3个场景。文档测试的0 tests不计行为证据。

开始/结束Go保护集合均为1983文件，包含ignored/untracked文件，所有路径、存在性、每文件SHA-256及Git状态完全相同；排序JSON摘要均为`eafa2dc5f0470621e39729f6ab4572c6cc35be32ae5d02034510e0e746ba4cbb`。最终只读入口的readonly_unchanged、inputs_unchanged、进程组清理及独占目录清理均为true。plan.md无差异。

真实Go二进制SHA-256：`b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`；最终Rust独立二进制SHA-256：`b8d5fc0356581a56bfc114759d58196b5d1fe30093b0fdadcb458b4bdbcdbd8d`。Rust集群差分使用当前runtime库装配，health/member/TSO另覆盖该独立二进制。
