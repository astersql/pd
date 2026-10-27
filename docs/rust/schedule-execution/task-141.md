# 任务141：慢节点驱逐配置传输适配

依赖140提交 `a3486f938` 和[证据](task-140.md)已核实，复用079磁盘、081网络
驱逐及真实core/placement/operator/storage API。两级plan及来源主题只读。

## 来源和边界

依用户后续明确授权，采用**只读Go源码/测试推导 + Rust真实行为及故障验证**。
没有运行Go测试、没有Go运行输出或运行差分主张。来源commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、Go源码/测试/索引及
[fixture](fixtures/task_141.json)的SHA256见[provenance](logs/task_141/provenance.json)。

审查evict_slow_store.go的newEvictSlowStoreHandler、updateConfig、listConfig、
persistLocked和ServeHTTP，以及config.go嵌入的baseDefaultSchedulerConfig，
apiutil.ReadJSONRespondError和140错误编码证据。本scheduler有独立handler，
故BaseScheduler的 `not implements` 分支不适用。

只读核实相邻Go测试：TestEvictSlowStoreTestSuite覆盖磁盘发现/恢复及网络启停/
持久化失败；TestEvictSlowStoreBatch覆盖3/5/100真实驱逐；BatchLimit直接
覆盖HTTP batch100成功和1.5拒绝；TestRecoveryTime覆盖连续恢复时窗重置；
TestCalculateAvgScore覆盖空/单值/多值/零分网络均值，不作为HTTP证据。
这些需failpoint的测试均未裸跑，列出名字不代表运行结果。

JSON只归一化键序、缩进和尾随换行，状态、消息和错误data不忽略。流错误使用
真实Rust backtrace，不比较语言相关栈帧；I/O故障保留真实MetaStore错误。
时钟10^12ns，随机按递增候选索引、不打乱，不声称Go map/随机遍历顺序一致。
超出uint64可表示范围的浮点转换、任意JSON解析诊断及Gorilla路径清理/重定向
未穷举。完整server/api挂载属于主题7。

## 实现与行为

入口[serve](../../../pkg/server/schedule_admin_evict_slow_store.rs)接受真实disk/network
owner、StorageEndpoint、ClusterPaths、宿主disabled值及相对HTTP请求，处理
POST `/config`、GET `/list`。未知路径404明文，方法不符405空body，无Allow。
无新依赖/feature/runtime。[领域说明](../../../pkg/server/schedule_admin_evict_slow_store.rs.md)。

依Go顺序校验恢复时长、batch和网络开关；缺失/null/未知字段仍保存，重复字段
最后值，非负恢复时长小数截断，batch整数1–100。输入错误400/input，流错误
500/internal，保存失败500/字符串。保存失败回滚数值但保留network开关新值。

最小领域桥接update_with_disk成功保存完整envelope后同步两owner的数值策略，
保留disk驱逐/捕获时间、network恢复表/暂停/驱逐引用。宿主disabled保存原值，
请求字段不修改它；Go list构造零值嵌入base，因此输出disabled=false。
宿主负责串行驱动两个owner；独立observe的持久化及完整生命周期组合沿用
既有领域边界，不宣称本任务新增统一调度循环。

[测试](../../../pkg/server/schedule_admin_evict_slow_store_test.rs)：5函数、60组案例：

- task_141_behavior_cases：41组固定HTTP案例，含默认/最终list、错误类型、
  batch0/-1/101/1.5和1/100、null/未知/大小写、重复字段、2.9截断、校验优先级、
  错误根JSON完整响应及拒绝后状态保持。
- task_141_updated_scheduling：4组HTTP更新后真实磁盘调度，batch1/3/5/100，
  共109个操作器；持久化/解码/list/捕获时间，URGENT/LEADER、TransferLeader
  1→2/3、confver、影响量、命令和执行完成均核实。
- task_141_network_updated_scheduling：4组HTTP启用后真实网络检测/暂停/驱逐，
  共109个操作器；后续HTTP更新保留集合/时钟，保存集合及真实恢复无操作已验证。
- task_141_faults_and_body_errors：10组，I/O故障回滚及重试、6种错误JSON输入、
  完整/截断JSON后真实读取错误拒绝应用。
- task_141_disabled_envelope：1组，宿主disabled=true保存，请求false不覆盖，
  list固定false。

## 红绿、环境与验证

[red](logs/task_141/red.json)：fmt退出0，实际执行1项行为测试因/list404、期望200
断言失败，exit101；[green](logs/task_141/green.json)对应行为通过。
补查嵌入字段后[disabled-red](logs/task_141/disabled-red.json)同一行为测试因缺少
JSON disabled字段失败exit101。其余中间格式/编译错误和故障消息期望修正均保存，
不当作行为红灯。各阶段JSON包含确切命令/退出码/测试摘要/原始合并stdout-stderr
的gzip路径与SHA256。

初次workspace验证为补齐disabled主动中断；runner等待子进程退出后释放槽位。
final2包级因磁盘耗尽复制既有etcd二进制失败；final3因磁盘不足创建etcd WAL失败，
均未当作成功。先回收本任务两份已退出重复fixture（约73MiB），继而核实并回收
六份与保留源二进制同SHA256、无活跃进程使用的重复外部I/O fixture（约220MiB）。
[回收记录](logs/task_141/duplicate-fixture-recovery.json)，Rust编译缓存/源fixture/
活跃锁均保留。本任务etcd工作目录清理时发生磁盘写入失败，未伪造该次清理统计。

