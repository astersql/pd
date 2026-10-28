# 任务004：Go只读兼容验证入口

状态：已完成（仅本任务的只读兼容入口，不解除后续服务/领域准入）。

基准HEAD：`d28d536f6`（完整值在压缩证据中）。依赖任务1–3的三份清单及task-001/002/003已核验存在并读取；保留原有编号任务1–3删除，不修改plan.md或来源史诗。

## 入口与调用链

```bash
# 先按计划动态领取空闲槽位并记录持有者；以下路径是本次实际槽位。
export CARGO_TARGET_DIR="$PWD/target/rust-slot-1"
cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server
export PD_COMPAT_MODE=dual
export PD_COMPAT_GO_BIN="$PWD/bin/pd-server"
export PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server"
bash scripts/run-pd-compat-readonly.sh
```

入口默认dual，保留go/rust/dual和health-member/tso选择。真实Go二进制必须有PD模块构建信息；Rust二进制必须来自已领取槽位的debug/pd-server，双实现必须是不同文件。不构建Go oracle，不调用旧入口、make或failpoint，不以different-service补缺。缺失/不可执行/相同二进制、缺失/非法fixture和无效槽位均非零退出。

`scripts/run-pd-compat-readonly.sh`只exec Python supervisor；后者仅启动固定Cargo测试命令，并强制每场景恰好1通过、0失败、0忽略，零测试失败。子进程继承绝对CARGO_TARGET_DIR、GOFLAGS=-mod=readonly、GOWORK=off、CGO_ENABLED=0、GOPROXY=off、GOTOOLCHAIN=local。每场景独立进程组；正常、错误、SIGINT/SIGTERM均终止/等待进程组。仅删除本次独占工作目录，保留槽位缓存和证据。

- `pkg/proto/build.rs`：只通过`go env GOMODCACHE`定位现有固定版本ZIP，SHA-256仍为`aff09d074d493874bfd7e3e63c766a8d93f42901ff235dde2d2ced0c7f2a615e`；缺缓存失败，不执行go mod download；只解压.proto到OUT_DIR。
- `pkg/storage/kv/etcd_fixture_test.rs`：真实Go etcd fixture以`go build -mod=readonly -o <槽位>/rust-etcd-fixture-<pid>`构建，关闭CGO/代理/工具链下载；优先使用按go.mod版本及Go宿主GOOS/GOARCH定位的既有缓存工具链，缺缓存时仅尝试本地Go并明确失败；数据放入supervisor独占目录；panic也清理fixture。
- `tests/rust-compat/readonly_oracle.rs`：共享二进制与fixture前置校验，`harness::run_minimal`在启动进程前调用；独立测试在lib.rs注册。`/usr/bin/true`/false仅用于前置文件属性校验，不能算兼容成功。

每次入口保存baseline、二进制/fixture/实际调用链源码及Cargo.lock哈希、完整Go清单和Git状态、确切argv、PID、退出码、有效测试摘要、进程组和目录清理结果。保护范围包括ignored/untracked的所有.go及Go模块文件，前后发现差异则失败，保留差异供检查。开始/结束哈希守卫证明本次文件集不变，不冒充操作系统级写入沙箱。

## 来源映射及边界

- [source-inventory.md](source-inventory.md)：准入表“只读Go兼容入口”、任务4映射及scripts/run-pd-compat.sh的owner=4来源行，由本入口和当前证据覆盖。
- [wire-inventory.md](wire-inventory.md)：RPC/HTTP表中“任务4只读oracle”是共享执行前置；本次只验收已有health/member及TSO，其他方法仍交由各owner，不把263条HTTP盘点计为运行通过。
- [surface-inventory.md](surface-inventory.md)：共同准入及4/86/90/91/92/101移交的只读运行入口已提供；配置、指标、安全、日志等具体差分不在本任务宣称通过。

