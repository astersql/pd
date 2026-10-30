# 任务061：历史热点查询HTTP

基准 `9f487ece8f686b5ed520714d3d85cfe859e96201`。已读取任务1–4、任务060、三份清单与只读plan.md。本任务按继续执行授权补齐必要的canonical领域依赖，解除此前仅因“未实现历史存储”而记录的阻塞；不改来源史诗或plan.md。

## 来源与实现

- [source-inventory.md](source-inventory.md):21、99、244及2583的任务61/history来源；[wire-inventory.md](wire-inventory.md):211的HTTP-079与:401领域移交；[surface-inventory.md](surface-inventory.md):4805的history路由。审计/限流仍由81/82接线，此处复用现有请求生命周期与leader转发层。
- Go只读映射：[HTTP入口](../../../server/api/hot_status.go)、[请求/筛选/采样打包](../../../server/handler.go)、[历史存储、迭代、后台采样和清理](../../../pkg/storage/hot_region_storage.go)、[宿主创建](../../../server/server.go)。固定协议仍为kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。
- [后端中立领域](../../../pkg/storage/domain/hot_history.rs)在已有storage-domain crate定义记录、选择条件、存储及原子发布，经现有KV/Storage facade导出。没有创建平行服务，没有把proto/tonic/etcd类型引入领域层。
- [HTTP适配](../../../pkg/server/http_hotspot_history.rs)、[独立TCP测试](../../../pkg/server/http_hotspot_history_test.rs)、[宿主采样/清理](../../../pkg/runtime/hot_history.rs)、[独立时钟测试](../../../pkg/runtime/hot_history_test.rs)。lib.rs与runtime模块显式注册测试，HTTP路由只新增history入口。

复用现有外部etcd MetaStore及TermStore的lease/value/revision fencing。在`/pd/{cluster}/rust/hot-history/{member}/`下隔离成员历史，保持Go每个PD成员独立数据库的观察边界；leader默认转发与显式本地follower查询继续走已有中间件。查询无需bootstrap，读取不写MetaStore。

每批以真实事务revision取得唯一版本，分段写入暂存行，再以一个事务发布标记。查询以首次范围读的revision同时固定所有分页、类型及标记读取；后续历史读返回的当前header revision不会移动快照。仅已发布版本可见，同一(type,time,region)保留最后已发布值；失败暂存不会覆盖旧值。此机制允许超过MetaStore单事务预算的完整批次，避免把部分成功误作整体成功。清理按过期版本分批删除，并在最后一行删除后移除其发布标记；失败暂存也在相同保留窗口被清理。暂存/标记属于内部协议，不改变HTTP或kvproto协议。

采样复用HotCache、Handler::get_hot_regions及Region/peer元数据，保留读热度六倍阈值、Go按peer ID取is_learner的现有行为、毫秒报告时间、浮点负载与大写hex键。采样和保留清理由宿主已有任期worker拥有，prepare阶段也执行；关闭/失去任期时随现有worker取消并join，不新建线程、私有运行时或全局状态。配置更新保留已等待的ticker，新周期在触发后reset；保留清理按本地首次04:00及后续24小时ticker，截止时间按calendar days计算。

HTTP仅GET读取JSON body，HEAD/POST/PUT/DELETE按Go返回空405；OPTIONS按已有Go二进制的实际观察返回空200。忽略无关query及Content-Type；没有新增Go不存在的page/limit参数，分页只用于内部范围扫描。Region/Store/peer空集合不限；两个boolean空集合排除全部；空类型默认read/write，类型顺序和重复类型保留；无匹配返回`history_hot_region:null`。保留int64/uint64原始数值语法、溢出诊断、重复/大小写折叠字段及null语义，未知字段不因巨大数字被误拒绝。参数错误为JSON字符串400，存储错误为JSON字符串500，取消沿请求上下文传播。

## 验证与边界

初始真实TCP测试可编译运行，聚焦命令退出101，1个行为测试失败：路由缺失导致Content-Type为application/grpc，而非JSON。不是以编译错误、零测试或环境错误充当行为红灯。