[Go拦截器](logs/task_141/reuse-go/go)拒绝实际Go命令，后续仅硬链接复用已有
etcd二进制，SHA256为
`57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5`；
[复用记录](logs/task_141/go-reuse.jsonl)。未构建/测试Go、未启停failpoint。

final4全量测试揭示复用脚本需支持同进程反复请求同一二进制路径（FileExistsError），
已修为目标存在时重新校验哈希后幂等复用；没有修改业务实现。final5重跑workspace。

所有阶段由[verify.py](logs/task_141/verify.py)原子mkdir动态领取共享槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录计划/任务/会话/PID；正常、失败和中断均等待子进程组退出后释放锁，
保留缓存。Rust每次修改后先cargo fmt --all。最终阶段设置build jobs/test threads=2。

[审计](logs/task_141/audit.json)比较1101份Go/模块源文件集合、SHA256和Git状态，
及两份只读plan，均不变；扫描排除生成缓存目录，构建拦截器保证不调用实际Go。
Cargo.toml/Cargo.lock无差异，20个Markdown链接有效，git diff --check退出0。
其它编号任务已有删除与本任务无关，排除提交。

final5进一步遇到历史PID复用：目标文件存在但属于旧版本fixture。最终拦截器按
原有go build输出替换语义，先建唯一临时硬链接，再原子替换当前PID的输出路径；
保留源二进制，不写源字节，记录旧哈希。final6再次运行完整workspace。

final6运行至storage的etcd_contract时再次因空间不足创建WAL失败。补充核实并
回收64份相同SHA256的无活跃进程重复fixture（二进制逻辑合计2,458,488,832字节），
[记录](logs/task_141/duplicate-fixture-recovery-2.json)；原件与Rust编译缓存均保留。
最终final7使用RUST_TEST_THREADS=1、CARGO_BUILD_JOBS=2重跑完整workspace，
不筛掉失败测试，也未降低验收要求。

final7已越过etcd契约，最后在无关TSO进程重启测试失败：其子进程stdout按
`PROCESS_TSO `行前缀解析，RUST_TEST_THREADS=1使libtest在同一行先输出测试名，
故子进程成功却找不到时间戳。只读审查pkg/tso/etcd_behavior_test.rs后恢复线程数2；
未修改TSO代码，final8先运行该单测再运行整个workspace。上述环境失败均保留原始日志。

## 后续批次可复用的安全验证方式

可审核源二进制为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1/rust-etcd-fixture-2647`，
SHA256见上文。它来自既有Go fixture，不重新构建或修改任何Go/模块源文件。
后续批次仍须动态领取自己的槽位；先验证源哈希，必要时在自己槽位中建立同哈希
硬链接副本，再设置PD_EXISTING_ETCD_FIXTURE及其SHA256。所有执行目标均落在
当前持有槽位，不改写其它槽的源文件。包装器仅接受既有fixture的go build调用形式，
用唯一临时硬链接和原子replace复用输出；目标不得等于保留源。这样不会因为PID
历史复用而使用旧二进制，也不会产生每次约37MiB的独立物理复制。

出现空间不足时先检查现有进程和槽位锁；仅回收与保留源同大小/同SHA256且没有
进程使用路径的冗余外部I/O fixture副本，记录路径、哈希、大小和保留源。保留Rust
编译缓存、源fixture和活跃锁。可从保留源重建同哈希硬链接，回收操作可恢复。
不要将此过程扩展到未知测试数据、不同哈希的产物或未核实的活动槽位。

不要为降低磁盘并发把RUST_TEST_THREADS设成1：既有TSO子进程测试依赖多线程
libtest输出格式。本任务已验证线程数2下该测试通过；优先避免重复复制，并保留
足够空间给etcd WAL。所有复用/恢复均有本目录日志，不宣称重新运行了Go oracle。

## 最终结果：已完成

最终[final8](logs/task_141/final8.json)完整workspace退出0，
[汇总](logs/task_141/final-summary.json)为1872通过、0失败、1既有忽略项。
此前环境失败已突破，不使用“待回归”替代验收。

| 命令 | 退出码 | 有效测试/结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-server --locked --lib task_141 -- --nocapture` | 0 | 5通过，60组案例，0失败/忽略；final4 |
| `cargo test -p astersql-pd-server --locked` | 0 | 71单元+1集成=72通过，0失败/忽略；final4 |
| `cargo metadata --format-version 1 --no-deps` | 0 | final4 |
| `cargo fmt --all -- --check` | 0 | final4 |
| `cargo check --workspace --locked` | 0 | final4 |
| `cargo test -p astersql-pd-tso --locked --test etcd_behavior real_etcd_oracle_process_restart -- --nocapture` | 0 | 1通过；final8，线程数2 |
| `cargo test --workspace --locked` | 0 | 1872通过，0失败，1既有忽略；final8 |

final4之后只修改验证脚本及证据，没有改动业务/测试Rust代码。
最终槽位1，绝对CARGO_TARGET_DIR见上文；命令及所有槽位子进程均已结束，锁已释放，
编译缓存保留。完成后删除编号任务141；稳定证据和安全复用流程保留于本文件。