健康/成员场景通过真实HTTP及GetMembers RPC核对身份/健康/成员/leader/协议header，真实Rust服务接入真实fixture etcd MetaStore。TSO场景核对三批count/header/suffix及物理/逻辑单调性，并对零count的Unknown状态和错误消息作差分。仅归一化随机成员/集群ID、listener地址和时钟原点；不把外部etcd当作embedded-etcd拓扑等价。底层数据目录实际创建且随后清理；更广泛持久化/崩溃恢复仍由后续owner验收。

已有Go oracle为预先存在的`bin/pd-server`，构建信息为go1.26.8、without_dashboard、darwin/amd64；其构建信息没有VCS revision，不能声称它来自当前HEAD。实际二进制SHA-256和完整build info随本次报告固定。Rust二进制由当前源码以规定命令构建。different-service仅保留在既有反例/进程资源测试中；包级默认stand-in场景不计真实差分，本次dual单独明确运行。

## 验收记录

原始命令输出、每条输出SHA-256、完整Go文件清单、入口报告及取消验证脚本见[压缩证据](task-004-evidence.json.gz)，SHA-256：`44ac13335df5607e073311de56cc4fdcddad4084ed520537bd81e3cf06f784e5`。重复快照以snapshot_ref去重，原入口JSON的raw_record_sha256也保留。

实际共享槽位：1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。从1–10依次原子mkdir领取成功，记录任务/会话/PID；所有Cargo、子Cargo和服务进程均继承该路径，无--target-dir覆盖。进程已全部结束，自己的owner记录和锁目录已删除，缓存保留。

| 确切命令 / 阶段 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始`cargo test --locked -p astersql-pd-compat-tests --lib task_004_ -- --nocapture` | 101 | 2失败；接口可编译，断言缺oracle必须失败及入口错误诊断，非编译错误 |
| 最终同一聚焦命令 | 0 | 3通过、0忽略 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建，不计行为测试 |
| `cargo test --locked -p astersql-pd-compat-tests -- --nocapture` | 0 | 36通过、1既有TSO忽略；单独显式运行见下 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh` | 0 | health_member 1通过；tso 1通过，均0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态，不计行为测试 |
| `cargo fmt --all`及`cargo fmt --all -- --check` | 0 | 静态，不计行为测试 |
| `cargo check --locked -p astersql-pd-compat-tests` | 0 | 静态，不计行为测试 |
| `cargo check --workspace --locked` | 0 | 静态，不计行为测试 |
| `cargo test --workspace --locked` | 0 | 1928通过、1既有忽略 |
| `bash -n scripts/run-pd-compat-readonly.sh`、Python AST解析、`git diff --check` | 0 | 静态检查 |

以上均在仓库根、默认features执行；没有Go/make/failpoint测试入口。聚焦测试中的缺oracle子进程非零并明确诊断PD_COMPAT_GO_BIN；无效模式/不可执行/重复二进制/缺失及非法fixture边界通过。

真实错误与取消：关闭自动下载后首次发现PATH的Go为1.24.3，fixture明确失败（入口101），报告确认只读守卫及目录清理仍通过；改为选择已有1.26.8缓存后真实双实现重跑通过。macOS取消时还发现向已退出但尚未回收的组直接发送SIGKILL返回EPERM，现先回收并检查非僵尸成员，再按需升级信号；清理失败也保留诊断与报告。最终通过实际入口启动服务后发送SIGTERM：dual中Go场景及rust场景各1次，均130退出，ps核对组内无存活进程、独占目录已删除、Go及输入哈希不变。验证脚本和两个结果在压缩证据内；不是mock调用次数测试。

最终真实Go二进制SHA-256：`b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`；Rust二进制SHA-256：`87fd89215d957a5e428b8d2b91a50e2d3ebed0542dc3e639a0be9cede5812e6c`。

开始/结束Go保护集合1983文件，路径、每文件SHA-256和Git状态完全相同，包含ignored/untracked；原始排序JSON快照SHA-256均为`0869cd94b87b15dd00db0a523c61e6ea30d361c99277c45413399f762ff088c4`。所有入口最终报告readonly_unchanged及inputs_unchanged为true。plan.md无差异，无Go/模块变更，无领域实现扩张；仅保留原有任务1–3删除及按要求删除本任务文件。
