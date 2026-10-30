# 任务070：Region标签规则HTTP

状态：已完成。基准 `c8d8365b3`。依赖[任务069](task-069.md)及[任务004](task-004.md)的只读oracle入口已核验；Go源码、Go模块文件及plan.md只读。默认features，协议仍固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

## 来源与接线

[source-inventory.md](source-inventory.md):108为本任务摘要；全部owner含70的来源行是209–225、256、412–494、516、1605–1637、2526–2551、2597（共享keypath/持久路径与labeler扫描，保留原盘点哈希作为基准，不把共享所有者的其他行为列为本任务新增验收）；831–832是同一持久路径的汇总说明。labeler扫描范围内既有领域实现全部复用。[wire-inventory.md](wire-inventory.md):186–193，任务70负责HTTP-054–059及061；既有HTTP-060单key查询由任务55实现，本次验证动态写入确实影响该入口。[surface-inventory.md](surface-inventory.md):4780–4787为共享审计前置，本任务不额外宣称81/82审计/限流验收。

只读Go行为来源：`server/api/region_label.go`的八个方法、`pkg/schedule/labeler/{labeler,rules,rule_index,plan}.go`；原Go测试仅只读参考，未执行make、failpoint或Go源码生成。

[HTTP适配](../../../pkg/server/http_region_label.rs)使用原[router](../../../pkg/server/http.rs)、PdServices、active leader与同一调度命令队列。[宿主适配](../../../pkg/runtime/region_label_http.rs)持有当前term admission锁，复用原调度任务加载的RegionLabeler和TermStore；读写都在同一任务上串行处理，调度器借用的Arc只在一次schedule调用内存活，写入通过Arc::get_mut取得唯一owner。没有平行标签实现、私有runtime或全局状态。存储提交成功后由既有领域层发布索引；原单key查询和新增全部标签查询立即看到同一状态。

编码ID使用escapeRouter契约，仅一次PathUnescape并保留加号；canonical labeler校验后把range data替换为已验证的KeyRangeRule序列，丢弃Go同样丢弃的未知JSON字段，保留原十六进制大小写；该差异由真实双实现观察发现并补领域回归。JSON解析复用既有Go结构校验器，增加labeler字段schema，patch envelope保留重复字段顺序、大小写和null语义。DELETE成功使用Go的text/plain响应，其余成功/错误使用JSON与既有input错误header。先检查bootstrap，后解码请求。

## 行为证据

初始真实TCP测试可编译运行，缺路由返回application/grpc，JSON Content-Type断言失败；退出101，1失败、0忽略。初稿测试误用了POST /rules，随后修正为Go支持的PATCH；使用基准c8d8365b3原有生产Rust字节、仅注册最终独立测试，重放有效PATCH行为失败；恢复本任务生产源码后再做全部最终验证。此基准回放不修改Go文件、不引入桩或人为替代响应。后续开发期泛型patch envelope移动到server，避免给runtime增加serde依赖；编译失败不计行为失败证据。

Go会创建keyspaces/0默认标签，当前Rust的keyspace启动尚未提供该初始化。差分先记录该差异，再通过双方同一PATCH `{"deletes":["keyspaces/0"]}`建立明确一致的规则初态；删除为幂等且双方都返回200，未归一化或删除响应中的规则字段。不宣称本任务验证了keyspace默认初始化。其余差分仅对Go无序的全规则和全标签列表排序，按ID查询的顺序与重复项原样比较。

成功、校验错误、编码ID、请求ID顺序/重复、缺失规则、批量last-wins/sets-over-delete、整批校验失败不发布、TTL原文、过期规则拒绝、跨范围边界空标签、全部/单key读取和同MetaStore重启均由[独立测试](../../../pkg/server/http_region_label_test.rs)验证。另用可控纳秒时间在真实etcd上验证TTL恰等于到期仍可见、晚1ns消失、读过滤不删除持久记录。真实后端注入非法lease验证批量及单条更新失败后内存/持久状态不变；挂起真实事务后取消宿主，HTTP500、持久快照不变并完成shutdown join。故障与取消不冒充Go failpoint差分。

