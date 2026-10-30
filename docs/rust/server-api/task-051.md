# 任务051：Store查询HTTP

状态：已完成（编号任务规定的 Store 查询边界）。

基准 `83a72ac35`；依赖[任务050](task-050.md)完成。Go/模块文件及 plan.md 全程只读。

## 实现与来源

[HTTP适配](../../../pkg/server/http_store_read.rs)接通同一 listener 的 GET `/store/{id}`、`/stores`、`/stores/check`、`/labels/stores`（均有 `/pd/api/v1` 前缀）。复用 PdServices、ClusterReadService、canonical StoreInfo、活动 schedule 配置；不另建集群缓存，不从 protobuf 默认值构造固定业务响应。单次响应复用同一配置与时钟观察，容量、权重、分数、计数、心跳、慢分数和控制状态来自领域对象。继续使用既有 leader 转发和请求取消链。

默认 stores 只接受 Up/Offline，数字 state 支持重复参数并按 Go int64→int32 转换；check 使用大小写无关状态名，空参数不筛选，动态 Down/Disconnected 来自现有时间判断。labels/stores 匹配同一标签的 key/value，包含 Tombstone；支持不区分大小写正则，并适配 ASCII Perl 类、引用字面量、八进制、字符类、重复上限及所测错误文本。非法 ID 保留 Go FieldError 的空对象 data 和错误头；未 bootstrap、缺失 Store、非法 state 各自保持状态码和错误 JSON。

来源映射：

- [source-inventory](source-inventory.md)：任务51；`server/api/store.go` 的 GetStore/GetAllStores/GetStoresByState/NewStoreStateFilter/Filter，`server/api/label.go` 的 GetStoresByLabel/NewStoresLabelFilter/filter。
- [wire-inventory](wire-inventory.md)：HTTP-062、069、074、076；当前证据更新至本文件。
- [surface-inventory](surface-inventory.md)：router.go:225/234/239/243 注册行；该表的 audit 仍归81，不冒称已实现审计。
- 补充只读 oracle：`pkg/response/store.go::BuildStoreInfo`、`pkg/core/store.go`、`pkg/utils/apiutil/apiutil.go::ParseUint64VarsField`，已有 Go 工具链 `regexp/syntax/parse.go`。

盘点表的旧 owner 与编号任务目的不一致，本次局部修正：labels/stores 依任务51明确要求归51；stores/progress 随 Store 生命周期归52；regions/store 依任务56明确目的归56；hotspot/stores 依任务60明确目的归60。这三项仍标缺证据，未将它们算成本次实现；labels 列表和标签/权重写入继续归53。没有修改这些后续任务或 plan.md。

## 行为验证

[独立测试](../../../pkg/server/http_store_read_test.rs)通过真实 TCP、tonic 和同一 etcd MetaStore bootstrap/注册/心跳，再从 HTTP 观察完整 Store JSON；覆盖 Up/Offline/Tombstone、动态状态、默认与重复过滤、空结果、非法和溢出 ID/state、URL 解码、标签成功/错误及真实统计更新。可控时钟案例验证20秒 disconnected和配置60秒 down的严格大于边界，Offline 不被时间覆盖。

第二个真实节点验证 follower 转发；比较 `/raft/s/` 持久条目前后内容。配置事务在真实 MetaStore 边界暂停并持有集群资源锁，Store GET 确实等待；取消节点后 GET/写请求退出，shutdown/join 完成且条目不变。取消响应由既有外层 middleware 生成，按实际 plain-text 500 验证，不误认成正常 JSON 响应。关闭两节点后以同身份和地址重启，验证 Tombstone、标签恢复。GET 本身读取内存共享视图，不为制造存储故障添加生产持久化调用。

首次红灯：`cargo test --locked -p astersql-pd-server --lib task_051_ -- --nocapture` 编译可运行，实际1项测试因缺路由的 Content-Type 为 application/grpc 而断言失败；退出101。随后编译错误、测试工具问题和差分失败均保留在原始证据，不作为红灯替代品。

[只读差分入口](../../../scripts/http-store-read-oracle.py)复用任务004守卫和现有 `bin/pd-server`。两端运行相同场景并比较状态与完整 JSON。仅排序无序 Store 数组、将同值整数/浮点数统一、按 RFC3339 时间点比较本地时区/UTC；心跳的三个进程时钟字段先验证时间范围、metadata与时间文本严格一致、uptime关系，再替换为已验证标记。其余错误及业务字段不忽略。Go binary build info 无 VCS revision，不宣称来自当前 HEAD。

## 最终验证

所有 Cargo 命令均动态原子领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，默认 features、`RUST_TEST_THREADS=4`。锁记录任务/计划/会话/PID；runner 对失败和中断清理自己的进程组和锁，保留缓存。中途因正则边界补充主动停止过一次未完成回归，其部分结果不代替下面的最终全量证据。

Rust 修改后先执行 `cargo fmt --all`（退出0），再执行：

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查通过 |
| `cargo fmt --all -- --check` | 0 | 静态检查通过 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查通过 |
| `cargo check --workspace --locked` | 0 | 静态检查通过 |
| `cargo test --locked -p astersql-pd-server --lib task_051_ -- --nocapture` | 0 | 3通过、1默认忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 178通过、12默认忽略 |
| `cargo test --workspace --locked` | 0 | 2070通过、49默认忽略 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-store-read-oracle.py` | 0 | 内部 `cargo test --locked -p astersql-pd-server --lib task_051_differential -- --ignored --nocapture` 1通过、0忽略；46个HTTP观察场景 |

默认忽略不计为通过；本任务差分已单独实际运行。开始/结束1101个Go/模块文件（含ignored/untracked）的路径、SHA-256和Git状态一致；plan.md与HEAD字节一致。最终验证期间1245个Rust/Cargo文件SHA-256未变。Python语法、证据链接和git diff --check通过。自己的锁和子进程已清理，缓存保留。

[压缩原始证据](task-051-evidence.json.gz)包含红灯/差分迭代/最终命令输出、退出码、二进制构建信息、文件守卫与槽位runner。SHA-256：`9502e610c8cc3140fd1403b3bb5e99d4525cf87ac5b4be587a245e6c3d3d82e7`。
