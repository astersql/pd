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
