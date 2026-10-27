# 任务140：领导者驱逐配置传输适配

状态：已完成。已核实批次139提交 `1f4690078`、[依赖证据](task-139.md)，
及任务075/076的真实驱逐调度和配置事务API。两级plan.md和来源主题只读。
本批次串行执行，没有子代理或其它批次实现。

## Oracle与适用范围

依用户后续明确授权，采用**只读Go源码/测试推导 + Rust真实行为及故障验证**；
没有运行Go测试或声称Go运行差分。[provenance](logs/task_140/provenance.json)
固定来源commit `836fdc7cf3ef34f7eabc9d051418995bb97efabe`、Go来源SHA256及
基准一致性、依赖139/075/076证据、oracle索引和[fixture](fixtures/task_140.json)哈希。
Go运行输出及其哈希明确为null，期望没有从待测Rust实现生成。

逐项只读审查evict_leader.go的updateConfig/parseUpdateStoreIDs/deleteConfig/
listConfig/newEvictLeaderHandler，及apiutil.ReadJSONRespondError/ErrorResp、
errcode v0.3.0的JSONFormat/ClientData、pingcap/errors的MarshalJSON。
TestEvictLeader覆盖迁出1到2/3及限速；TestEvictLeaderWithUnhealthyPeer覆盖pending/
down排除；TestConfigClone覆盖范围与batch克隆；TestBatchEvict覆盖3/5/100批量；
TestEvictLeaderBatchLimit直接覆盖HTTP合法/越界/小数/字符串batch及状态保持。
前四项不代替HTTP证据，也未将这些Go测试名称当作运行结果。

JSON仅归一化缩进、尾随换行和对象键序；状态、消息、error data精确比较。
I/O故障的诊断栈使用真实Rust backtrace，语言相关栈帧位置及其原生表示归一化；
错误code/msg/data、状态及状态变更仍验证。MetaStore故障保留实际Rust后端错误，
不伪造Go后端错误字符串。时钟固定10^12ns，随机取首个候选且不打乱；
不声称Go map/随机遍历顺序一致，默认features下验证共享领域行为。

边界沿用076：有效存储schema批量1–100，HTTP拒绝不可表示的2^64 ID；Go在该
超范围float64转换上的平台相关行为没有被声明为等价。未穷举任意JSON语法
诊断、Gorilla路径清理/编码跳转和损坏持久化schema。完整server/api装配属主题7。

## 实现和行为证据

生产入口[serve](../../../pkg/server/schedule_admin_evict_leader.rs)接受真实
EvictLeaderScheduler、BasicCluster、StorageEndpoint、ClusterPaths、HTTP request
及宿主移除回调，独占借用覆盖事务I/O；[领域说明](../../../pkg/server/schedule_admin_evict_leader.rs.md)。
只注册本任务生产模块与独立测试，增加已有集中声明的core-constant测试依赖；
Cargo.lock仅增加一条本地依赖边，无远程升级、feature、新crate或私有runtime。

POST `/config`、GET `/list`、DELETE `/delete/{store_id}`分别适配已有canonical API。
保留参数校验顺序、map字段大小写敏感、重复字段最后值、null/未知字段保存、
空ranges保留旧范围、URL解码、batch边界、配置JSON与删除回调错误映射。
非最后删除返回null；最后删除返回消息；未知路径404明文、方法不符405空body。
请求体读取失败返回500，即使已取得完整JSON也不应用；JSON语法/类型错误返回
400并设置TiDB-Error-Code及Go errcode数据。list克隆空范围为null，不修改快照。

[独立测试](../../../pkg/server/schedule_admin_evict_leader_test.rs)共4函数、63有效案例组：

- `task_140_behavior_cases`：48项固定HTTP请求，逐项核对状态/body/content-type，
  方法错误无Allow；所有拒绝请求保留原配置。含默认/最终list、单/批量ID、去重、
  batch上下界/类型/小数、互斥ID、非法ID、空/null/ranges、奇数尾、重复字段、
  非法URL escape、错误根JSON/空body、删除参数语法/溢出/缺失/非最后/最后store。
- `task_140_updated_scheduling_cases`：5组POST更新→真实内存调度→重载后调度，
  10次真实决策。验证包含/排除/开放边界、batch1/100、持久化字节；实际操作为
  region1、LEADER、URGENT、单步TransferLeader 1→2，错误为空。
- `task_140_transactions_and_faults`：7组，真实MemoryStore外部I/O故障验证更新
  保存失败回滚；真实缺失store999验证中途暂停失败回滚；保留已有独立IN/OUT
  暂停引用；删除保存失败500、最后store回调失败500恢复内存/暂停但存储仍为空、
  NotFound回调404且不恢复、恢复保存及重载。只控制I/O边界，不mock调度结果。
- `task_140_body_read_and_clone`：3组，完整/截断JSON后的真实流错误均500，
  无配置/存储/暂停副作用，验证internal错误header/code/msg/data和非空真实栈；
  空范围list序列化为null，领域编码保持不变。

## 红绿与完整验证

[red](logs/task_140/red.json)：fmt退出0，同一注册行为测试执行1项，因冻结
`/list`期望200、实际404而失败，exit101（不是缺API/零测试）。
[green](logs/task_140/green.json)1项、45案例通过；增加故障测试时
[expanded](logs/task_140/expanded.json)保留重复导入编译错误，不当作行为红灯；
修正后[corrected](logs/task_140/corrected.json)3项通过，扩充边界后
[edges](logs/task_140/edges.json)4项通过。最终检查见[final](logs/task_140/final.json)。
命令、退出码、有效测试摘要、原始合并stdout/stderr的gzip及SHA256均逐条保存。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo test -p astersql-pd-server --locked --lib task_140 -- --nocapture` | 0 | 4通过、0失败、0忽略 |
| `cargo test -p astersql-pd-server --locked` | 0 | 67通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1867通过、0失败、1忽略 |

每阶段[verify.py](logs/task_140/verify.py)通过原子mkdir动态领取槽位1；实际
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner含计划、任务、会话与PID；正常、失败、中断均等待子进程组退出后清理自己的
owner/锁，保留编译缓存。Rust修改后先cargo fmt --all。

[Go拦截器](logs/task_140/reuse-go/go)拒绝实际Go命令，只按预先验证SHA256复用
既有etcd I/O fixture二进制，未构建/测试Go或启停failpoints；每次复制记录于
[go-reuse](logs/task_140/go-reuse.jsonl)。二进制SHA256为
`57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5`。

初始磁盘仅131MiB可用。依用户授权，核实持有PID已退出且路径无活跃使用，
仅回收140份与保留副本完全同哈希的重复测试fixture可执行文件，逻辑约5GiB。
[首次](logs/task_140/space-recovery.json)及[补充](logs/task_140/space-recovery-2.json)
记录路径、PID、哈希、字节及进程证据；保留源fixture、所有共享槽位编译缓存和活跃锁。

[审计](logs/task_140/audit.json)核对1101份Go/模块文件集合、SHA256及Git状态，
包括untracked扫描；两份只读plan哈希不变，远程依赖版本不变，Markdown链接及
`git diff --check`通过。其它任务既有删除不属于本任务提交；140完成后删除编号文件。
