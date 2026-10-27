# 任务139：见证均衡配置传输适配验收

状态：已完成。依赖138的提交 `0b8054f17`、[证据](task-138.md)、真实
BalanceWitnessScheduler、任务073/074配置与调度API已核实。未以任务文件删除推断完成。
两个plan.md及来源主题均保持只读，仅实施本批次，没有并行任务或子代理。

## 来源及行为边界

按用户后续明确授权采用**只读Go源码/测试推导 + Rust真实行为和故障验证**，
替代缺失的Go运行oracle/差分；没有执行Go测试或声称Go运行输出。
[provenance](logs/task_139/provenance.json)固定来源commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、Go来源SHA256及基准一致性、
依赖138、074证据和fixture、oracle索引及[本任务fixture](fixtures/task_139.json)哈希。
Go binary/output哈希明确为null。源码期望独立冻结，不从待测Rust输出生成。

Go `balance_witness.go` 明确覆盖ServeHTTP，注册POST `/config`和GET `/list`。
返回JSON配置或字符串消息；未知路径404明文，方法不匹配405空body，无Allow。
正常batch范围1–10，默认4；无效batch回滚；合法更新先发布，保存失败仍返回200。
未知字段/空对象/null无变化返回400；确切JSON tag存在的相同更新返回200。
大小写不敏感赋值、重复字段按输入顺序、JSON类型错误可能保留其他字段的已赋值值。
KeyRange自定义解码错误立即停止；syntax错误先于赋值。请求体I/O错误被Go忽略，
仍解码已读字节。list通过clone将null ranges变成[]，存储保存原始null。

只读核实TestBalanceWitnessSchedulerTestSuite的两个子测试：
TestScheduleWithOpInfluence比较在途影响和评分容忍边界；TestTransferWitnessOut
比较4→1/2的见证转移和操作影响。这两个测试没有验证配置HTTP接口；没有把其
名称当作运行或HTTP证据。任务073/074真实领域测试随scheduler全包192项重新通过。

归一化仅限JSON缩进与对象键序；状态码、错误字符串、路由body和content-type精确比较。
时钟固定10^12ns，随机索引0、shuffle保持输入；适用默认features的共享领域输入。
调度命令使用conf-change-v2；普通角色切换已由既有073回归覆盖，不新增feature。
全局server/api路由装配仍属主题7。

## 实现、验证与故障

生产入口为[serve](../../../pkg/server/schedule_admin_balance_witness.rs)，签名接受
真实BalanceWitnessScheduler、StorageEndpoint、ClusterPaths和HTTP Request，
独占借用覆盖更新与I/O；[说明](../../../pkg/server/schedule_admin_balance_witness.rs.md)。
canonical server显式注册；直接使用已集中声明的serde用于保留输入字段顺序和重复字段。
Cargo.lock只增加server对既有serde的依赖边，没有新增crate或升级远程依赖。
leader适配仅将三个既有纯解码函数提升为crate可见，行为未改。

必要前置补齐位于[BalanceWitnessConfig](../../../pkg/schedule/schedulers/balance_witness.rs)：
保留可空ranges/带符号batch，新增from_transport_parts和精确读取接口，
让Go部分解码错误状态落在同一canonical配置，不复制调度状态。
正常new仍验证1–10并保持空输入全域规范化；传输构造显式不验证，
领域update增加batch校验，保存失败仍保留旧值，延续074契约。
reload仍验证batch，保留已保存的null/[]；空ranges继续按真实core采样为全域。
本任务没有改变调度算法或将HTTP保存失败行为套用到074领域API。

[测试](../../../pkg/server/schedule_admin_balance_witness_test.rs)有5个函数、61个有效案例组：

- task_139_behavior_cases：42项HTTP，方法/路径/query、相同/未知/大小写字段、
  batch最小/最大/越界/错误类型、空/非法JSON、非法ranges/hex、重复字段、null clone、
  部分解码后的list和数组根值。检查状态码、body、content-type及活动batch。
- task_139_updated_scheduling_cases：6组HTTP更新→内存调度和重载后调度，实际执行
  12次；覆盖batch1/10、多范围、包含/排除、开放边界、空列表全域。核对region1、
  WITNESS|REGION kind和真实步骤的见证目标store1/非见证来源store4，并核对持久化。
  使用真实BasicCluster、RuleManager、OpInfluence及canonical操作工厂，未只断言调用次数。
- task_139_persistence_failure：4组故障/恢复。MemoryStore外包MetaStore I/O故障，
  保存失败返回200并发布batch1；相同配置不写；list可见新值；恢复后存储仍旧、
  reload恢复batch4；恢复保存batch10并再次reload通过。
- task_139_body_read_failure：2组真实流错误，完整JSON后错误仍成功，截断JSON后错误500。
- task_139_partial_decode_and_snapshot：7组，类型错误保留负batch、领域API拒绝非法
  snapshot且存储不变、修复batch、null持久化/list克隆、null重载、hex错误保留已赋值batch
  和slice元素、syntax错误保持之前中间状态。存储内容及内存状态均核对。

范围限制：本证据覆盖冻结请求和有效调度输入，不声称任意JSON解析错误的全部
Go诊断文本、任意Gorilla路径清理/跳转或非法中间配置下的Go进程panic均已验证。
HTTP适配没有引入宿主runtime、后台任务或全局信号。

## 红绿及完整验证

[red](logs/task_139/red.json)：fmt退出0；同一已注册测试执行3项，均因错误404与
冻结200断言失败，exit101，证明是行为红灯。随后
[green](logs/task_139/green.json)3项通过；新增真实调度测试时的
[expanded](logs/task_139/expanded.json)保留include_str相对路径编译错误（不是行为红灯）；
修正测试路径后[corrected](logs/task_139/corrected.json)4项和既有task_07的31项通过。
最后新增partial/snapshot边界，完整重新验证。

[final](logs/task_139/final.json)记录每条确切命令、退出码、完整合并stdout/stderr的gzip
日志、SHA256及测试摘要；[audit](logs/task_139/audit.json)核实来源/fixture/日志及只读文件。

| 命令 | 退出码、有效测试 |
| --- | --- |
| cargo fmt --all | 0 |
| cargo test -p astersql-pd-server --locked --lib task_139 -- --nocapture | 0；5通过、61有效案例组 |
| cargo test -p astersql-pd-server --locked | 0；63通过 |
| cargo test -p astersql-pd-schedule-schedulers --locked | 0；192通过 |
| cargo metadata --format-version 1 --no-deps | 0 |
| cargo fmt --all -- --check | 0 |
| cargo check --workspace --locked | 0 |
| cargo test --workspace --locked | 0；1863通过、0失败、1既有忽略 |

[verify.py](logs/task_139/verify.py)每个阶段使用原子mkdir动态领取槽位1，
CARGO_TARGET_DIR=`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner包含计划、任务、会话和PID；正常/失败/中断均先回收子进程组，
再只删除自己的owner和锁，保留编译缓存。全部Cargo/脚本继承绝对路径。

[Go拦截器](logs/task_139/reuse-go/go)仅复用已保存的etcd I/O fixture二进制，
SHA256 `57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5`，
每次实际复制记录于[go-reuse](logs/task_139/go-reuse.jsonl)，拒绝其他全部Go命令。
没有运行Go构建/测试、make、failpoint、生成或模块整理。
审计比较全部1101份Go/模块文件集合、哈希和Git状态；检查两级只读plan、
远程依赖版本、Markdown链接、git diff --check和自身锁释放。
既有其他编号任务删除保留并排除提交；本任务编号文件完成后删除。
