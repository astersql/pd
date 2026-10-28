# 任务010：装配Bucket心跳服务

状态：已完成（ReportBuckets真实runtime接线边界）。

基准HEAD：`5d4baa964`。批次9及此前证据存在，主题6准入沿用task-001核验结果。复用canonical BucketHeartbeatService、StatePipeline::bucket、handle_bucket_heartbeat及现有HotStat worker；没有创建第二份领域算法或服务。

## 来源与实现

- source-inventory.md任务10整行、心跳/同步共同准入行、bucket_heartbeat_adapter.rs与server/cluster/bucket_heartbeat.rs的owner=10来源行，以及共享server/grpc_service.go::ReportBuckets（1092起）归本任务。
- wire-inventory.md的RPC-ReportBuckets行由真实client-streaming TCP/tonic接线验收；它是客户端流，不声称双向流。HTTP热点查询归任务77，完整转发/安全/限流归90/91/92。
- surface-inventory.md中ReportBuckets转发、安全、bucketReportCounter/Interval/Latency来源行仍归90/91/92及15/86/101。本次没有新增指标或配置，不把共享文件所有来源行算作完成。
- Go只读oracle：server/grpc_service.go::ReportBuckets、server/cluster/cluster.go::processRegionBuckets、cluster_worker.go::HandleRegionBuckets、pkg/core/region.go::CompareAndSetReportBuckets。协议固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。
- Application与RoleServices提供BucketHeartbeatService，ClusterHost在现有resources锁及有效leader下调用同一StatePipeline。停止/失去角色后不再接受bucket副作用；没有新worker、私有runtime或持久bucket缓存。新直接依赖复用workspace的statistics-buckets，Cargo.lock仅增加runtime依赖边。

## 行为证据

独立文件wire_bucket_heartbeat_test.rs在lib.rs显式注册。task_010_behavior使用真实etcd MetaStore、宿主Tokio、同一TCP listener及tonic客户端；RegionHeartbeat先提交region 7并等待实际持久化。

先报告缺失region 999，然后依次报告版本3/key=m、版本2/key=stale、版本3/key=equal、版本4/key=n。观察同一host snapshot：缺失region不产生统计，版本/边界依次为3/m、3/m、3/m、4/n；raw counters及2000ms周期保留，HotStat bucket worker实际产生统计。每次报告的EOF均为Unknown/EOF，持久region元数据不变。错误cluster返回FailedPrecondition且不修改bucket版本。开放流接受version5并更新snapshot但无成功ack；取消host后返回Cancelled，worker join、listener关闭、Registry为空、MetaStore引用释放。没有墙钟对比或需要推进的计时逻辑。

失败证据：初始接口已可编译，实际运行1项测试，断言Unknown/EOF时收到Unimplemented（bucket heartbeat state service is not configured），退出101。接线后同测试通过。增加直接依赖后首次locked命令明确拒绝更新锁（101），以cargo check --offline维护唯一依赖边后重跑locked验证；不把锁文件错误作为行为失败。

只读监督器新增bucket-heartbeat场景，复用既有bin/pd-server，启动一次真实Go PD，并在Rust真实runtime运行相同报告序列。Go HTTP region buckets边界与Rust snapshot比较均为[m,m,m,n]；旧版/同版用不同key确认被忽略，更高版本实际替换。双方同时断言缺失region被忽略、Unknown/EOF、错误cluster FailedPrecondition、真实etcd region元数据不变；Go HTTP health为true。Go服务端配置use-region-storage=false，仅确保读取同一etcd持久元数据，不改Go源码。

Go GetRegionByID的bucket返回受TiKV store feature控制，本场景未伪造TiKV配置；Go比较的是实际HTTP边界变化，不声称比较了未返回的bucket版本号或完整统计载荷。Rust单独验证版本、统计worker及取消。已有Go二进制构建信息没有VCS revision，不声称它来自当前HEAD。外部etcd不代表embedded-etcd拓扑等价。

## 验证记录

所有命令从仓库根、默认features执行。原子mkdir依次领取共享槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。原始日志、Go文件清单、监督器完整报告见task-010-evidence.json.gz。

| 确切命令 | 退出码 | 有效测试数 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-runtime --lib task_010_ -- --nocapture`（初始） | 101 | 1实际行为失败/0忽略 |
| `cargo check --offline -p astersql-pd-runtime` | 0 | 锁文件依赖边更新及静态检查 |
| `cargo test --locked -p astersql-pd-runtime --lib task_010_ -- --nocapture`（最终） | 0 | 1通过/0忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 13通过/4 oracle忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all -- --check` | 0 | 格式 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked` | 0 | 1938通过/5 oracle忽略 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 真实二进制 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case bucket-heartbeat` | 0 | 1通过/0忽略 |
| `git diff --check`及Python AST解析 | 0 | 静态 |

开始时1101项仓库Go/模块文件（含ignored/untracked，未扫描target缓存）及Git状态已记录。补充核验任务009保存的完整1983项快照与本次监督器before完全一致；监督器before/after完整扫描包括target缓存且readonly_unchanged、inputs_unchanged、work_directory_cleaned均true。最终完整1983项守卫与监督器before及任务009历史完整清单完全一致。没有改动Go/模块、只读plan.md或来源史诗；已有任务1–9删除保持原样并不纳入提交。

原始记录：[压缩证据](task-010-evidence.json.gz)，SHA-256：`a8c1f79a998b8f04542890eaca22a869598a6f7e578b70faa8fdcf66ed2f8240`。所有验证及槽位产物子进程已结束，自己的slot-1锁已释放，编译缓存保留。按要求删除任务10文件。
