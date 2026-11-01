# 任务099：真实Go客户端核心业务

基准 `a6b2a9a182ed38a18287d5b6307982a17d4696d2`；依赖
[任务098](task-098.md)及此前传递依赖已核验。
使用仓库已有 Go 工具源码及其真实二进制，未新增或改写 Go 源码、测试或模块。
新增 [工具进程辅助](../../../tests/rust-compat/go_client_acceptance.rs)及
[独立行为测试](../../../tests/rust-compat/go_client_acceptance_test.rs)，在
[lib.rs](../../../tests/rust-compat/lib.rs)显式注册。
生产服务已有本次核心行为；本任务只补真实 Go 调用方验收与只读入口。

## 来源和运行边界

[source-inventory.md](source-inventory.md)任务99行的 `client/`、`tests/server/`、
`harness.rs` 来源由本次真实客户端证据覆盖；复用任务98的进程装配和 leader 就绪观察。
[wire-inventory.md](wire-inventory.md)任务99的边界为“Go客户端只作兼容调用方，不移植”；
本次使用已固定 kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4` 的真实 Go 调用链：

- `tools/pd-heartbeat-bench/main.go`：GetMembers、IsBootstrapped、Bootstrap、PutStore、
  StoreHeartbeat、RegionHeartbeat；同目录 config 与 `tools/utils/meta.go` 创建真实元数据。
- `tools/pd-region-bench/main.go`：`pd.NewClientWithContext`，真实 service discovery，
  GetRegion/GetPrevRegion/GetRegionByID；工具自身检查 nil、region ID 及完整键范围。
- `tools/pd-tso-bench/main.go`：真实 PD Go client 的 GetTS；成功请求才计入统计。
  `client/clients/tso/dispatcher.go` 在返回成功前执行 `checkMonotonicity`，时间戳回退会 panic；
  单客户端、单并发跨切主持续运行并正常退出，实际执行这一检查。
- `client/servicediscovery/service_discovery.go`：原客户端进程重新发现 leader，原 TSO
  client 重建流；测试要求日志中的新 leader/TSO URL 与真实选举结果一致。

[surface-inventory.md](surface-inventory.md)没有99专属配置/CLI/指标来源；沿用任务4只读入口，
不增加服务开关，不把工具吞吐统计当作新增 PD 指标验收。
Go service discovery 会尝试可选 router/MetaStorage 发现；Rust 在该主题8边界的诊断仍保留，
本任务不宣称独立 MCS 或完整 Go client API 兼容。
Rust 三进程共享真实外部 etcd；Go 三进程使用自身 embedded-etcd，关闭可选本地 region storage
以比较共享持久元数据。外部 etcd 不构成 embedded-etcd 或主题10生产替代的证明。

只读测试参考为 `tests/server/server_test.go::TestLeader` 的三成员停 leader、
`tests/server/cluster/cluster_work_test.go::TestValidRequestRegion` 的 region epoch 语义及
`tests/server/tso/tso_test.go` 的单调性断言；这些源码只作 oracle 设计依据，未运行 Go 测试
或 failpoint 用例，也不将源码参考计为运行证据。

## 真实行为和故障

`task_099_behavior` 默认执行真实 Rust；dual 时先 Go oracle 再 Rust，同场景、同一套断言。
Go heartbeat 工具完成 bootstrap、注册3个store并持续发送心跳。保留原工具60秒上报周期，
通过真实 RPC 观察已提交的三副本 region 后才取消该工具；不依靠固定 sleep 假定上报完成。
随后 Go region/TSO 工具持续运行；测试停止当前 leader、等待新 leader，要求原工具进程继续
完成多个非零统计区间，region 稳定区间没有查询错误，并确认实际新 leader/TSO serving URL。

RPC 观察仅用于就绪及状态核验，业务请求由原 Go 工具发起，不用 tonic 代替 Go 调用方。
切主前后核对3个store、已 bootstrap、region ID=1、全键范围、epoch=(2,1)、
peer/store 对应关系 `(2,1)/(3,2)/(4,3)`；切主后 HTTP members 为200、3个成员。
Go region 工具还在运行时校验查询结果，Go TSO client 在每次成功分配前执行单调性断言。
随机 cluster ID、leader 序号、地址、时钟原点和吞吐数不逐值比较；比较相同状态及性质。

错误/恢复：停止真实 leader 产生连接/流失效，原 Go discovery 与 TSO client 自动重连，
原进程继续成功请求；原始日志保留连接拒绝、Unavailable/流取消和 serving URL 切换。
取消：向三个工具逐个发送真实 SIGTERM，Go context 取消真实 RPC/流，等待退出0；
心跳日志实际出现 `Canceled/context canceled`。剩余服务也须有界优雅退出。
Drop 在断言失败时 kill/wait 自有工具，既有 Server/Fixture 回收服务与etcd；目录 guard
清理独占数据目录。只读 supervisor 另外核验整个进程组与独占工作目录已清理。

最终 dual 聚焦结果：Go leader 0→1、region非零统计区间8个、TSO区间6个；
Rust leader 1→2、region区间11个、TSO区间4个。这些是完成请求的统计区间数，
不是 RPC 请求总数；原始逐区间计数、完整日志和二进制身份保存在压缩证据中。

## 失败证据与装配修正

初次工具构建从根模块指定 tools 子包，退出1；改用现有 tools 子模块的 `go -C tools build`，
不更改模块文件，region/TSO/API bench 均构建成功；API bench 仅作入口盘点，不计本次验收。
初次 Rust 测试把 ScanRegions 的 pdpb.Region 当成 metapb.Region，编译退出101；
修正类型访问后才取得运行失败证据，不把编译错误计作行为红灯。

10-region 初轮可运行，Rust 工具报 `unmatched nil region`，0通过/1失败、退出101。
先对 Go oracle 跑相同工具，得到相同失败。源码显示该工具发送准备心跳后立即取消流，
没有等待提交；因此不能把失败归因为 Rust 服务缺陷。
单 region 场景中 Go oracle 通过，但 Rust 尚未收到三副本心跳，真实元数据断言失败，
仍为0通过/1失败、退出101。改为复用持续心跳工具并等待真实提交，保持三副本断言；
不编辑 Go、不手工以 tonic 补发业务，也不降低状态检查。此后 dual 1通过、退出0。

增加 HTTP 断言后，第一次最终入口用了 harness 默认保留端口而非显式拓扑端口，
连接拒绝、0通过/1失败、退出101；修正测试中 Server.url 的观察地址后，重跑最终入口通过。
以上是验收装配修正；生产源码未修改，不宣称发现并修复了生产缺陷。

## 可复现入口和检查

先从1–10依次原子 mkdir 领取共享槽位，记录计划、任务、会话和持有PID；本次每阶段均领取1。
实际 `CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
所有 Cargo、工具、fixture 和服务子进程继承该绝对路径，默认features，无 --target-dir。
`CARGO_BUILD_JOBS=4`、`RUST_TEST_THREADS=4`。