全工作区首轮在既有[singleflight_randomized](../../../pkg/utils/syncutil/ordered_single_flight_test.rs)失败，退出101：通用register helper断言首次poll必须Pending，但多线程合法快速完成会返回Ready。单独cargo复跑1次通过，随后直接运行同一槽位测试二进制的第1次就重现相同断言失败，非本任务HTTP路径引发。按阻塞修复授权，仅修改随机测试的取消分支：Pending则drop取消，Ready则继续校验成功结果的时间序。执行计数、互斥执行、结果先后、capture释放、最终排空等原断言全部保留；不修改syncutil生产实现。修复后包级9单元+2编译期doc-test通过，随机测试额外200次均恰好1通过、0忽略。此修复独立提交 `4d38608fc`；此前HTTP实现及其聚焦/差分/包级验证的全部生产与测试字节保持不变，仅最终全工作区输入新增这一项测试修正。

## 验证记录

实际动态领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。Cargo及监督入口继承该路径，环境CARGO_BUILD_JOBS=2、RUST_TEST_THREADS=1、GOMAXPROCS=2。Go差分复用已有bin/pd-server，不重建Go服务；fixture仅按任务004既有只读调用链构建etcd工具。Go二进制构建信息没有VCS revision，不宣称来自当前HEAD；外部etcd不等同Go embedded-etcd拓扑。

原始输出、初始失败、有效PATCH基准回放、开发差分及最终源码/Go哈希保存于[压缩证据](task-070-evidence.json.gz)。原始文件不把doc-test零测试或ignored计为通过。

| 确切命令 | 退出码 | 有效测试数量 |
| --- | ---: | --- |
| 基准 `cargo test --locked -p astersql-pd-server --lib task_070_behavior -- --nocapture` | 101 | 1失败、0忽略 |
| `cargo fmt --all` | 0 | 静态检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server --lib task_070_ -- --nocapture` | 0 | 4通过、0失败、1忽略 |
| `python3 scripts/http-region-label-oracle.py` | 0 | 1通过、0失败、0忽略；103项HTTP观察及双方重启快照一致 |
| `cargo test --locked -p astersql-pd-schedule-labeler -- --nocapture` | 0 | 38通过、0失败、0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 237通过、0失败、31忽略（236单元+1集成） |
| 首轮 `cargo test --workspace --locked` | 101 | 既有随机测试1失败，完整日志保留，不计为整轮通过 |
| `cargo test --locked -p astersql-pd-syncutil -- --nocapture` | 0 | 11通过、0失败、0忽略 |
| `$CARGO_TARGET_DIR/debug/deps/astersql_pd_syncutil-7894a0de13c7e69e ordered_single_flight_test::singleflight_randomized --exact --nocapture`（重复200次） | 0 | 每次恰好1通过，累计200通过、0忽略 |
| 修复后 `cargo test --workspace --locked` | 0 | 2138通过、0失败、68忽略 |

server默认忽略项包括30项既有测试及本任务需只读监督入口单独运行的1项Go差分；已单独运行并通过。批量保留领域层每120项分段事务的语义；后续分段失败可保留先前磁盘提交，内存仍不发布，不能宣称整个任意大小patch跨分段回滚。领域38项回归包含真实etcd的分段失败、内存保持及后续重载证据。

最终1303项Rust/Cargo/监督输入在修复后的整轮workspace前后SHA-256一致；此前聚焦、独立Go差分、server和labeler包级检查对应的HTTP生产/测试字节全部相同，唯一额外差异为上面明确记录的syncutil测试修正。全程1101个Go/模块文件（含ignored/untracked）的存在性、内容SHA-256和Git状态均与任务开始一致；plan.md与HEAD原字节一致。没有执行Go测试、make、failpoint启停、生成或tidy；未写Go源码或模块文件。

全部Cargo、服务、fixture与oracle子进程已结束，自己的槽位1 owner记录与锁目录已删除，缓存保留。监督报告记录process_group_clean、readonly_unchanged和inputs_unchanged；最终槽位进程检查为空。原有任务58删除及prompt.md修改不纳入提交。Python AST、Markdown相对链接和git diff --check另外核对。

[压缩原始证据](task-070-evidence.json.gz)，SHA-256 `c509fd51af9715f30ea505304cb62696f09758a29dc741fd4fb358953475e753`。相同快照用snapshot_ref/SHA-256去重，含完整恢复字典，已验证恢复后的记录相等及实际gzip/JSON往返相等；保留首轮workspace失败、单独复现、200次修正后随机运行、最终全轮输出/退出码/测试数、完整Go/源码哈希、Go构建信息与监督报告。
