# 任务102：最终审计的阻塞修复进展

2026-11-02；基准 `a0f492cc1`。状态：部分缺口已修复，整体准入仍阻塞。

## 本轮实质修复

- `pkg/server/http_maintenance.rs` 将四个 maintenance 方法接到已有
  `StorageEndpoint` 的原子启动、删除、单项读取与分页查询；与 runtime 的其他
  服务共用 `recovery_marker_store()` 返回的 MetaStore，不新增领域算法。
  路径、状态、字符串、冲突对象以只读
  `server/apiv2/handlers/maintenance.go` 及
  `pkg/storage/endpoint/maintenance.go` 为契约依据。
- 全局锁跨 task type 互斥；PUT 原样读取描述；无任务404、冲突409、损坏单项500；
  全量读取跳过锁和坏记录。取消使用现有请求 cancellation，错误返回 JSON 字符串。
- `ServiceMetrics` 在宿主 Registry 注册原有 Go 名称/HELP/标签的
  `pd_maintenance_task_info`，仅事务成功后置1/0，宿主退出注销。
  task_id 动态标签与 Go 契约一致，操作属于低频维护路径；保留删除后零样本，
  不擅自更改其历史指标行为。
- `http_v2_boundary` 仅保留26个尚未准入的领域方法。原任务078的拒绝测试和
  拒绝差分只检查这26项；maintenance由新增独立行为测试覆盖，未把它静默归为
  原拒绝差分通过。历史任务078证据不改写。
- `pkg/proto/build.rs` 优先消费显式 `GOMODCACHE`，避免在已有协议ZIP时仍强制
  执行不存在的 `go env`。缺省继续调用只读 Go 查询；固定版本和ZIP校验未改变。

## 行为证据与局限

独立模块 `pkg/server/http_maintenance_test.rs` 已在 lib.rs 显式注册。
`task_102_maintenance_persistence_and_conflicts` 使用真实 TCP、完整 PdNode runtime
和同一个 MemoryStore，验证空查询、创建、描述和时间、跨类型冲突、错误ID删除、
关闭并按相同成员身份重建宿主后读取、删除并释放锁、损坏JSON和8并发请求唯一胜者，
以及实际指标值和宿主关闭后的注销。该重建保留同一内存后端，**不是进程崩溃恢复或
etcd durability 证据**。未使用 mock 调用次数或固定成功响应。

修复前已编译运行，首个行为断言 `GET maintenance` 实收503、预期404，退出101；
修复后1个测试通过、0失败、0忽略。开发中重建地址变化触发 IdentityConflict，
测试改为复用同一地址，未放宽生产身份规则。

本机无可用Go工具链、etcd fixture二进制或旧pd-server oracle，未重跑Go差分。
Go来源和任务078保存的历史运行输出只是契约参考，不冒充本轮差分。
GC的15个RPC、keyspace/TSO组/meta-service组/MCS、主题10拓扑与durability，
来源manifest漂移归因及完整兼容证据仍未完成，任务102不得标为整体完成。

## 验证环境