已有 Go 工具只读构建使用缓存 Go 1.26.8；全部从仓库根调用，
`GOFLAGS=-mod=readonly GOWORK=off CGO_ENABLED=0 GOPROXY=off GOTOOLCHAIN=local`：

```sh
TASK099_GO=/Users/xiangmin/go/pkg/mod/golang.org/toolchain@v0.0.1-go1.26.8.darwin-amd64/bin/go
"$TASK099_GO" -C tools build -mod=readonly -o "$CARGO_TARGET_DIR/pd-region-bench" ./pd-region-bench
"$TASK099_GO" -C tools build -mod=readonly -o "$CARGO_TARGET_DIR/pd-tso-bench" ./pd-tso-bench
"$TASK099_GO" -C tools build -mod=readonly -o "$CARGO_TARGET_DIR/pd-heartbeat-bench" ./pd-heartbeat-bench
cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server
export PD_COMPAT_MODE=dual
export PD_COMPAT_GO_BIN="$PWD/bin/pd-server"
export PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server"
bash scripts/run-pd-compat-readonly.sh --case go-client-core
```

只读 supervisor 新增该 case，固定执行以下聚焦命令，要求恰好1通过/0失败/0忽略；
额外核验三个工具为已有 Go PD tools 模块的可执行二进制，记录 build info/hash，结束时再查 hash。
既有 Go server 无 VCS revision，不声称由当前HEAD构建；新工具的构建身份与只读源文件哈希均记录。
包级/workspace采用标准环境（清除 PD_COMPAT_MODE/GO_BIN/RUST_BIN），其中本任务仍执行真实Rust；
dual 差分由独立入口证明，不把标准环境结果冒充双实现结果。

| 确切命令 | 退出码 | 有效测试（通过/失败/忽略） |
| --- | --- | --- |
| 上述三个 `go -C tools build -mod=readonly` | 0 | 构建 |
| `cargo fmt --all` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `cargo test --locked -p astersql-pd-compat-tests --lib task_099_ -- --nocapture`（dual入口实际argv） | 0 | 1/0/0 |
| `bash scripts/run-pd-compat-readonly.sh --case go-client-core` | 0 | 1/0/0；全部只读/清理守卫通过 |
| `env -u PD_COMPAT_MODE -u PD_COMPAT_GO_BIN -u PD_COMPAT_RUST_BIN cargo test --locked -p astersql-pd-compat-tests -- --nocapture` | 0 | 38/0/1 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-compat-tests` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `env -u PD_COMPAT_MODE -u PD_COMPAT_GO_BIN -u PD_COMPAT_RUST_BIN cargo test --workspace --locked` | 0 | 2225/0/79 |

原始命令、退出码、完整日志、Go文件清单和二进制build info/hash见
[压缩证据](task-099-evidence.json.gz)，SHA-256 `1b6c5c22032dfa1b3efa408aa04ed486a0cbf5018f598713e85cab7282953f8b`。
包级及工作区的既有忽略项不计为任务99证据；任务99不忽略、实际行为1通过。

Go只读守卫：开始/结束含ignored/untracked共1101个Go/模块文件，路径集合及每文件SHA-256
完全一致，Go Git状态为空；plan.md字节哈希与开始一致。未运行make、failpoint启停、Go测试、
源码生成、格式化或模块整理。构建只复用已有只读源码，不生成Go工具源码。
最终只读入口确认 readonly_unchanged、inputs_unchanged、process_group_cleaned、
work_directory_cleaned 全部为true。全部工具、服务、fixture已结束，ps未发现自有服务进程，
自有go-client99目录与slot-1锁已清理，编译缓存保留。
Python AST、shell语法、文档链接及git diff --check通过。
保留原有任务94–98删除及prompt.md修改，不纳入本任务提交。

状态：已完成。