真实外部etcd、TCP HTTP及tonic心跳验证140条跨页记录、全部过滤集合、重复类型/字段、空boolean集合、成员隔离、重启恢复和读取前后持久entry不变。另验证142行大批次在真实etcd事务绑定不存在lease时的暂存/发布故障，要求旧结果完整保留；后续完整发布成功。分页第二页边界注入真实并发写入，要求当前请求保留旧快照，下一请求观察新值。真实backend FutureRevision错误、损坏JSON和缺少密钥服务的加密记录返回500；阻塞真实读取后关闭宿主，要求500、shutdown完成且存储不变。

时钟测试使用Tokio可控单调时钟与显式墙钟值，验证旧ticker边界、reset及首次04:00/后续24小时。保留清理通过显式cutoff作用于真实etcd记录；这些是Rust运行证据，不冒充Go时钟推进差分。

[只读监督入口](../../../scripts/http-hotspot-history-oracle.py)复用任务004守卫、进程组清理和精确测试计数，单独执行默认ignored的Go差分。使用已有bin/pd-server（build info没有VCS revision，不声称由当前HEAD构建），双方真实心跳生成读/写历史并比较完整记录、所有过滤/类型顺序、参数/数值错误与重启结果。Go整数形式与Rust浮点形式的速率通过同一f64记录模型比较；不归一化ID、计数、boolean、数组顺序、hex键或毫秒时间，不使用不同服务替身。

本实现仅准入现有外部etcd过渡后端；不选择本地生产引擎、不宣称Go LevelDB字节格式或embedded-etcd拓扑等价，符合ADR-001。当前canonical加密库只有配置，缺少密钥服务时采样/查询明确失败，不把密文当明文、降级空数组或声称完整加密能力。后续加密领域准入后可扩展该边界。本任务未修改任何Go源码/模块、协议版本或feature开关；新增依赖能力限于chrono本地时区时钟、serde_json原始JSON值及锁定传递依赖。

## 最终验证（2026-10-30）

最终阶段动态领取槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。此前一次可控时钟阶段实际动态领取槽位2，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`；其余记录见原始日志。所有Cargo及调用Cargo的子进程继承对应绝对路径，没有--target-dir覆盖。最终全套串行验证使用默认features，CARGO_BUILD_JOBS=2、RUST_TEST_THREADS=4、GOMAXPROCS=2。

| 确切命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_061_ -- --nocapture` | 101 | 0通过、1行为失败 |
| `cargo fmt --all` | 0 | 静态检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-runtime --lib task_061_ -- --nocapture` | 0 | 2通过、0失败、0忽略 |
| `cargo test --locked -p astersql-pd-server --lib task_061_ -- --nocapture` | 0 | 5通过、0失败、1忽略 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-hotspot-history-oracle.py` | 0 | 1通过、0失败、0忽略；60组Go/Rust HTTP观察（39输入/方法、21历史过滤）及真实读写采样/重启 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 213通过、0失败、22忽略 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 2110通过、0失败、59忽略 |

开发中修正编译接口、GetRegion轮询的既有octet-stream响应、Go配置文件的启动读取时机、历史快照header revision漂移、ticker更新时机及HTTP方法边界。HEAD测试首次使用curl -X HEAD时因客户端错误等待不存在的body超时，改为显式Connection: close后验证；OPTIONS的405假设被Go实际200推翻，修正后重新运行全部验证。编译/脚本失败没有当作业务红灯或成功。最终60组差分全部通过，默认ignored差分由监督入口显式执行，不计入默认测试的通过数。

状态：已完成。最终1273个Rust/Cargo输入在fmt之后冻结，全部命令结束后逐文件SHA-256一致；开始/结束1101个Go/模块文件（含ignored/untracked）路径、字节SHA-256及Git状态一致，plan.md原字节哈希一致。Python AST、本文相对链接与git diff --check另作提交前检查。所有本阶段测试、服务、fixture及worker已结束，两个自己的槽位锁均已释放，缓存保留。保留任务58删除及prompt.md的用户已有修改，不纳入本任务提交。

[压缩原始证据](task-061-evidence.json.gz)包含红灯、开发失败、最终命令/退出码/计数、完整输出、Go差分与构建信息、Go和Rust/Cargo哈希清单、槽位及清理记录。SHA-256 `d8ac59a5ce5a91ea84f6891b74b55f6365e830c7ce3c4c2be1e772e5ce2fde19`。