槽位1：`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
原子mkdir领取，owner记录执行进程；仅本阶段使用，结束后释放锁并保留缓存。
工具链下载未完成，实际用已安装 `RUSTUP_TOOLCHAIN=stable`（rustc 1.99.1）。
`CARGO_BUILD_JOBS=6`；最终回归 `RUST_TEST_THREADS=1`。
`GOMODCACHE=/tmp/task102-proto-cache`，仅下载固定ZIP、不提取Go源码；校验仍为
`aff09d074d493874bfd7e3e63c766a8d93f42901ff235dde2d2ced0c7f2a615e`。
`PROTOC=$HOME/.cargo/registry/src/rsproxy.cn-e3de039b2554c837/protoc-bin-vendored-macos-aarch_64-3.2.0/bin/protoc`。
依赖镜像出错后通过static.crates.io补齐缓存并逐项核对Cargo.lock SHA-256；
未改锁文件、仓库registry配置或任何Go源码/模块文件。

确切命令、退出码和日志见 [本轮证据](task-102-evidence.json.gz)。

最终复验：metadata、fmt --check、workspace check退出0；task_102_和
task_078_cancel_and_unread_body各1测试通过，0忽略。workspace test退出101，
已执行98通过、6失败；6项均在定位Go工具链时失败，尚未启动etcd fixture，
其他crate未由该次命令继续运行。不声称全工作区测试通过。
compat包级、四crate联合回归、pd-server双实现构建/运行未执行：缺少Go
及oracle环境，且最终领域准入仍缺失。Go只读守卫1101项一致，Git状态无Go差异。


## 同日恢复：真实etcd与双进程差分

本节更新上述首轮环境限制；首轮日志保留为历史，不与本轮结果混合计数。

- `pkg/storage/kv/etcd_fixture_test.rs` 新增显式 `PD_COMPAT_ETCD_BIN` 入口，
  直接启动并验证官方 etcd 3.5.15。默认 Go fixture 路线保留。
  native模式直接拥有etcd子进程，原有suspend/CONT作用于实际存储进程；
  stop发送TERM并等待退出，start复用相同地址和WAL并等候 `/health`，Drop回收进程和目录。
  `PD_ETCD_BIN` 仍用于已有HTTP/member supervisor，需相邻etcdctl。
- 官方 darwin-arm64 ZIP SHA-256 为
  `86e189e61f021d9d94e16cdad7c2fda65fd2439ecc4895c45429e3bdc1ca9512`，
  与发布的SHA256SUMS核对；仅提取etcd/etcdctl/etcdutl可执行文件。
  下载来自 [etcd 3.5.15官方发布](https://github.com/etcd-io/etcd/releases/tag/v3.5.15)，
  本次环境位于共享槽位的 `etcd-official/`，缓存保留。
- 上轮失败的cluster六项均已通过，cluster包70通过、0失败；etcd_contract的
  12项全部通过，包括真实事务、lease、watch、选举和停止/WAL恢复。
- maintenance新增真实EtcdMetaStore/WAL重启用例，不再仅有同一MemoryStore重建。
  新增历史Go输出回放；其首次实际失败揭露Content-Type缺少charset，修复为
  `application/json; charset=utf-8` 后通过。历史响应保存在
  `tests/rust-compat/fixtures/task-102-maintenance-go.json`，带原压缩证据哈希和归一化口径。
- `workspace_acceptance.rs` 与独立 `workspace_acceptance_test.rs` 实现本轮
  maintenance真实进程差分，并在compat lib显式注册。显式运行ignored测试，
  `PD_COMPAT_MODE=dual`；真实Go和Rust进程各执行11个成功/错误场景，状态、
  JSON全部字段和Content-Type一致。仅将各进程不同的时间戳归一化，同时强制同一
  任务在查询和冲突响应中的实际时间戳保持一致。两个进程的原始响应另存证据。
- runtime TLS测试原来依赖Python/LibreSSL的英文错误名称，在macOS显示unknown error。
  测试现在直接检查fatal TLS alert 120，不放宽为任意握手失败；协议依据
  [RFC7301 §3.2](https://www.rfc-editor.org/rfc/rfc7301.html#section-3.2)。
  HTTPS、HTTP/2 AllocID、真实持久值和ALPN拒绝的原行为测试已通过。

### Go oracle的适用范围

通过官方TiUP版本化manifest取得现成macOS PD二进制，归档SHA-256
`e76ca942ab8163eebd6dc07f56e7f5a39e7fa4f96e7c0be89dfe22de7e377275`，
版本 `v9.0.0-beta.2.pre-553-g988bf955d`，提交
`988bf955dedd4f91df598eb13331720c14ce698b`。
`maintenance.go`、对应storage/keypath/metrics来源与当前Go源码一致；
本轮maintenance差分可归因于这些真实契约。

该发布二进制的kvproto却是 `107095f1d250`，不是本计划固定的 `f8ad079c8e98`。
扩大的任务100回归实际失败于Go丢弃 `txn_protocol_version_range`；不能以此修改
Rust来丢字段，不能声称完整新协议差分通过。仍需匹配固定协议的现成Go oracle。
为读取build info，本轮仅从校验过的Go 1.26.8发布归档提取 `go/bin/go` 可执行文件；
没有提取、新增或修改任何Go源码/模块文件，也没有编译Go程序。

### 补齐来源manifest扫描

三份清单实际解析1909/365/267条唯一路径，缺失均为0；历史哈希变化分别为
162/20/26，变化条目没有Go文件。已保存每项原始/当前哈希、最近修改提交与工作树
变化标识。此前未完整读取的wire 365项现已全部核查。此项只是静态漂移定位，
尚未逐项完成行为证据归因，不把有git提交当作已经兼容。

本轮完整命令、退出码、有效测试数、输入哈希、manifest报告、实际双进程响应
及失败证据将保存在 `task-102-resume-evidence.json.gz`；最终结果以该证据为准。


### 最终收尾结果

- server包默认测试283通过、0失败、39个既有差分默认忽略；本轮maintenance三项均实际通过。
- task_102显式ignored入口实际运行2项、2通过：maintenance真实双进程比较，
  以及对本轮52份实际采样报告独立重验76个shared metric family契约。
  后者只验证已采集指标属性，明确打印仍失败的业务replay，不替代完整HTTP验收。
- 指标矩阵原先缺少Rust `disappear` 的运行观察。扩展现有service-metrics真实RPC
  场景：公开API登记store2，创建admin-add-peer operator，删除region cache并等待
  控制器实际移除；Go和Rust都恰好增加1次 `admin-add-peer/disappear`。
  旁路HTTP采样确认两侧真实样本。原指标断言、原矩阵失败规则均未放宽；
  未添加冷零样本或人工调用计数器。仅重跑该受影响场景，再使用其结果替换同轮
  service-metrics报告做指标契约重验；没有声称重新执行了全部52个场景。
- CLI配置/退出/日志supervisor也统一复用native etcd，解决仍查找旧编译fixture的问题。
  三项实际回放最终全部退出0；配置包含9个真实Go/Rust校验对。
  单独重跑退出/日志时曾漏传报告路径，本轮已传入两个原有报告环境变量并重新通过。
- native suspend/CONT、停止、事务失败、取消及恢复的真实runtime指标测试1项通过；
  TLS行为测试1项通过。metadata、workspace check、fmt --check退出0。
- 扩大的workspace test仍退出101，未计作全绿。最初无oracle运行98通过/6个环境失败；
  native修复后cluster70全部通过。带旧Go oracle并显式跳过缺失benchmark的task099后，
  compat包29通过/2失败/1默认忽略/1过滤，失败是task100及task101整体入口。
  task101原52个supervisor有46个成功；CLI三项已修复并重跑成功，剩下三个业务差分：
  region-schedule的超范围数值断言、server-config的Dashboard构建配置差异、
  operator-write差分。它们没有被指标属性重验覆盖或豁免。
- 官方Go oracle协议过旧，且缺pd-region-bench/pd-tso-bench/pd-heartbeat-bench，
  因Go只读限制未现场构建这些工具。当前不具备完整新协议/客户端验收条件。
- `docs/rust/runtime.md` 已清理过时的MVP能力描述，并改为指向100/101的历史范围
  和102的当前证据，禁止使用旧的Go写入入口。此文档修正新增一项已解释静态漂移；
  最终source/wire/surface变化数为163/20/26，路径缺失仍全为0。

压缩证据将保留本轮命令/退出码/有效测试数、实际maintenance原始响应、
受控operator指标原始报告、Go/etcd来源和版本、只读守卫、manifest全量扫描结果。
52份指标报告仅保留目录metric_contract各family的原始HELP/TYPE与原始sample_schemas，
受控operator保留该family的真实数值样本，并记录完整exposition SHA-256，
以复现指标契约验证；原始309MB观察保留在槽位缓存，不把简化报告当作完整数值差分证据。
GC/keyspace/MCS等必要领域实现和主题8/9/10兼容证据仍缺失，任务102保持阻塞。

## 收尾任务1：固定协议基准已恢复（2026-11-03）

此前仅检查darwin/arm64漏掉了其他可执行架构。本机能执行macOS amd64，
官方OCI历史产物提供了固定协议版本，无需编译或新增Go源码：

- [官方OCI固定提交manifest](https://us-docker.pkg.dev/v2/pingcap-testing-account/hub/tikv/pd/package/manifests/master-8a51f15_darwin_amd64)，
  manifest摘要`cb5a2ec19b47ef575a5e9e1cbd701ffcef93cf00dd29d823cd70bd788a31a9c3`。
- PD归档SHA256 `567292bd6d23e35491d3663126dcc5552a5cfa576ba71f288eacb362ef2a7f0c`；
  仅提取`pd-server`可执行文件，二进制SHA256
  `a6856ddbac5cd128ea3f9a598546c33cb3786ebb9b5a51abe3b29ced6025127f`。
  build info确认Go1.26.8、darwin/amd64和精确kvproto
  `v0.0.0-20261016101550-f8ad079c8e98`。版本与完整build info保存在压缩证据。
- 实际基准缓存为`target/rust-slot-1/tail102-inputs/pd-server-f8ad079`；
  该目录`env.sh`保存dual模式、绝对Go/Rust/etcd/inspector路径。
  使用前必须重新领取共享槽位；env.sh本身不领取锁。
- `--require-pinned-protocol`在启动进程前拒绝协议漂移；旧协议、错误二进制、
  缺失文件三个真实负向探测均退出2。未加该选项的历史窄合同入口保持原行为。
- `readonly_oracle_test`为4通过、1显式集成测试默认忽略；随后
  `tail102_inputs_ -- --include-ignored --nocapture`实际2通过、0忽略，
  包含真实Go build-info读取和版本入口执行。默认忽略未计入通过数。
- `python3 scripts/pd-compat-readonly.py --require-pinned-protocol --case store-registration`
  退出0，真实双进程差分1通过、0忽略。Go与Rust均保留
  `txn_protocol_version_range { min: 1, max: 3 }`，重启后恢复6个相同store；
  原始响应、错误、归一化口径和副作用沿用现有场景，不丢弃字段。

三个工具仍缺，任务1尚未完成。已核查TiUP bench/ctl归档、官方OCI package
及image目录；官方产品镜像history最终仅COPY pd-server、pd-ctl、pd-recover。
[官方构建Dockerfile](https://github.com/PingCAP-QE/artifacts/blob/main/dockerfiles/cd/builders/pd/Dockerfile)
虽在中间阶段运行`make build tools`，最终镜像只复制上述三个产品程序；
8286个image标签没有bench/tool/build/cache/intermediate标签。
完整本机扫描未发现SDK或三个benchmark；现有Linux arm64 VM的只读搜索也无结果，
Colima未运行，配置的远端SSH均连接失败。没有下载SDK/模块源码或触发远端构建。
剩余外部输入是已有CI中间阶段`/ws/bin/`的pd-region-bench、pd-tso-bench、
pd-heartbeat-bench，或其他可执行架构、来源可核对的现成二进制，架构不限于arm64。

槽位1，`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`；
命令、退出码、日志和只读快照追加在
[恢复证据](task-102-resume-evidence.json.gz)的`tail102_inputs_2026_11_03`，保留旧失败记录。
本轮证据只解除Go oracle缺口，不解除工具、其他领域或原102验收阻塞。

继续核查中间产物：GHCR公开`pingcap-qe/cd/builders/pd`完整分页列出2200个
标签，未发现cache/bench/tool/building标签；选定Go1.26 builder的配置blob读取
经三次重试仍TLS EOF，不能声称已逐层核查该blob。
[官方skaffold配置](https://github.com/PingCAP-QE/artifacts/blob/main/dockerfiles/cd/builders/pd/skaffold.yaml)
明确发布`target: builder`，该阶段只有SDK/构建依赖，尚未进入含`make build tools`
的`building`阶段。没有找到公开导出的building阶段或远端BuildKit cache地址。
GitHub CI的release-8.5 ghpr_build仅运行race/default make，未声明benchmark归档；
内部Jenkins缓存没有公开读取入口，本机没有对应可用远端会话。

按完整提交号探测fileserver.pingcap.net的centos7/pd-server.tar.gz、
pd-tools.tar.gz及三个单工具归档均TLS EOF，HTTP master引用也无法访问；
这些是网络不可达记录，不声称归档不存在。历史TiUP v3.0.1归档实际列目录：
PD只有pd-server；ctl只有binlogctl、ctl、etcdctl、pd-ctl、tidb-ctl、tikv-ctl。
三个工具的TiUP nightly linux/amd64候选路径均404。当前可访问官方公开渠道尚无
可取得的三个工具；内部缓存/文件服务器的可访问性是剩余外部状态。

另一个解除条件是明确允许在仓库外预置Go SDK及全部模块源码/模块文件缓存，
然后用现有只读Go工具入口、CGO_ENABLED=0、GOTOOLCHAIN=local、GOWORK=off、
GOPROXY=off和-mod=readonly构建三个工具，可执行文件输出共享槽位。
这会突破当前“不得新增Go源码（含临时目录）”规则，因此本轮没有执行。
这是可具体尝试的替代路线；在实际构建及CLI验收之前不保证工具兼容成功，
不需要修改仓库Go源码或Go模块文件。

固定基准Dashboard编译信息：内置tidb-dashboard e5fc397a69b0，Standalone=No，
内部版本9.0.0-e5fc397a；runtime沿用既有Server::start的默认Dashboard设置，
未传Dashboard关闭标志或额外配置文件。CLI只设置本机动态client/peer地址、
独立data-dir及initial-cluster；Rust侧通过现有PdNodeBuilder连接真实etcd。

## 收尾任务1完成：授权隔离构建工具（2026-11-03）

用户明确允许仓库外SDK及模块源码，仓库内Go/模块继续只读。
官方Go1.26.8 darwin/arm64 SDK SHA256为
`a012b25b571bd0138a03dcd25375ceba866fe5ca822f426d2c66a4de56fd3f4b`，
校验后解压至`/tmp/pd-task1-go-env/go`。模块、编译缓存和临时目录均在同级隔离目录。
从现有tools模块以`GOTOOLCHAIN=local GOWORK=off GOFLAGS=-mod=readonly CGO_ENABLED=0`
构建三个benchmark，输出到共享槽位1根目录，三次构建退出0；三个真实CLI --help均退出0。
未运行make、failpoint、生成、格式化或tidy流程，1101项仓库Go/模块快照完全一致。

readonly_oracle_test显式包含集成测试后5通过、0忽略；task099真实Go/Rust双实现
工具访问与选主恢复1通过、0忽略（165.65秒）。固定协议字段回读证据沿用上节已通过结果。
至此本任务输入与工具验收完成，不代表原102或其他领域阻塞解除。

实际槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；
所有子进程结束后释放自己的锁，保留二进制与缓存。确切命令、SDK/三个工具SHA、
完整build info、CLI输出及行为日志追加在现有压缩证据
`tail102_inputs_authorized_build_2026_11_03`，历史失败记录保留。

## Tail task 2: scheduling HTTP differential (2026-11-03)

The original supervisor passed against the pinned darwin/amd64 Go oracle:
exit 0, 1 passed, 0 ignored. The historical overflow failure did not reproduce.
Both 1e100 and 2^64 select the heartbeat-registered, read-back 2^63 Region
(HTTP 200, processed-percentage 0). Saturation to u64::MAX would contradict
this observation. Preserve the documented amd64 conversion contract; other
Go architectures are not certified by this run.

Added 2^63, -1 and normal ID 10 to the same real dual-process scenario.
`PD_COMPAT_MODE=dual python3 scripts/http-region-schedule-oracle.py` again
exited 0: 1 passed, 0 ignored, 186.89 seconds. Existing batch, errors,
operator side effects, cancellation and role assertions remain enabled.
After `cargo fmt --all`, `cargo test --locked -p astersql-pd-server --lib
 task_067_ -- --nocapture` exited 0: 3 passed, 1 explicitly ignored differential;
the supervisor executed that ignored test separately.

Slot 1: CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1.
Both supervisors confirmed unchanged Go/module snapshots and input hashes,
and cleaned their process groups. Raw responses, fingerprints, normalization
and side effects are preserved in the existing compressed evidence under
`tail102_schedule_2026_11_03`. Other domain and original 102 blockers remain.

## Tail task 3: configuration HTTP differential (2026-11-03)

The fixed-protocol Dashboard-enabled oracle reproduced the historical failure:
Go resolved auto to its HTTP address while Rust retained auto. Built the existing
read-only Go sources with `-tags without_dashboard`, using the authorized SDK and
module cache outside the repository, `CGO_ENABLED=0 GOTOOLCHAIN=local GOWORK=off
GOFLAGS=-mod=readonly`. No generators, failpoints or module edits. Build exited 0;
build info pins kvproto f8ad079c8e98, matching the existing protocol input.
The supervisor now requires that build tag instead of accepting mismatched inputs.

The same full task050 differential passed with default auto (not explicit none).
Added real TCP requests for /dashboard/ and /dashboard/index.html: Go returned
200 with `Dashboard is not built.\n`; Rust initially returned 404. Added the real
placeholder handler matching Go's unbuilt profile. The identical supervisor then
passed, 1 passed/0 ignored; all configuration response/state comparisons remain.
Initial full configuration responses are retained in the final raw run. Explicit
addresses, hot writes and errors are compared without dropping Dashboard fields.

`cargo test --locked -p astersql-pd-server --lib task_050_ -- --nocapture` exited 0:
2 passed, 1 explicitly ignored differential (executed separately by supervisor).
This covers cancellation, transactional failure and persistence across restart.
`cargo metadata --format-version 1 --no-deps`, `cargo fmt --all -- --check`, and
`cargo check --workspace --locked` exited 0.

Slot 1, CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1.
Raw failures and successes, build profile and SHA, initial responses and local
test output are appended under tail102_config_2026_11_03 in the existing
compressed evidence; historical failures remain. The certified profile omits
the Dashboard application, and does not claim full Dashboard domain completion.

Supplementary `cargo test --workspace --locked` was started, then stopped with
SIGTERM while the long task100/101 compatibility suites were running. It is not
counted as a passing full regression; task6 owns that result. Completed task099
and earlier package results are retained in the raw log. All owned descendants
were terminated before slot release. Go/module Git status remained unchanged;
each configuration supervisor also verified its complete read-only snapshot.

## Tail task 4: Operator HTTP differential (2026-11-03)

Dependencies 1 and 2 are verified above. The historical full-array failure did
not reproduce with the pinned protocol oracle and existing numeric fixes.
`PD_COMPAT_MODE=dual python3 scripts/http-operator-write-oracle.py` exited 0:
1 passed, 0 ignored, 13.18 seconds. Complete request/response/operator arrays
and restart state matched without changing normalization or removing assertions.
No Operator implementation change was necessary.

`cargo test --locked -p astersql-pd-server --lib task_063_ -- --nocapture`
exited 0: 2 passed, 1 explicitly ignored differential (run above), including
cancellation and follower coverage. Raw arrays, build information and unit
output are appended under tail102_operator_2026_11_03 in compressed evidence.
The supervisor verified unchanged Go/module snapshots and cleaned its process
group. No Go/module files were written. Slot 1,
CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1; both stages
released their own locks and retained caches. This does not clear domain or
original 102 blockers.

## Tail task 5: source audit progress (2026-11-03)

Toolchain recovery succeeded. `cargo metadata --format-version 1 --no-deps`
exited 0 (75 packages), slot 1,
CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1; lock released.
Real-format parsing found 1909/365/267 unique existing source paths and
163/20/26 historical hash changes. Full baseline/current hashes and candidate
attributions are preserved in tail102_source_audit_2026_11_03 in compressed
evidence. References and nearby test sources are explicitly candidates, not
current business certification. Effective behavior tests: 0.
Normal workspace dependency closure for core, storage-domain, member, tso and
schedule reaches none of proto/server/runtime. storage-kv's tonic dependency
is a backend adapter detail requiring public-contract review.
This is partial audit evidence; source attribution and additions review remain
in progress. It does not clear domain admission or original task102 blockers.

### Tail task 5 attribution review

All 1909/365/267 frozen source rows have source roles and implementation
responsibility recorded in the existing compressed evidence. Historical drift
163/20/26 is linked to preserved runtime input snapshots and actual execution
lineage (command, exit, test summaries, output digest), not inferred from git
history. Current input hashes match for 151/16/24 rows; all unmatched rows are
Markdown documentation, for which runtime execution is inapplicable.
Input identity does not certify every behavior in a file. Scheduling task067,
configuration task050 (without_dashboard profile), and Operator task063 retain
their precise successful real-process scopes and earlier failures. Metrics-only
reports remain property evidence, never business admission.

Since the source baseline, Rust/Cargo/scripts contain 368 additions and 156
modifications, no deletions. Every path is listed with current hash, owning
crate/infrastructure responsibility and test/build/supervisor/implementation
role. Existing domains are handed off to the original control-plane/MCS and
production-backend epics; GC/keyspace/MCS/native persistence remain uncertified.
No domain plan or original102 completion state is changed.

The ADR review uses actual normal workspace closures plus public source
boundaries: core and storage-domain have no protocol/transport dependencies;
member/tso/schedule do not reach server/runtime/proto. Storage-kv uses tonic as
an external-etcd adapter; storage-domain exports domain/byte/JSON contracts.
The facade only re-exports runtime APIs. A process exit in TSO fault tests is
explicitly test-only, not production library lifecycle behavior.

### Tail task 5 completed audit validation

`cargo fmt --all` exited 0. With the existing vendored ARM protoc and
GOMODCACHE=/tmp/pd-task1-go-env/mod, `cargo test --locked -p
astersql-pd-compat-tests --lib task_102_source_ -- --nocapture` exited 0:
2 passed, 0 failed, 0 ignored. Negative controls reject missing/zero rows,
duplicates, stale runtime input hashes and metric-only or absent business
proof. The second test applies that gate to all 163/20/26 drift records.
These are two audit tests, not two new live business differentials.
Earlier missing-PROTOC/GOMODCACHE build failures remain recorded alongside
the successful retry. Metadata exited 0 (75 packages). Slot 1 used
CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1;
all owned children finished and the lock was released, caches retained.
Final Go/module snapshot contains 1101 files including ignored/untracked,
with empty Git status; no Go/module writes were performed.

This completes the bounded provenance audit and records domain handoff gaps;
it does not certify unimplemented domains or change original task102 status.

## Tail task 6: final regression closure (2026-11-04)

Final default-feature regression uses the pinned-protocol darwin/amd64 Go
`without_dashboard` oracle built read-only at
`/tmp/pd-task1-go-env/pd-server-without-dashboard-amd64`, official etcd 3.5.15,
the existing protocol ZIP and absolute vendored ARM protoc. The arm64 oracle's
out-of-range numeric conversion differs from the certified amd64 contract;
its failed attempt is retained, not merged into the final result.
All final tests use `RUST_TEST_THREADS=1`. Heavy unrelated host load and
interrupted runs are preserved as failures/incomplete, not successful evidence.

The regression exposed and repaired bounded validation defects:

- v2 evidence paths now support an absolute oracle outside the repository,
  retaining its SHA and all source/input checks.
- TLS/mTLS/shared-listener rejection snapshots exclude only the exact current
  cluster's periodically persisted TSO timestamp row. Extracted failed snapshots
  prove that was the only changed row; all other entries/revisions still compare.
  A negative control retains alloc_id, config, another cluster and similar keys.
- health/member cleanup checks the real leader key exists before shutdown and
  disappears after successful exit, with directory and port cleanup retained;
  it no longer depends on an internal lifecycle event appearing in CLI logs.
- checker fixtures assign distinct real store region weights to avoid tied
  candidate ordering while comparing complete diagnostic reasons unchanged.
- HTTP concurrency cancellation waits for a real `100 Continue` body-read
  handshake before probing 429, preventing the probe from rejecting the intended
  pending request. Cancellation must still return the occupied slot.

Current evidence (exact commands, exits, raw logs, requests/responses, persistent
side effects, input hashes and cleanup are appended under
`tail102_final_regression_2026_11_04` in the existing compressed evidence):

| Entry | Exit | Effective result |
| --- | --- | --- |
| pd-server build | 0 | executable built |
| task099 behavior | 0 | 1 passed, 0 ignored |
| task100 filter | 0 | 3 passed, 0 ignored; also passed in both final full runs |
| task101 filter | 0 | 14 passed, 0 ignored; 52 real supervisors; final full runs repeat all 52 |
| server task102 | 0 | 3 passed, 0 ignored |
| compat task102 explicit ignored | 0 | 2 passed, 0 ignored, final combined-run 52-report JSON input |
| health_member integration | 0 | 10 passed, 0 ignored |
| service-limit supervisor | 0 | real Go/Rust replay and 1 explicit cancellation test |
| metadata / fmt check / workspace check | 0 | all passed |
| workspace test | 0 | 2252 passed, 0 failed, 82 default ignored |
| four-crate combined test | 0 | 468 passed, 0 failed, 82 default ignored |

Full commands add `--no-fail-fast` to execute every target, without filtering
cases. Zero-test/doc targets do not contribute to effective passed counts.
The final explicit metric check consumes exactly 52 distinct reports, all with
business exit 0, and observes 53 shared family schemas. Historical 56/54/76
observations remain historical samples, not substituted for this measurement.
Metrics property validation does not expand domain admission.

All stages dynamically acquired slot 1:
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`.
1101 Go/module paths including ignored/untracked retain identical existence,
SHA and empty Git status from task start through final cleanup. All owned
processes and fixtures ended before each slot release; caches remain.

This completes the six-item bounded regression/audit closure. Original102
remains blocked by GC/keyspace/MCS and native production persistence/embedded
backend evidence; local tests and explicit refusal boundaries do not certify
those domains. Their existing plans and source epics remain untouched.
