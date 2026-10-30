# 任务067：Region调度管理HTTP

基准 `db537b63482a8bede17e1e62bc868fb3c42358d2`；依赖[任务066](task-066.md)及任务001–004已核验。默认features，kvproto仍固定 `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。plan.md只读，Go及模块文件只读。

## 来源与实现

来源：[source-inventory.md](source-inventory.md):105、255、1494、2596；[wire-inventory.md](wire-inventory.md):242–245的HTTP-110/111/112/113四行全部覆盖。[surface-inventory.md](surface-inventory.md):4836–4839共享审计来源仍归任务81；source清单513–515是同文件的lookup/scan/keyspace共享注入定位，不将其算成本任务的新故障差分。

[HTTP适配](../../../pkg/server/http_region_schedule.rs)通过[原router](../../../pkg/server/http.rs)挂接单范围/批范围accelerate-schedule、scatter和split，在[lib.rs](../../../pkg/server/lib.rs)注册[独立测试](../../../pkg/server/http_region_schedule_test.rs)。[HTTP测试客户端](../../../pkg/server/http_region_schedule_client_test.rs)供server与runtime共享场景引用，workspace测试编译覆盖两个crate。Application和RoleServices透传到[原调度队列](../../../pkg/runtime/schedule_scatter.rs)，不创建新coordinator、controller、MetaStore或worker。

Go只读映射：server/api/region.go:748–940；pkg/schedule/handler/handler.go:1113–1265；pkg/schedule/scatter/region_scatterer.go；pkg/schedule/splitter/region_splitter.go；pkg/utils/apiutil/apiutil.go的ParseKey和ReadJSONRespondError；pkg/utils/typeutil/conversion.go的JSONToUint64Slice。

范围加速复用canonical Handler的accelerate_regions_schedule_in_ranges，在leader fence内按每个范围独立限额，将region ID加入同一pending-processed队列，保留去重及绕过普通重试队列容量检查的语义。该队列不同于regions/replicated读取的rule-checker pending list，成功响应不表示已经产生或完成operator。宿主新增的pending_processed_regions只读快照在同一队列内验证leader fence，供嵌入宿主观测真实待检查状态。

scatter复用已验证的ScatterRequest、canonical scatter算法、admission及heartbeat dispatch。新增的可选KeyRange在同一coordinator内扫描和调度，避免HTTP先扫描再按ID重新读取。gRPC继续传None，既有单region语义不变。HTTP范围字段同时为字符串时优先于IDs；空、缺失、类型错误、非法hex、重试截断、部分失败百分比遵循Go契约。公开并复用[既有JSON数字转换](../../../pkg/schedule/handler/admin_parse.rs)，operator管理和本路由共用已对齐amd64 Go oracle的截断/越界语义；不是另写一份饱和转换。测试实际heartbeat注册ID=2^63的region，再用1e100及2^64请求scatter，必须命中该region并返回部分处理结果，不能把不存在ID的统一错误当作转换证据。超大retry_limit也通过真实HTTP验证。HTTP响应只暴露processed-percentage，不伪造逐ID失败列表。

split复用现有SplitRequest和非阻塞SplitRetry驱动；只有实际heartbeat边界变化才计为完成。保持split_keys校验优先级、bootstrap/hex错误优先级、processed-percentage及regions-id。HTTP请求取消令牌传到既有观察驱动，宿主取消由同一生命周期回收。

## 当前行为证据

初始失败命令可编译并运行1项测试，split HTTP误落入gRPC fallback，Content-Type断言实际为application/grpc，退出101，非编译失败。扩展真实Go差分发现冷启动非法batch输入仍优先500/ErrNotBootstrapped；现把四条路由共同的bootstrap门禁前移到JSON解析前，并纠正初始测试从handler源码推导的400预期。后续测试记录保留了复用scatter/split场景时的epoch与残留operator冲突、错误地用rule pending观察加速队列，以及取消时通用JSON helper对纯文本响应的错误假设；最终分别用递增epoch、显式清理已测试的operator、canonical队列快照及原始TCP/状态码观察修正，没有更改Go行为来迁就断言。重复差分还捕获Go active operator移除与records发布之间的短暂NotFound（operator_controller.go的RemoveOperator/GetOperatorStatus分别操作两个容器）；共享测试在原15秒截止内轮询至所有operator的Success，永久缺失仍会失败。

[共享scatter场景](../../../pkg/server/grpc_scatter_scenarios_test.rs)增加HTTP入口，仍通过真实GetOperator和RegionHeartbeat观察两项admin,leader operator，确认发送transfer-leader、回报目标leader后状态Success。HTTP批次包含3个有效region和1个缺失ID，百分比75；范围scatter继续使用同一算法。

[共享split场景](../../../pkg/server/grpc_split_regions_scenarios_test.rs)增加HTTP入口，确认region-splitter Running operator、真实split-region心跳命令及准确split keys；请求在收到命令后仍未完成。真实回报边界后完整请求100%/ID102；只回报两个目标中的一个时，等待canonical观察超时后50%/ID103。客户端HTTP超时与RPC deadline均不制造新边界，之后GetRegion仍为已有region。HTTP与gRPC共用原端口；RPC读取与心跳不是mock调用次数。

Rust额外验证：follower单范围limit=1后leader的真实pending-processed快照为[10]；两个相同批范围只新增11，快照[10,11]；follower scatter错误/部分结果可观察。真实etcd配置事务注入非法lease导致拒绝，持久配置不变、pending队列保留。再挂起真实配置事务，使加速请求在同一队列等待，宿主取消后HTTP500、事务不提交、worker退出。重启复用同一MetaStore，region元数据保留，volatile pending队列清空。split成功后的101/102/103均经StorageEndpoint读取持久状态确认。Rust故障注入证据不冒充Go failpoint差分。SplitRetry的超时前/截止时、剩余key重试和取消保留进度继续由canonical handler的task_126测试使用显式毫秒时钟推进；真实TCP/Go差分另等待实际观察窗口，不能用推进Rust时钟冒充Go超时。

[只读Go监督入口](../../../scripts/http-region-schedule-oracle.py)复用任务004的binary校验、完整Go/模块哈希与Git守卫、进程组清理；显式执行默认ignored差分项并强制1通过、0失败、0忽略。输入冻结包括Rust/Cargo文件及监督脚本，执行结束逐项复核。Go oracle是预先存在的bin/pd-server，其build info没有VCS revision，不声称由当前HEAD构建；真实SHA-256/build info随原始证据保存。最终54项观察（HTTP响应与operator摘要）逐项相等。HTTP状态、错误结构/字节偏移、Text及JSON结果逐项比较；随机scatter目标只比较实际operator kind、数量、发送命令与完成状态；共享RPC记录仅按原场景归一化cluster ID。外部etcd fixture不代表Go embedded-etcd拓扑等价。

## 最终验证

状态：已完成。

所有Cargo使用原子mkdir动态领取的槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。最终环境`CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=4 GOMAXPROCS=2`；监督入口另设`PD_COMPAT_GO_BIN=$PWD/bin/pd-server`及`PYTHONDONTWRITEBYTECODE=1`。Rust修改后运行cargo fmt --all。

| 确切命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| 初始cargo test --locked -p astersql-pd-server --lib task_067_ -- --nocapture | 101 | 1失败、0忽略 |
| 最终cargo test --locked -p astersql-pd-server --lib task_067_ -- --nocapture | 0 | 3通过、0失败、1默认忽略 |
| python3 scripts/http-region-schedule-oracle.py | 0 | 1通过、0失败、0忽略；54项Go/Rust观察 |
| cargo metadata --format-version 1 --no-deps | 0 | 静态检查 |
| cargo fmt --all -- --check | 0 | 静态检查 |
| cargo check --locked -p astersql-pd-server | 0 | 静态检查 |
| cargo check --workspace --locked | 0 | 静态检查 |
| cargo test --workspace --locked --no-run | 0 | 测试编译检查，不计行为测试 |
| cargo test --locked -p astersql-pd-server -- --nocapture | 0 | 227通过、0失败、28忽略 |
| cargo test --workspace --locked | 0 | 2127通过、0失败、65忽略 |

默认忽略的任务67差分由监督入口显式运行，不把ignored计为通过。最终1291个Rust/Cargo输入在所有检查、聚焦、差分、包级及workspace验证后逐项哈希一致；监督另冻结两个Python入口。初始及最终1101个Go/模块文件（包含ignored/untracked）的路径、SHA-256和Git状态完全相同；plan.md原字节不变。Go二进制SHA-256为b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc，结束时再次匹配。

没有执行Go源码写入、Go测试、make、failpoint启停、生成或模块整理。Python AST、本文相对链接及git diff --check通过。开发阶段的行为失败、Go差分失败、SIGTERM监督清理、旧轮次源码冻结拒绝及跨crate测试编译失败均保留；最终全轮基于同一组冻结输入，未删去失败记录。任务58原删除与prompt.md原修改不纳入本任务提交。

真实服务、fixture、worker及oracle进程组全部结束；剩余槽位/服务进程为空，自己的持锁进程正常退出、锁已释放，槽位缓存保留。[压缩原始证据](task-067-evidence.json.gz)，SHA-256 `2eaf9a3032733ef9cea96482501ec77bb578b827a7e4f376e54e71e9fab9d3f3`。
