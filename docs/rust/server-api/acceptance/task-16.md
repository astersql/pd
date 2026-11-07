# 验收16：额外gRPC注册与责任交接审计

2026-11-06；基准HEAD `128c50050748fac7820b48039759d77a522929ec`。
后续实施说明：用户已解除此前纯文档/Cargo/服务实现的限制，允许按原拥有任务
继续Rust实施及现成Go oracle验证。当前正在实施，尚未获得整项协议验收证据；
下文是提交9ceedb1be时的只读审计和当时授权边界，不再作为停止实施的理由。
方法责任和注册条件见[extra-services.md](extra-services.md)。

## 本次实际交付

修补原控制面100/101/138/148编号任务的实施入口、唯一拥有者、先失败后通过的
行为断言和交付格式；在控制面coverage增量补记classic RM proxy来源。
42个方法逐ID分配到唯一方法实现任务，装配/差分单列；不复制领域算法。
四个原任务仍为未开始，未修改任何plan.md，也未用交接文档冒充实现回执。
这是对原阻塞记录“只建议交接”的实际修复，用户最新指示明确授权补实施计划。

## 只读证据及其范围

- 解析验收09/13/14/15的压缩回执及当前源码摘要：37/41/19/21项，
  起始变化19/5/4/0项；本轮coverage文档补充后13为6项变化。
  旧回执范围仍有效用于历史归因，不能声称本轮重新运行。
- 验收15固定Go oracle、etcd/etcdctl、protoc和kvproto ZIP共5项摘要重算匹配；
  固定协议42方法与wire清单一致，42个稳定ID与原行匹配。
- Go classic注册PD、Keyspace、Diagnostics、MetaStorage及条件RM；Rust canonical
  classic入口只有PD、Keyspace、Diagnostics。TSO已有独立服务入口和三方法；
  MetaStorage/Router/Scheduling canonical Rust服务尚不存在，RM只有配置crate。
- 原控制面138/148的持久回执仍不存在；未启动任何构建、Go/Rust服务或oracle。
  本轮行为测试、RPC差分均为0，不能用文档检查成功替代这些缺口。

本轮重跑编号16内保存的Python只读审计（按其首个python代码围栏执行），exit0：
`coverage: 42 unique IDs and source rows match`；
`admitted input/binary fingerprints: 5 match`；
`pinned methods: 42 missing inventory: [] extra inventory: []`；
`required receipt .../task-138.md exists False`；
`required receipt .../task-148.md exists False`。
该编号任务保留至真正验收，不作为已删除的临时证据引用。

## 替代方案及授权边界

1. 复用旧回执：已解压并重算来源/输入摘要；只能接收其明确子集，无法生成不存在
   服务的当前运行证据，也不能覆盖完整fallback/TLS/流故障矩阵。
2. 复用现有canonical服务并最小接线：已核对TSO及Keyspace可复用；
   MetaStorage、Router、Scheduling和RM领域/服务不是单个注册遗漏，
   无可供完整装配的公共服务。新增tonic默认桩或HTTP 503不能替代。
3. 仅通过源码、descriptor和文档闭环：已完成42方法、注册条件及唯一责任校验，
   并实际补齐原实施计划。此路径不提供真实RPC、取消/EOF及持久状态，
   因此只能消除责任缺口，不能达到本项行为完成标准。
4. 在本轮执行原领域任务：原提示已授权推进依赖，并无账号/工具权限障碍；
   但本轮同时明确限定纯文档、不运行Cargo/Go、不实现整组MCS。
   真正推进缺失领域必须写Rust并执行Cargo/TDD和现成Go oracle差分，
   超出这三项明确边界。缺失实现与工作量本身不是索取权限或停止文档交接的理由。

**剩余工作无新的产品设计、后端选型或外部发布决策。** 可由原拥有任务按既有授权、
原顺序执行，回执生成后本项继续纯只读接收，无需放宽本项范围。
若要求在本轮内一直推进到整项验收，所需最小范围调整是：允许切换到原控制面
65–68、78/79、86/87、100/101、119–122及其必要既有依赖/装配/148任务的
Rust实施与Cargo/现成oracle运行；仍保持Go源码/模块只读、动态槽位、禁止发布，
不是允许在验收16重建整组MCS。可以按原任务逐项执行，无需一次授权新系统设计。
当前明确文档限制未解除，故未执行这些动作；不虚构外部环境故障或请求新的系统权限。

## 文档验证（本轮执行）

从仓库根目录用Python执行下列代码，检查方法/唯一责任、四个实施任务状态和
新文档引用；不运行计划中列出的未来Cargo命令。

```python
from pathlib import Path
import re,json,hashlib
root=Path.cwd()
d=Path('docs/rust/server-api/acceptance')
rows=[r for r in json.loads((d/'coverage.json').read_text())['rows'] if '额外服务' in r['section']]
text=(d/'extra-services.md').read_text()
table={}
for rid,method,owner in re.findall(r'^\| `(wire:[a-f0-9]+)` \| `([^\`]+)` \| ([^|]+) \|$',text,re.M):
 assert rid not in table,rid
 table[rid]=(method,owner.strip())
assert set(table)=={r['id'] for r in rows}
for r in rows:assert table[r['id']][0]==r['cells'][0]
plans=Path('.plans/2026-10-29-pd-rust-控制面与MCS')
paths=[]
for n in [100,101,138,148]:
 p=next(plans.rglob(str(n)+'-*.md'));s=p.read_text()
 assert '状态：未开始' in s and '## 验收16补充' in s
 assert '## Cargo 共享槽位规则' in s
 assert '## 测试计划' in s and '失败' in s and '通过' in s
 paths.append(p)
for r in rows:
 owner=table[r['id']][1]
 if r['cells'][0].startswith('resource_manager.ResourceManager/'):
  assert owner==('控制面101' if r['cells'][0].endswith('/AcquireTokenBuckets') else '控制面100')
for p in [d/'extra-services.md',d/'task-16.md',Path('docs/rust/control-plane-mcs/coverage.md'),*paths]:
 s=p.read_text()
 assert s.count(chr(96)*3)%2==0,p
 assert not any(line.rstrip()!=line for line in s.splitlines()),p
 for link in re.findall(r'\]\(([^)]+)\)',s):
  if '://' in link or link.startswith('#'):continue
  assert (p.parent/link.split('#')[0]).resolve().is_file(),(str(p),link)
print('42 unique method/ID/owner rows: PASS; RM 5+1 ownership: PASS')
print('4 implementation plans pending with TDD/Cargo contracts: PASS')
print('7 document links/fences/whitespace checks: PASS')
print('No Cargo/Go/RPC executed; behavioral pass/fail/ignored counts: 0/0/0')
```

实际结果exit0：42方法/ID/唯一责任、RM五加一责任、四份未开始实施计划及
七份文档引用/围栏/行尾空白均通过。首次检查把脚本自身的三反引号字面量也算作
围栏而exit1；改用chr(96)*3避免自计数后通过，未改方法集合或弱化验收标准。
`git diff --check` exit0。本轮新增行为通过/失败/忽略数均为0。

Go只读守卫使用git ls-files的cached/others与ignored并集，比较1101项Go/模块
文件存在性及SHA-256；另比较41项plan.md。首尾1142项完全一致，exit0，
未生成临时Go文件，未执行Cargo、Go、make、failpoint或构建。
仅提交四份原任务实施计划、控制面coverage及本项两份交接/审计文档；
编号16保留已阻塞，原有无关计划删除/未跟踪计划保持。

## MetaStorage前置65–68实施与验收

2026-11-06，代码基准`9ceedb1beb0e1f3be359f7a7d3f9eb5186ce3850`。
用户已解除纯文档限制，本阶段实际实现并验证原拥有任务65–68。
第16项仍在实施，RM/Router/Scheduling及148整体验收继续，未标为完成或阻塞。
[原始压缩回执](task-16-metastorage.json.gz)保存47个阶段命令及全部日志，
包含真实red和最终green、RPC、独立物理状态、源码/二进制/准入输入摘要、
Go/plan首尾守卫与运行器；后续实现不得覆盖本阶段原始失败。

实现详情见[65](../../control-plane-mcs/task-065.md)、
[66](../../control-plane-mcs/task-066.md)、[67](../../control-plane-mcs/task-067.md)、
[68](../../control-plane-mcs/task-068.md)。服务实现只在canonical MetaStorage crate；
server窄桥接注入原Host资源，领域存储接口保持后端无关且不含protobuf。
MetaStore新增一致count、范围watch及真实KV version；响应header revision与重放游标分列，
避免历史回放把header错误改为最后事件revision。8个流请求保存12条回执/侧，
另有14个读取和11个写删配对；数字不是把回执行数当成有效测试数。

| 本轮最终命令 | 退出码与有效结果 |
| --- | --- |
| `cargo test --locked -p astersql-pd-mcs-metastorage-server` | exit0，4通过/0失败/0忽略；doc-test 0不计行为通过 |
| `cargo test --locked -p astersql-pd-storage-kv --lib -- --nocapture` | exit0，36通过/0失败/0忽略 |
| `cargo test --locked -p astersql-pd-election --lib -- --nocapture` | exit0，19通过/0失败/0忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib -- --test-threads=4` | 修复夹具后的最终exit0，106通过/0失败/38历史忽略 |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_016_metastorage_ -- --nocapture --test-threads=1` | exit0，3通过/0失败/0忽略，包含真实双侧WAL重启 |
| `cargo test --locked -p astersql-pd-runtime --lib mcs_068_ -- --nocapture` | exit0，1通过/0失败/0忽略；已包含在runtime包106项中，不重复累计 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | exit0，当前源码实际二进制 |
| `cargo metadata --format-version 1 --no-deps` | exit0 |
| `cargo fmt --all -- --check` | exit0 |
| `cargo check --workspace --locked` | exit0 |

最终实际槽位为1，绝对`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`；
最初一次内存单元阶段动态领取槽位2，未使用其二进制执行wire。阶段结束释放自己的锁，
缓存保留；现成Go、官方etcd/etcdctl、protoc及固定kvproto ZIP五项摘要匹配。
Go只运行现成服务与只读`go version -m`，没有Go编译/测试或生成/整理；
1101项Go/模块和41项plan.md首尾一致。完整workspace test集中于24，不以编译替代它。

### 额外完整server包未通过的交接

`cargo test --locked -p astersql-pd-server --lib`真实执行exit101：
283通过/8失败/39历史忽略。原日志完整保留，**未把该包认证为通过**。
本阶段MetaStorage四方法与宿主的专项/真实差分已通过，不以以下旧断言替代其证据，
也不为凑通过而删除或放宽它们；后续22/23/24必须按原行为核查并修复。

| 原测试 | 当前失败症状 | 唯一后续验收拥有者 |
| --- | --- | --- |
| `grpc_alloc_id_test::task_021_behavior` | 重启near-u64边界后持久ID值998与旧MAX断言不等；需对照既有Go Rebase wrap证据 | 22 |
| `http_config_read_fault_test::task_047_follower_recovery_fault_and_cancel` | 等待SchedulePhase::Running超时 | 23 |
| `http_schedule_config_write_fault_test::task_048_fault_cancel_and_concurrent` | 等待SchedulePhase::Running超时 | 23 |
| `http_replication_config_write_fault_test::task_049_fault_cancel_and_concurrent` | 等待SchedulePhase::Running超时 | 23 |
| `http_hotspot_history_test::task_061_persistence_filters_pages_and_errors` | 无效加密key先被校验，错误不同于旧缺manager断言 | 23 |
| `http_hotspot_history_test::task_061_sampler_and_restart` | 采样仍运行时取的快照与关闭/重启后的历史行集合不等 | 23 |
| `http_maintenance_test::task_102_maintenance_etcd_wal_restart` | 释放reservation再重绑地址遇AddrInUse | 23 |
| `http_region_label_test::task_070_storage_failure_and_cancel` | 默认空间标签存在，旧saved.is_empty断言失败 | 23 |

24消费22/23的修复结果再完整工作区回归。这些当前症状尚未全部归因完成，
不宣称已在原基准重跑证明，更不以“待回归”绕过必需领域或oracle证据。
与本阶段直接相邻的AskBatchSplit allocator故障夹具已实际修复：诊断回执显示
before=1、持久窗口1000、fault仍armed但返回1129起的IDs，后台预取干扰了窗口边界。
在该夹具禁用自动scheduler并延迟checker巡检后保留原故障/取消断言，专项及同4线程
完整runtime包通过；修复前0/1专项、105/1/38全包失败均保存，不覆盖或隐藏。

## Classic RM RU前置阶段

2026-11-06，代码基准`461359230`。本阶段在原canonical
`pkg/mcs/resourcemanager/server`中补RU资源组、客户端槽位、借贷、写角色、
JSON状态及protobuf适配，接通classic本地六个RPC。领域值不含protobuf；
`grpc_codec`和`grpc_storage`为原schema的边界转换，没有在server/router复制内核。

`pkg/runtime/control_resource.rs`在leader任期ready前加载，同一MetaStore由现有
TermStore约束写入；宿主拥有一分钟持久化任务，并在退休时取消、等待它。
`PdServices::resource_groups`只返回当前有效任期的canonical manager；
服务层不保存第二份缓存，也不创建runtime、信号或转发worker。
令牌流按读取需求推动，首请求处理后提交headers，逐请求检查serving并保留半关闭。

真实red包括：初始List/令牌流Unimplemented；重启RU统计覆盖而未累加；
非有限余额错误丢PD JSON错误码；controller未继承宿主配置及部分覆盖丢默认值；
状态标量null和无效RFC3339时间戳处理不同。原始输出保留，未删除或放宽这些断言。
controller使用宿主原始调整值，不把不包含该启动设置的动态配置恢复对象当作来源。
重启时统计值与Go一样累加设置和状态中的消耗；无效时间戳整条状态不应用。

本阶段5个真实Go/Rust进程测试均通过（0失败/0忽略），分别覆盖：

- 五个元数据方法、排序/默认组/空keyspace wrapper、priority与UTF-8字节边界，
  失败Modify的部分内存变更、Add幂等覆盖、物理protobuf/JSON删除及双方WAL恢复。
- 无限/有限/速率控制/调节模式、两个client share、零申请清理、缺组/缺设置、
  非有限申请抑制及恢复、未知RU枚举后继行为、后台消耗、非法后台/TiFlash组合、半关闭。
- NaN/+Inf/-Inf余额：设置已实际保存，但JSON状态未保存、缓存未发布；
  RPC错误完整保留Go的PD错误码与具体非有限值。
- 自定义controller、部分JSON覆盖、单字段类型错误、null标量/值结构和根null的5组启动/恢复。
- null状态字段、非法时间戳、无RU字段、根null及有效状态的5组重启输入。

空闲stream deadline的原始client错误分别保留为HTTP/2 reset与本地Timeout expired。
Tonic本地`status.rs`与`transport/service/grpc_timeout.rs`确认它们来自竞争的客户端
HTTP/2取消和deadline路径，均为Cancelled；只对该客户端生成文本分列原样回执，
仍严格断言Cancelled、至少250ms且小于2s的300ms请求预算，不归一化应用错误或余额。

真实宿主专项已通过：两个调用方runtime中的listener共享同一实际etcd；
ready前即存在default状态；follower拒绝；实际等待一分钟定时写入余额；
关闭旧leader结束流，继任者恢复190000余额；父token和原runtime仍可用。
此专项不冒充Go双成员差分，也不重复累计到完整runtime结果外。

阶段包级/工作区回归结果和原始压缩回执在本段末尾登记。
本阶段不销项90–101、138或148：服务总额度与priority协调、元数据watcher、
RU策略实际应用、独立RM、fallback=false路由/连接池/代理、双模式TLS等仍继续。
当前本地适配器无条件注册，不能将fallback=true回执用作条件注册证明。
Go本身未实现Raw模式且构造会panic；Rust库对非RU新组返回`no implementation`，
不将其称为已实现Raw模式或与Go进程崩溃完全等价；本阶段wire准入为RU组。
完整server包此前8项未通过仍由上段22/23/24交接持有，未改写为本阶段通过。

完整runtime首次回归实际为106通过/1失败/38历史忽略：
`cluster_restore_test::task_006_corrupt_data_releases_resources`在损坏数据启动失败后
看到后端Arc计数2而非1。随后单项诊断曾通过，未把它冒充稳定回归通过。
检查发现初始化错误路径只依赖KeyWatch的Drop/abort，未join取消任务；
在初始化返回错误前显式取消并等待加密watch，保留原错误及清理失败信息，
原计数、registry和listener释放断言不变，仅补诊断键名。专项随后通过；
完整包修复后结果另列于最终阶段表，不覆盖106/1/38原日志。

第二轮默认线程完整包仍如实记录106/1/38：清理用例已通过，另一条
`join_preflight_test::task_088_behavior`的端口重绑窗口遭AddrInUse。
夹具改为连接态source-port reservation，保持端口不被其它:0夹具复用；
预先严格断言原listener活着时重绑必须失败，启动失败后同地址重绑必须成功。
未用SO_REUSEPORT、重试掩盖、放宽断言或跳过测试。4项preflight专项通过。
完整包按上一阶段同一4线程准入重跑，**107通过/0失败/38既有忽略**；
不是把默认线程两轮失败改写为通过，也未将既有忽略算成验收。

[本阶段不可覆盖原始回执](task-16-rm.json.gz)保存所有`rm-*`命令/完整日志和5类
RPC JSON、保护守卫、输入与当前源码/二进制摘要及运行器。最终结果：

| 最终检查 | 退出码与有效结果 |
| --- | --- |
| `cargo test --locked -p astersql-pd-rm-server` | exit0，7通过/0失败/0忽略；doc-test 0不计行为 |
| `cargo test --locked -p astersql-pd-runtime --lib -- --test-threads=4` | exit0，107通过/0失败/38既有忽略 |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_016_rm_ -- --nocapture --test-threads=1` | 当前二进制重建后exit0，5通过/0失败/0忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib join_preflight_test:: -- --nocapture` | exit0，4通过；已包含在107项中 |
| `cargo test --locked -p astersql-pd-runtime --lib mcs_138_resource_groups_are_term_owned_and_periodically_durable -- --nocapture` | exit0，1通过；已包含在107项中 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | exit0，运行该当前源码二进制做5类RPC差分 |
| `cargo fmt --all -- --check` | exit0 |
| `cargo check --workspace --locked` | exit0 |
| `cargo metadata --format-version 1 --no-deps` | exit0 |

本阶段命令动态领取槽位1，绝对
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
所有阶段子进程结束后释放自己的锁，保留缓存；未改其它槽位。
五项现成输入摘要重新匹配，Go/模块1101项及plan.md 41项仍与总守卫一致。
没有Go构建、Go测试、生成器、make或failpoint写入；Go只运行已有PD二进制及只读build-info。
工作区完整test仍由后续24集中执行，当前完整server包的8项未通过没有被覆盖。

## RM配额、协调与独立宿主阶段

2026-11-07，基准提交`02a8c198e`。本阶段继续验收16，尚不销项原拥有任务。
`rm_service_limit`在同一keyspace的多个组之间共享可补充额度，最多积累5秒额度；
`rm_conciliate`使用真实客户端需求EMA，按priority和同级比例覆盖运行速率，
不改写持久配置。宿主驱动1秒协调和60秒过期tracker清理；覆盖组桶时保留
属于keyspace manager的需求tracker，删除组时清除它。

真实Go/Rust失败后通过的场景：额度10的两个无限组在补充后共享50 token，
其他keyspace保持独立；priority变化使两组运行burst由100/50反转为50/100，
配置仍为100。原始额度与协调请求、响应及观察时间均保留，不归一化余额。

独立`services resource-manager`使用原canonical RM crate和新增的canonical
`pkg/mcs/server`公共listener；后者接受宿主runtime handle、listener和取消token，
只在完整路由交给worker后执行start callback。公共listener关闭时取消已接受IO，
有界等待worker；测试验证保留连接关闭、端口释放、callback失败回收和父runtime继续工作。
`pkg/utils/memberutil`集中Go SHA256前8字节小端participant ID。
原runtime的TermStore原样移至`pkg/election/leadership_store.rs`，两处宿主复用同一
任期栅栏；没有第二套选主或持久化内核。独立宿主复用ServiceRegister、Leadership、
LeaseKeep、LeaderWatch及expected-primary工具，按原`resource_manager`路径注册和选主。

独立服务初始快照版先通过注册ID、读取和写入拒绝，真实失败在实时priority未更新；
随后从单个线性一致快照的revision+1订阅元数据。设置事件只改配置，不重复增加
metadata中的token delta；状态事件保留运行slot/override/tracker，删除设置清除组。
服务继续使用同一manager；未在transport中建立平行缓存。独立RM拒绝三个写RPC，
错误为FailedPrecondition和`resource group metadata writes must be handled by PD`。
六方法真实进程测试已通过，包含token分配、设置/删除及关闭后保留流退出。
同一现成Go PD作为双方元数据上游；此场景不冒充Rust classic代理验收。

本阶段仍缺watch断连/compaction故障差分、完整RU策略运行应用、classic
fallback=false的metadata-only角色与代理池、TLS及双部署148矩阵。当前经典RM
适配仍无条件本地注册，不能以独立服务通过代替条件装配或代理证据。
`rms-watcher-red`记录的是一次Cargo包名输入错误；真正行为red为
`rms-watcher-behavior-red`的实时priority断言，不将编译或命令错误计作TDD失败。
当前结果如下；先前两个阶段回执不覆盖。

| 验证 | 结果 |
|---|---|
| RM包级 | 14通过、0失败、0忽略 |
| 共享listener / member ID / election | 2 / 1 / 19通过，均0失败、0忽略 |
| 最终Go/Rust RM线级回归 | 8通过、0失败、0忽略，逐测试真实运行双方进程 |
| 最新完整runtime（4线程） | 107通过、0失败、38历史忽略 |
| workspace check、fmt check、metadata、CLI build | exit0，使用共享槽位1 |
| Go/module/plan.md保护检查 | 1142项无差异 |

相邻首次完整runtime仍有106通过/1失败/38忽略，失败是join preflight在返回
错误后立即重绑定端口得到AddrInUse。没有删除释放断言或增加重试；仅在失败时
补实际错误与端口owner诊断。专项4项与后续完整107项通过，不能据此认定偶发问题
已修复。三个操作系统socket探针未发现端口碰撞；其中3.57秒wildcard探针与第二次
runtime诊断运行重叠，原始记录如实保留，不用该次通过证明首个失败的根因。
该不确定项继续交验收22/23/24集中回归处理，不阻止继续实施代理。

不可变[本阶段压缩回执](task-16-rm-quota-watch.json.gz)保留所有阶段命令、真实red、
原始RPC请求/响应、失败日志、工具与源码摘要以及上述未覆盖边界。

## Classic RM代理与连接池阶段

2026-11-07，基准`ec8873856`。本阶段继续验收16，不销项100/101/138/148。
CLI把调整后的fallback开关显式传给宿主；库builder默认仍启用嵌入RM。
同一listener只注册一个ResourceManager适配器：true复用canonical真实Service，
false使用PD metadata-only任期manager处理写入，读取与token流转发到RM primary。
Nil group/空名称先返回InvalidArgument；forwarded-host校验当前PD leader，远程写
清空该metadata后转发；本地写仍由原manager与LeadershipStore完成。

`pkg/mcs/server/delegate_connections.rs`集中按目标复用连接和宿主TLS策略，使用
给定executor、取消token及可取消IO。返回的DelegateChannel只弱引用宿主持有的
连接；克隆client不能在pool关闭后保留连接资源。失效、重复关闭、进行中的Watch、
两个宿主/两个目标隔离以及未完成TLS握手取消均有真实连接测试。
Go按配置凭据选择传输而不依URI scheme自动启用TLS；真实Go/Rust测试曾在
https标记的明文RM地址上失败，修正后http/https两种地址均通过。

四个真实行为red分别是Rust无primary时错误返回本地default组、同目标建立两条
TCP连接、URI scheme误选TLS，以及上游退出时将连接IO错误映射为Unknown。
最后一项通过有本地错误来源的h2/IO类型识别改为Unavailable；业务Unknown的
message/details/metadata保持不变。h2使用既有锁文件中的0.4.20，只增加直接类型
依赖与workspace声明，未引入另一HTTP/2实现或字符串匹配错误分类。

当前已取得的线级证据包括：

- API模式、fallback=false，Go/Rust PD分别连接同一现成Go RM；无primary错误、
  五个管理方法、写入参数校验、伪造forwarded-host拒绝、元数据写入无token状态、
  运行设置/删除监听，以及http/https标记地址。对双方实际存储与响应取证。
- PD/RM的Go/Go、Rust/Go、Go/Rust、Rust/Rust四组合；逐字节比较三批protobuf响应，
  包括未知RU枚举、缺失组、空批次、客户端半关闭后响应顺序及EOF，不重放请求。
- 第一条RPC独占前端连接关闭后，另一条流仍实际申请到9 token；双方行为一致。
  最初“另一条流必须结束”的源码推断被Go运行证据否定，两个失败夹具原始日志保留，
  未据此修改Rust生产行为，也不将其冒充Rust行为red。
- 上游RM退出并有下一帧输入时，双方均以Unavailable结束旧流；随后发现新primary，
  新流实际取得7 token。原始Go与Tonic连接错误文本分别保留；只校正已证明的错误码，
  不把框架生成文本称为相同，不改写业务错误。
- TLS专项使用canonical service_auth listener验证信任CA、入站客户端CN拒绝、
  不受信CA/身份拒绝、按配置加密http标记地址，以及字节凭据优先于路径。
  这不替代完整Go/Rust TLS双部署矩阵，也不证明客户端对服务端CN策略已实现。

`rmp-proxy-red`首次使用非API夹具，Go本身不启动primary watcher；修正为双方
`PD_SERVICE_MODE=api`后才取得`rmp-api-proxy-red`真实Rust失败。编译/类型错误、
取消夹具的错误推断均分开保存，不算领域TDD失败。

仍需：primary缓存/断连观察语义（当前代理按请求读取发现数据）、PD follower写转发
故障矩阵、出站服务端CN约束及完整TLS/证书重载、watch compaction、Router/Scheduling
注册和148整体验收。接下来继续这些实现；不会用本阶段通过批量关闭拥有任务。

本阶段最终校验：12项真实Go/Rust RM测试通过（0失败/0忽略）；共享宿主包6项通过；
TLS与错误分类两个专项各1项通过；完整runtime在4线程下107通过/38历史忽略。
workspace check、fmt check、metadata与CLI build均exit0，槽位1；1142项保护无差异。
[不可变原始回执](task-16-rm-proxy.json.gz)保留成功/失败、错误夹具、原始RPC与摘要。
完整workspace test仍由验收24集中执行，旧server包8项失败未改写为本阶段通过。

## 出站TLS与primary观察阶段（进行中）

2026-11-07，基准`f0b658330`。出站HTTP与gRPC共用
`pkg/server/client_auth.rs`，在正常证书链、主机名及签名校验之后应用文件配置的
服务端CN列表（包括单个CN）；字节凭据优先于路径且遵循Go字节配置无CN回调的
分支。PD入站单CN例外保持原有行为。客户端关闭会话恢复，确保重连重新读取文件
身份，不会通过旧TLS会话绕过已轮换客户端证书的校验。

真实网络验证先后发现服务端单CN限制缺失、轮换客户端身份后仍恢复旧会话两个
行为失败；修正后通过。现成Go指标代理与Rust共享出站策略的七种场景
（empty/http/denied/https/bad-ca/bad-host/bad-cn）通过；既有四种PD入站鉴权
对照也通过。这些结果不替代完整RM TLS四部署矩阵。

`pkg/mcs/server/primary_observer.rs`新增编码无关的宿主观察器，由调用方提供
canonical Participant解码，复用LeaderWatch。第一次线性一致快照后从revision+1
监听，只响应精确key。成功delete或空快照清除缓存，暂时元存储错误保留已观察
地址；compaction后重新取快照并对齐删除。取消与drop终止宿主任务。PD RM代理
改为读取该缓存，保留既有25次缺失等待，不再每次RPC查询元存储。依据Go
`server/server.go::initServicePrimaryWatcher/GetServicePrimaryAddr`，缓存本身不
代表目标存活，不在转发前追加健康探测。

三个专项通过：连续更新/相邻key过滤/删除、原生etcd暂停期间缓存可读与恢复后
更新、单线程确定性制造watch cursor落后于compaction并验证缺失对齐及新primary。
共享宿主包共9项通过，0失败/0忽略。上述观察专项是Rust契约验证，尚不冒充Go/Rust
完整故障差分。PD follower写转发、独立服务TLS入口、RM watch策略应用与
Router/Scheduling注册继续实施，100/101/138/148保持未销项。

本阶段RM真实线级回归12通过、0失败、0忽略；CLI build与workspace check使用
共享槽位1。原始输入、输出、失败及证书轮换回执保存在
[本阶段不可变压缩回执](task-16-rm-tls-primary.json.gz)。不重复执行已经通过且本轮
未改动的完整runtime集合；其最近107通过/38历史忽略仍按前阶段证据范围引用。

## Router注册与共享查询转换阶段（进行中）

基准`23517469c`，2026-11-07。初次有效Go fixture已执行七方法及QueryRegion EOF，
Rust现成二进制在`services router`返回unknown option，作为实际注册缺失red。
最初无header夹具在Go错误请求触发transport失败；源码RequestCounter直接访问header，
后续统一传入存在但cluster ID为0的header，不把该Go夹具失败当作Rust red。

新增canonical `astersql-pd-mcs-router-server`，只注册固定七方法，未添加Go源码helper
ScanRegions到descriptor。Service读取宿主同一BasicCluster。Router unary与Go一致
不执行PD leader/bootstrap及cluster ID门禁；QueryRegion校验ID，逐请求读取当前
缓存，保留缺失ID的空RegionResponse；正常半关闭排空后EOF。
原有point/query/scan wire组装及core store/region转换移至grpcutil，原PD模块
re-export或调用共享实现，未复制BasicCluster查询、范围索引或持久化内核。

RouterHost接受宿主executor/listener/MetaStore/cache/delegate pool，启动store快照与
ReconciledWatch，复用primary观察器发现PD leader，并接收SyncRegions。独立CLI
复用RM的既有四标志启动边界；新增`standalone_mcs.rs`取代RM专用入口，不安装
库内runtime或PD election。配置/TLS与完整换主/重连故障矩阵继续由原拥有任务交付。

真实已bootstrap缓存线级red发现Go空占位bucket（version=0）在Rust被投影为存在的
Buckets。Go `core/region_option.go::SetBuckets`及PD/Router同步调用方均要求版本>0；
在共享SyncApplyResponse投影中修正，并新增0/1版本回归。未改变canonical core
SetBucketMeta的其它调用语义。Rust首版red二进制未单独保存摘要；保留失败命令、
原始双方protobuf帧及精确差异，不冒充已保存该二进制。

目前七方法Go/Rust进程测试通过，包含空cache、bootstrap后region/store真实同步、
查询错误cluster、单项/前项/ID/范围/Store/全Store与QueryRegion EOF；逐方法保存
实际protobuf帧，仅对不同进程cluster ID做显式比较归一化。三个包级专项覆盖
共享cache实时替换、范围验证/空leader存在性、嵌入store watch更新/删除及停服后
保留client连接关闭。空起点“必须报空洞”的首次测试预期与Go/core规则不符，改用
非空起点后通过；没有为此修改生产查询逻辑，也不算真实领域red。

未交付完整双部署TLS、RM watcher故障/策略应用、PD follower写转发、Router换主与
Scheduling注册；验收16和所有相关未完成拥有任务继续执行，不能据本阶段批量销项。

共享PD follower同步应用回归11通过、0失败、0忽略，包含原生etcd重叠与持久化故障、
完整同步位置、数组长度、零leader及新增零bucket版本；未重复执行无变更的RM/TLS
完整矩阵。classic读取专项和最终workspace检查按本阶段原始回执记录。

classic region lookup/query/scan专项3通过，共享store/region转换9通过；共用CLI的
Go/Rust独立RM回归1通过。workspace check、fmt check、metadata及CLI build均exit0。
[本阶段压缩回执](task-16-router.json.gz)保留实际槽位、源码/工具摘要、成功与失败、
原始帧和未覆盖边界；全workspace test仍由验收24统一执行，未改变历史失败范围。

## Scheduling注册、心跳角色与PD ID所有权阶段（进行中）

2026-11-07，基准`36321c916`。新增canonical
`pkg/mcs/scheduling/server`，七个固定descriptor方法复用现有PD wire转换、
ClusterHost/coordinator/operator与心跳内核；`SchedulingHost`由调用者提供executor、
MetaStore、listener、delegate pool、registry和PD发现策略，独立入口是
`services scheduling`。不构造PdNode、私有runtime或第二套调度器。
原119–123/139/143/147/148保持未完成；当前不是完整配置、TLS及故障验收。

角色拆分明确PD拥有region元数据、follower同步历史、全局配置和ID持久状态。
Scheduling角色仅向同一BasicCluster提交心跳/统计，沿用canonical规划与命令流；
不启动PD store状态、resolved-ts或成员指标worker，不写回整份PD配置。
Router与Scheduling共享`pkg/mcs/utils/StoreWatcher`，保留本地store统计，强制刷新和
compaction快照协调删除，整批decode失败不发布部分缓存。

实际Go/Rust sidecar对照覆盖七方法注册、真实bootstrap后的store/region心跳、
缺失store/operator、空split/scatter、无region的AskBatchSplit、两类流终止以及
有效AskBatchSplit。双方均连接现成Go PD作为元数据/ID拥有者；保留原始protobuf、
PD region entry与mod_revision以及绝对ID，不用成功的自测冒充双语言差分。
Go活跃RegionHeartbeat收到输入EOF返回`Unknown: EOF`，未激活任期收到首帧则返回
NotBootstrapped再正常EOF；二者不是统一成一种终止行为。nil/空bucket帧可跳过。

记录的真实行为red包括：Rust缺少Scheduling CLI入口；空ScatterRegions错误被当成
单region查询；空闲调度器在无operator时提前消耗128个PD ID。最后一项由同一组
有效心跳后读取PD AllocID的持久副作用发现：Go前后ID为1、2，Rust为1、130。
修复为仅在canonical planner实际要求新ID时调用PD，不预留Scheduling批次；
classic PD原分配策略保留。首次找不到PD客户端与后续成功分配均单独保存。
Go的成员刷新是收到通知或一分钟timer后查询etcd共识leader的client URL，
不是读取PD primary key。CLI保留此默认策略；嵌入宿主显式提供PdLeaderLookup。
独立Rust PD与外置etcd的显式发现配置仍待实现，不能假设etcd地址就是PD地址。

同步规划回调使用调用者提供的阻塞执行器与Handle发起真实AllocID，未建立私有runtime；
单线程Tokio宿主专项通过，PD计数仅增加实际请求的一个ID。分配RPC失败不自动重放，
通知成员刷新并使旧连接失效；旧PD count兼容fallback尚待专项故障对照。
编译错误、Go fixture对首次发现/EOF的错误假设不计入生产行为red。

本阶段压缩回执将固定当前源码、二进制、原始失败与最终通过记录。
后续继续补正bucket/operator命令、split reason/affinity、配置watch/切主与取消、
全部MCS Diagnostics/health注册及TLS部署矩阵；42个wire ID保持原唯一拥有任务。

本阶段最终验证：Scheduling真实Go/Rust两项0失败/0忽略；未激活任期1项、
单线程宿主真实分配1项、共享store watcher 2项、scheduler恢复/PD配置归属6项、
Router 3项、classic AskBatchSplit与Scatter各1项均通过。Runtime全包108通过、
0失败、38项历史条件测试忽略；没有把这些忽略计入通过，也没有扩大为workspace全测试。
原有1142项Go/模块/plan.md守卫一致。原始证据保存于
[Scheduling压缩回执](task-16-scheduling.json.gz)，包含最初red的二进制摘要及Rust patch。

## 公共MCS Diagnostics与health注册阶段（进行中）

2026-11-07，基准`8ac591ae9`。现成Go oracle四种MCS均提供Diagnostics及
`grpc.health.v1.Health/{Check,List,Watch}`；初始Rust四种MCS全部返回Unimplemented。
实际差分记录四种服务各自的空日志配置错误、未知ServerInfo类型、空/未知health服务
Check/Watch及List。第一次fixture错误把prost省略的空map key字段计入编码，修正为
Go响应decode/re-encode后观察到的字节，再记录真正Rust注册失败；不把fixture错误
作为生产red。原始结果包含全部八个sidecar，不在首个错误后停止收集。

Diagnostics日志读取、机器观测及RE2解析移动到canonical公共`pkg/mcs/server`，
classic PD与全部MCS复用；不新增领域副本或反向依赖。调用者提供log path getter、
Tokio Handle及取消scope，文件/机器IO用宿主blocking pool；RE2旧HTTP入口保留窄
re-export。MCS宿主新增可选日志路径入口，旧调用者仍默认空路径；CLI完整配置仍待补齐。
公共health只注册Go同样的空service=SERVING，未知Check返回NotFound/unknown service，
未知Watch首帧SERVICE_UNKNOWN并保持等待。它表示listener可用，不表示primary或bootstrap。
Watch按需求驱动，不产生每连接后台任务，取消/宿主关闭释放保留流。

health协议由固定grpc-go v1.82.1的FileDescriptorProto生成，来源/摘要在
`pkg/proto/grpc-health-provenance.json`，descriptor SHA-256为
`ece2cefbd2f9521efdfadfd89cf3d297c3be45ca4298aa24674b30b397d554d5`。
从现成Go generated rawDesc只读提取，未运行Go或引入新外部依赖；Rust build只消费
已固定的871-byte binary descriptor。绑定包含List，单独导出HEALTH_DESCRIPTOR_SET，
不修改固定kvproto descriptor集合、42 wire ID或方法拥有任务。

公共原有14项专项与新增日志/保留Watch关闭专项通过。四种MCS及classic PD实际差分、
TSO原业务路由/宿主回归和workspace检查继续，整项16未完成。

原TSO宿主业务GetMinTS、注册/启动错误清理、打开的TSO流与新增保留Health Watch
关闭测试通过。classic日志完整Go/Rust差分已通过。首次classic回归漏传旧测试必需的
14/15回执环境变量，修正环境后保留原断言；ServerInfo的native arm64与固定Rosetta
amd64对照触发16KiB/4KiB页大小差异，遵循验收15已有的同架构准入重新构建amd64，
不硬编码页大小或修改测试范围。上述环境/架构失败不归为本阶段生产行为red。

同架构amd64 ServerInfo完整Go/Rust差分通过，包含未知类型、follower本机观测、
命令缺失/恢复/stderr/3秒超时及独立内存区间采样。amd64产物仅复制到所领取槽位内
独立测试文件，运行结束删除；默认native产物未覆盖。公共注册四种Go/Rust MCS
差分1项通过，classic日志差分1项通过，ServerInfo同架构差分1项通过，均无忽略。

原kvproto八项兼容测试及workspace check通过；42项固定方法清单不变。
完整阶段日志、最初注册red、环境/架构错误、两个架构产物摘要与实际机器/日志回执
保存在[公共注册压缩回执](task-16-common-registration.json.gz)。同一次native组合命令
记录为1通过/1失败，后续独立amd64 ServerInfo命令0失败，未将组合命令伪记为全绿。

## HTTP入口与出站TLS配置兼容修正（进行中）

2026-11-07，基准`8aba1bede`。为TLS部署矩阵增加真实Go/Rust启动对照：相同
有效CA/证书/私钥配置、HTTP client URL，检查明文GetMembers和HTTP config保留路径。
Go正常启动并接受明文gRPC，初始Rust启动失败；原始成员protobuf与失败日志已保存。
修正canonical Runtime listener按URL scheme选择入站协议；HTTP配置仍复用既有
client_auth工厂校验证书，证书继续用于出站客户端，不以配置文件路径强制HTTPS。

差分已由失败转通过，原有缺失证书负例也通过：HTTP/HTTPS都拒绝无效凭据，
释放listener且不产生metadata副作用。没有删除、重试或放宽负例断言，也没有建立
第二套TLS/证书解析实现。这是完整TLS矩阵的一项配置前提，不代表全部MCS TLS可用。

本阶段现成Go/Rust差分1通过/0失败/0忽略；无效凭据与listener/metadata清理1通过，
workspace check、格式、metadata及1142项只读守卫通过。原始red/green与产物摘要见
[HTTP/TLS配置压缩回执](task-16-http-tls-config.json.gz)。MCS的TLS listener选择策略
与classic URL策略不同：Go BaseServer在cmux之前按ToServerTLSConfig包装tls.Listener；
后续须以真实MCS部署矩阵验证，不由此推断MCS已支持TLS配置。

## MCS前端TLS与共用配置入口阶段（进行中）

2026-11-07，基准`435fe4961`。四种MCS共用CLI文件/参数适配，支持transport、
security、log及既有ControllerConfig字段；复用ConfigMetaData和controller默认值，
不复制RU算法。非空命令行值覆盖文件，空字符串遵循Go AdjustCommandLineString保留
文件值，`--config=`不打开空路径。原有四参数入口继续调用同一适配器。

原始Go/Rust red：Go RM在配置文件提供证书时正常通过四种客户端认证探测，
Rust不接受`--config`；随后真实合并差分发现Rust错误清空文件证书和读取空配置路径。
三种合并场景（空证书参数、空配置参数、非空参数替换缺失文件证书）已由失败转通过。
另用已核验SHA的历史amd64产物实际运行配置日志red：Go四种服务读取成功，历史Rust
不接受`--log-file`；这是复用现成历史二进制，不是改坏当前实现制造失败。

MCS TLS listener由文件证书/私钥字段选择，独立于HTTP/HTTPS URI，按Go InitListener在路由前
完成认证。复用canonical `service_auth::Listener`、文件证书重载、WebPKI和CN解析；
新增microservice策略，不改变classic的单CN例外。MCS有CA但无CN限制时客户端证书可选；
非空CN列表含单个名称也限制认证。未指定CA时，匿名连接仍可用，提供证书则以系统根验证。
TSO宿主改为接受调用者提供的同一种axum listener，不建立私有runtime或第二套TLS内核。
Router/Scheduling出站连接复用既有client_auth及可刷新delegate工厂；显式凭据不按URI
降级。当前SDK HTTPS backend和TSO HTTPS transition仍待后续接线，未声称全部TLS可用。

四种MCS各三种CN策略、五种客户端身份、两个实现的120次实际认证探测通过；
成功比较health响应，拒绝保留完整错误诊断，不将不同TLS实现的错误文本归一后称逐字相同。
最终同一二进制重跑TLS/CN及参数合并共2测试通过/0失败/0忽略。
RM无CA的独立五身份对照也通过（两个实现共10次）；四种MCS实际配置日志读取对照通过。
日志生产者和Diagnostics共享同一宿主路径，固定轮转输入与运行日志分开，比较实际protobuf。
这些是前端认证/配置子集，不是RM token、Router同步、Scheduling命令或148整体验收。

classic认证四种策略的既有Rust专项通过（1测试，保留16组HTTP读写/gRPC观察）；
本轮未将它冒充新的classic Go差分。配置日志差分在四种MCS上通过，包含实际logger
文件初始化及轮转固定输入。额外发现controller TOML的布尔/整数/小数字面量不能
直接套用JSON的string标签，真实Go正常启动而Rust拒绝；改为复用RM canonical
`decode_toml`和存在性默认值后该启动对照通过，未另写转换/默认值算法。
此启动测试不据此声称RU策略已经在运行时生效；该原有差分缺口继续保留。

最后的凭据预检补齐另一真实差异：只有无效SSLCABytes且无文件identity时，Go拒绝
启动，Rust初始却提供明文服务。四种CLI现在在backend/listener装配前调用同一既有
client_auth工厂校验出站file/byte凭据，修补未配置gRPC delegate的RM/TSO分支；
CA文件单独存在且无identity不据此启用gRPC TLS，保留Go的显式凭据规则。
仍需后续完成HTTPS backend SDK连接及各领域完整部署/故障矩阵，不将本阶段子集销项。

所有生产修正后的同一最终产物上，MCS frontend模块五项Go/Rust测试通过，
0失败/0忽略；默认明文RM六方法/metadata watch的现有实际Go/Rust专项亦通过。
不再重复已通过且未修改的metrics、机器观测或完整runtime回归。

最终TSO泛型listener宿主（GetMinTS、重复注册、打开的TSO/health流关闭）专项通过。
workspace check与1142项只读守卫通过，原来的无关计划变更保持。完整原始失败、
当前产物、历史产物指纹及请求/诊断见[MCS前端TLS压缩回执](task-16-mcs-front-tls.json.gz)。

## MCS HTTPS后端与原生SDK传输阶段（进行中）

真实native etcd 3.5.15双向TLS对照先观察到：现成Go RM正常读取cluster ID并注册，
Rust启动报metadata unavailable、注册键不存在。修正复用现有DelegateConnections、
client_auth和etcd-client 0.17的raw-channel/Custom入口，不复制SDK、协议或TLS连接器。
新OwnedDelegateChannel供SDK强持有宿主连接池；普通代理仍使用原弱持有通道。
仅建立连接失败时尝试下一端点，请求发出后的远端错误不触发重放，避免不确定写入重复。
取消令牌由进程拥有，退出路径在宿主租约清理后才取消传输，启动失败同样释放资源。

MetaStore trait保持后端中立；具体EtcdMetaStore接受调用者的传输工厂，主SDK使用
多端点通道，每端点健康探针使用隔离的单端点SDK客户端。真实TLS存储契约涵盖CAS、
watch重放、压缩及恢复、租约续期/撤销、断开端点与健康端点隔离，以及取消后不可访问。
独立真实HTTP 503故障服务确认已发送请求不会连接第二个健康服务器。

Go源契约明确显式HTTP端点丢弃TLS、HTTPS端点必须TLS；虽然SDK支持无scheme地址，
实际Go MCS在进入SDK前即拒绝该地址，对照已捕获并保留相同的进程校验边界。
仅CA文件无identity时ToClientTLSConfig返回nil，HTTPS使用系统根。CLI共用后端装配
按该规则复用现有凭据工厂，保持TSO/Router/Scheduling/RM同一入口。TLS阶段不能被
解释为完整四MCS领域差分；TSO HTTPS transition、完整配置/故障矩阵及148仍待完成。

最终RM进程三profile×双实现共六组观察通过：显式HTTPS、配置凭据的HTTP均健康注册，
注册项有非零lease且正常退出后删除；无scheme地址均在服务前拒绝、不产生注册。
四MCS前端120次认证探测回归通过。原生TLS存储专项1、连接所有权/取消与不重放专项2
均通过，0失败/0忽略；并非把契约内断言数或网络观察数当测试数。
完整输入、源码与二进制摘要、原始red/green及校验命令见
[MCS原生SDK传输回执](task-16-mcs-backend-tls.json.gz)。该回执仅接收上述范围，
不因此关闭四MCS完整HTTPS部署或148。

## TSO HTTPS交接与公共HTTP驱动阶段（进行中）

真实TLS listener上的TSO completion专项先失败为InvalidArgument：旧函数只接受HTTP。
该red为Rust真实网络测试，Go部分仅只读核对finishSplit/finishMerge及DELETE路由，
不冒充本轮Go自动分组交接差分。后续通过新增调用者提供的TLS策略入口补齐HTTPS；
保留原嵌入API，旧无TLS策略入口遇HTTPS仍拒绝，不降级为明文。

HTTP驱动从原metric_transport提取到canonical MCS公共包，沿用原有连接/响应流驱动、
取消、绝对超时和body资源所有权；metrics继续自己解析/校验并固定DNS目标，TSO
提供控制面目标及宿主策略。没有另建TLS算法、独立runtime或后台HTTP驱动任务。
CLI的gRPC delegate、原生SDK及TSO HTTP现在共用同一client_tls_policy工厂，
文件信任/identity及CN策略仍由原client_auth实现，按新连接读取配置。

TLS专项包含split/merge准确DELETE路径、远端503、无TLS策略、正确CA但无客户端
identity、不可信CA与取消；拒绝请求均未到达应用。此测试仅验证HTTP completion
传输契约，不等同完整分组迁移的时间戳保留或并发故障证据。
因HTTP内核移动，已重跑现有metrics HTTP/HTTPS两种真实上游专项，包括GET/POST、
gzip、转发头、错误/重定向及取消。两次各1通过/0失败/0忽略，都是Rust回归，
没有运行metrics全矩阵、旧Go构建链或把历史Go响应当本轮结果。

四MCS真实HTTPS backend对照已通过：双方都使用实际启动并选出leader的Go PD所有者，
不是native etcd中伪造PD leader。八组观察（四服务×双实现）均完成TLS health、
非零租约注册及优雅退出后的注册键删除；TSO真实读取PD leader后启动。
本项仍未覆盖八组中的所有业务RPC/自动迁移；明确不将readiness冒充领域验收。
TSO crate最终2通过/0失败/0忽略（原宿主生命周期及新TLS completion），doc-tests为0；
workspace check、fmt、metadata及1142项只读守卫通过。
原始输入/失败/成功、当前二进制及源码指纹见
[TSO HTTPS与公共HTTP阶段回执](task-16-tso-https.json.gz)。

## MCS名称与primary租约配置阶段（进行中）

真实Go/Rust进程及固定etcdctl对照捕获：配置lease=9时Go primary租约为9，Rust仍为5；
默认name在Go为角色前缀加主机名（RM前缀保留空格），Rust却使用advertise地址。
先收集未bootstrap场景时，还观察到Rust Scheduling primary被快速撤销，TTL探针
取得已消失租约；该原始失败保留，未bootstrap初始化/primary生命周期继续单列验证。
配置专项改用真实Bootstrap RPC完成的PD API所有者隔离测试前置，仍确实失败于9对5，
不是用bootstrap抹去配置差异或把旧失败当通过。

CLI共用配置入口按角色命名，零lease使用canonical默认值5；RM/Scheduling新增
调用者primary lease入口，旧嵌入API仍采用5。TSO将配置传入已有GroupWatchConfig，
registry lease独立固定5，不跟随primary配置。Router没有primary，不套用其它角色lease。
具体配置中的controller只由RM调用既有canonical TOML解码器，其它角色保留Go的
未知表边界；不把未知表的RM字段类型错误扩大为所有MCS的启动失败。

最终两测试通过/0失败/0忽略：16组lease/name观察（四角色×两配置×双实现），
以及8组controller类型启动观察（四角色×双实现）。primary比较etcdctl granted-ttl，
不比较随时间递减的剩余TTL；registry固定为5，primary默认5/配置9，名称逐角色相等。
一次green因health已可访问但注册事务仍在发布中而过早读取键，原始失败保留；
测试增加有界等待实际注册键，租约、名称和存在性断言不弱化。一次文本编辑脚本
定位失败未修改文件，其后运行通过；修正定位并实际写入同步后，最终模块再次通过。
这些不代表完整配置、TSO时间窗口参数、Scheduling文件配置或全部148故障矩阵完成。

TSO原宿主生命周期专项另1通过/0失败/0忽略，workspace check/fmt/metadata通过；
完整原始输入、失败与最终16/8组观察及只读守卫见
[MCS进程配置压缩回执](task-16-mcs-process-config.json.gz)。
仍需处理未bootstrap Scheduling观察及其它剩余配置/领域证据，不关闭本项。

## Scheduling独立primary与在线bootstrap阶段（进行中）

接收名称/租约专项暴露的未bootstrap观察后，新增真实Go/Rust专项先失败：PD API
所有者尚无cluster meta时，Go的Scheduling primary十次采样均持有同一租约；
Rust九次无primary，一次瞬时出现。Go GetOperator为region not found，Rust为
cluster is not initialized。原始失败先落盘后断言，不用仅源码推导代替差分。

原因是Rust MCS open误用了classic PD的meta存在性门槛；移除此局部门槛，MCS
继续复用canonical ClusterHost。共享调度worker的准备条件按角色处理：Scheduling
自己的缓存可以准备，classic PD仍要求真实bootstrap；没有伪造元数据或重写bootstrap。

增强的同一专项通过：十次采样跨约一秒保持primary完整value/lease/create/mod revision
不变；通过真正PD Bootstrap RPC后同一MCS进程及primary租约不变，store watcher
看到新增store并接受StoreHeartbeat。GetOperator在bootstrap前后以及store heartbeat
比较完整protobuf，只把两次独立PD所有者的cluster ID置零，原始bytes同时保留。
这是有限primary/初始化差分，不替代完整故障、compaction或调度策略watch矩阵。

最终Scheduling模块3通过/0失败/0忽略，包括原七方法/EOF与心跳/PD-owned split ID；
classic网络生命周期1通过，保留未bootstrap不调度、不消耗ID的原断言。
workspace check、fmt、metadata通过。完整失败、最终protobuf/租约采样及源码/二进制
摘要见[Scheduling在线bootstrap回执](task-16-scheduling-bootstrap.json.gz)。

## 未注册服务回退与MCS信号清理阶段（进行中）

固定协议中Deadlock两方法、TraceRecordPubSub一方法、Namespace四方法在Go注册链中
未注册；本阶段用实际typed RPC核验classic及四种MCS，两个实现共70次负向调用。
含Detect单帧半关闭及Subscribe服务端流，均要求Unimplemented；最初差分实际失败于
Go的unknown service文本与Rust空文本。额外实际HTTP探测又捕获Rust MCS未知路径
返回HTTP 200，Go为404；保留两次原始red。

补丁只给tonic路由集安装canonical公共protocol fallback，以tonic Status生成Go同义
未知服务响应，普通HTTP维持404；不安装三个服务的tonic默认桩，不虚构领域实现。
BaseServer或宿主的任意自定义fallback未被覆盖，TSO既有HTTP路由仍独立合并。
最终比较70次RPC完整code/message，不归一文本；10次HTTP仅断言404，原body保留，
不把Go/Rust HTTP错误body相等列为已验证。

已注册Diagnostics/health回归一度捕获sidecar退出为SIGTERM信号15、无正常退出码。
只读核对Go四种包装器在Run前安装信号，而Rust原来在宿主启动后才轮询并安装。
CLI新增提前安装/随后等待的信号入口，通知在初始化期间缓冲；库不安装进程信号。
Root classic保留原shutdown_signal异步包装，MCS在backend/listener之前准备信号。

对已经构建且指纹保存的pre-signal现成二进制追加实际信号red，没有改坏当前源码重建：
Rust RM收到INT后退出1但registry仍有1项。原来非TERM信号通过问号提前返回，跳过了
异步宿主shutdown。现在无论TERM或INT/HUP/QUIT，都先等待宿主清理，再返回信号
退出结果；SDK传输的drop guard仍在lease清理之后生效。

信号矩阵初版只等registry键，曾使Go Router在store watcher尚初始化时收到HUP，
Run以ctx canceled fatal退出并留下键；该实际启动取消记录保留。正常运行关闭的
专项前置改为registry及真实health均已就绪，不再把注册发布当成初始化完成；
退出码、registry为空及端口关闭断言保持。此阶段不声称覆盖初始化中取消的所有行为。
最终两测试通过：70负向RPC加10 HTTP观察，以及32组信号（四角色×四信号×双实现）。
TERM退出0，其它退出1，全部registry清空、端口关闭；不是把观察数当成测试数。
Diagnostics/health原有真实Go/Rust专项另1通过/0失败/0忽略，已注册路由保持可达。
完整跨域故障、启动取消、TLS及148仍单列，不因此关闭整项16或后续22/23。

classic现成Go/Rust进程关闭专项实际显式启用后1通过/0失败/0忽略，验证退出码、
bind失败重试、端口/身份/leadership复用、ID增加及持久写保留。原日志明确仍有Go
TSO/heartbeat终止Unknown与Rust EOF差别，此专项本身不要求该终止文本相等，
不据此关闭完整验收22。workspace check/fmt/metadata通过。
完整输入、red/green、32组信号及pre-signal产物指纹、只读守卫见
[未注册协议与信号阶段回执](task-16-unregistered-signals.json.gz)。

## Scheduling实际命令与operator状态阶段（进行中）

新增真实Go/Rust独立Scheduling差分，使用真实API模式Go PD的Bootstrap、PutStore、
StoreHeartbeat和双向RegionHeartbeat，不伪造PD leader。三个场景分别覆盖默认keyspace
边界split、默认cluster version的单peer变更、5.0.0的ConfChangeV2；共六个进程观察，
三个测试。比较首条命令与GetOperator完整protobuf，只归零独立集群的cluster ID，
原bytes保留；严格核对PD AllocID增量和PD region持久键不变。

实际red首先捕获Rust把领域STARTED(1)直接写成wire TIMEOUT(1)，Go为RUNNING(4)。
复用classic现有映射并提取公共转换后，split命令和运行状态一致。副本规划进一步
捕获RemoteIds在blocking actor的LocalSet内调用block_in_place发生panic；此前不含
LocalSet的专项未覆盖真实上下文。强化后的单线程宿主专项先复现，再让调用者提供
的runtime驱动真实AllocID future，blocking actor只接收结果，不新增runtime或线程。

随后真实wire差分捕获Rust始终发ConfChangeV2，而Go默认版本发单peer变更；Rust在
operator等待入场期间重复规划，还额外消耗14个PD ID。调用既有canonical admission
在checker入队后立即promotion，并复用现有版本比较实现Go的5.0兼容门槛；scatter与
worker命令发送使用同一权威配置。没有弱化ID断言或归一两种peer命令。
5.0场景最初TOML把semver写成字符串被Go拒绝，改用Go接受的版本表，此为fixture
错误，独立保留日志，不作为行为差异。

最终三个命令专项通过，默认及5.0版本分别核对完整单peer/ConfChangeV2命令、运行中
operator和恰好一次peer ID分配；split不消耗peer ID。七方法/心跳/在线bootstrap模块
及blocking LocalSet、classic初始化回归另行检查，具体计数和执行日志见
[Scheduling命令阶段回执](task-16-scheduling-operators.json.gz)。当前仅证明首条命令与
运行状态，不声称operator ACK/完成/取消、完整bucket、reason/affinity、配置watch或
切主故障矩阵已完成；16、148及后续验收继续。

## Scheduling learner ACK与完成阶段（进行中）

在前阶段实际命令修复后，额外两个真实Go/Rust场景依照收到的AddLearnerNode命令
回报learner及conf_ver增加，再接收真实AddNode promotion并回报voter及下一conf_ver，
持续查询直到GetOperator SUCCESS。默认版本及5.0版本分别核对完整promotion和
SUCCESS protobuf、两次ACK输入（仅cluster ID归零），保存原bytes及实际观察到的
命令，不预先伪造成功状态。PD peer ID仍恰好消耗一个，PD region持久键不变。

两专项首次运行均通过，无额外生产补丁或人为red；扩展后的整个命令模块5通过/0失败/
0忽略，fmt通过。原Go/module及plan守卫保持1142项无变化。当前补充的是正常learner
入场到完成的两种格式，取消、超时、换主、重复/乱序ACK及split完成仍未整体验收。
输入、原始日志及源码摘要见[Scheduling ACK阶段回执](task-16-scheduling-ack.json.gz)。
16、148与后续验收保持进行中。

## Scheduling外置后端发现与PD启动配置阶段（进行中）

同一真实replica learner/ACK/SUCCESS场景扩展至Rust PD API模式加独立原生etcd，
与Go PD API模式加embedded etcd的原场景比较。不是让Go Scheduling连接不支持的
外置Rust PD拓扑。初次实际失败于Rust PD忽略已接受文件的动态配置：API返回默认
max-replicas=3而文件为2。CLI现在通过新增显式startup_config能力把已经adjust的
配置传入canonical恢复，持久文档存在时仍由既有reload优先；listener/telemetry配置
能力保持独立，不把传输配置隐式当成领域配置。

曾尝试在此恢复时主动持久化首份配置，但独立真实restart差分否定该假设：Go API
未bootstrap时config key不存在，文件改成4/true后重启应采用新文件。撤销主动写入，
保留该失败日志。专项现验证初次2/false、重启4/true，前后config key都缺失；不把
尚未验证的“文件总被存储覆盖”写为验收结论。既有文档优先沿用canonical reload；独立库专项验证已有replicas=5覆盖启动replicas=2，
并保持存储value/revision不变。

配置正确后，旧默认发现策略在native拓扑真实等待30秒仍没有规划命令，PD ID没有
用于peer。Go默认取etcd leader client URL，而native etcd URL并不是Rust PD URL。
Scheduling CLI增加明确部署选项`--pd-discovery=primary-key`（TOML `pd-discovery`），
通过canonical election key读取实际租约选出的pdpb.Member及其client URL；没有伪造
member/leader或实现etcd协议桥。默认`etcd-leader`继续原SDK调用；新策略读取带取消
和两秒超时，通过原RemoteIds通知/周期刷新与调用者连接池使用，不新增私有runtime。
该选项只供Scheduling，非法策略/其它角色flag会被拒绝。

本机过渡部署示例：先启动带`--etcd-endpoints=http://127.0.0.1:2379`的Rust PD，
再运行`pd-server services scheduling --backend-endpoints=http://127.0.0.1:2379 --listen-addr=http://127.0.0.1:3379 --pd-discovery=primary-key`。
其它name/地址/TLS设置仍用既有公共入口。仅这是外置EtcdMetaStore过渡路径，不能
据此声称无外部etcd启动、原生Rust生产持久化、掉电或嵌入式替代验收通过。

最终native拓扑正常learner命令、promotion、ACK和SUCCESS与Go基线完整protobuf
一致（仅cluster ID归零）；保留实际primary Member bytes，明确核对native etcd
consensus URL不等于PD URL，恰好一次peer ID分配且PD region持久键不变。
完整命令模块、启动文件restart及默认策略/生命周期回归和检查计数见
[外置发现与启动配置回执](task-16-scheduling-discovery.json.gz)。
TLS换主、故障时发现收敛、配置watch及148/16/17–25仍待继续。

## Scheduling在线复制配置观察阶段（进行中）

新增真实Go/Rust专项，在Scheduling启动前通过PD公共replicate HTTP API把副本数
设为1，发送真实region heartbeat保持双向RPC存活，确认700ms内没有命令；再通过
同一PD API改成2，等待实际AddLearner、promotion及ACK后的SUCCESS。初版fixture
把第二次配置更新放在等待response headers之后，Go在首命令前不发送headers，
导致Go也超时。修正为并发轮询整个stream建立/首响应future，不把header发送时序
当成业务消息；保留该fixture失败。

修正fixture后的实际red：Go完成操作并消费一个peer ID，Rust保持旧副本数，30秒
没有命令且未分配peer ID。原因是Rust只在primary初始化时读取PersistedConfig。
当前接入已有canonical ReconciledWatch，按exact config key观察PD事件，观察任务
由调用者runtime及term取消/关闭管理；配置通过现有串行coordinator actor重新加载，
使用既有PersistedConfig decoder及从classic恢复提取的同一provider转换，验证成功
后原子发布一代配置。没有第二套调度算法、私有runtime或观察者写回PD配置。

删除事件维持Go的忽略策略；compaction/reconnect使用既有watch恢复。这里只对
正常在线replication更新进行端到端差分，其它配置/规则/TTL/affinity/scheduler watches
及完整watch故障差分仍待补。新增内部etcdutil依赖复用现有实现，lock仅增加内部
workspace依赖，不引入外部库。最初一次Rust check因新模块注册层级错误失败，
修正到ClusterHost内部；日志保留，不当作行为red。

当前真实专项通过，比较完整命令、运行及SUCCESS protobuf与两次ACK，cluster ID
之外不归一，严格核对一次peer ID和PD region持久键不变；同时在PD第二次更新后
保存config value/revision，MCS完成操作后要求二者不变。最终模块/恢复与watch契约
回归计数及workspace检查见[在线配置观察回执](task-16-scheduling-config-watch.json.gz)。
16与148保持进行中，不由一次live update关闭完整配置或故障矩阵。

## Scheduling原生配置观察故障补充（进行中）

增加原生etcd加实际Rust PD身份的库级consumer专项；真实Scheduling election通过
canonical campaign获得lease，再用生产open及同一配置观察循环。直接metadata故障
输入不冒充PD公共API，也不冒充完整MCS CLI或Go故障差分。记录器/延迟门只观察或
暂缓实际native watch响应，不制造Compacted结果或替换事件内容。

先加载replicas=5并保存旧cursor，再写canonical replicas=2并压缩真实etcd历史，
然后启动consumer；要求实际Compacted错误、旧cursor及重建cursor超过compact
revision，共享配置最终为2，后端完整entry没有观察者写回。删除与损坏JSON也必须
实际送达consumer并保留上一有效配置，后续有效4恢复；坏值被拒绝后继续观察时
取消，须两秒内join、domain关闭并清除Scheduling primary键，坏字节仍原样存在。

首次有限场景通过，但格式化后的重复运行暴露consumer没有继续消费某个坏值事件。
原因是原实现以事件为唤醒信号，再读取latest配置：读取可能越过当前事件，遇到后续
损坏值后一直重试。强化的实际native响应延迟门保留合法replicas=6事件，故障producer
在它交给consumer之前写坏JSON；真实red为仍停留在2，遗漏了合法6。Go LoopWatcher
对每个event.Kv调用putFn，Scheduling putFn直接解码event.Kv.Value，坏事件记录错误
后继续；此顺序语义由只读Go源码核对，未声称执行了完整Go故障矩阵。

修复改为按原事件顺序把完整document送入已有串行actor。PersistedConfig提取唯一
纯document入口，与原reload复用相同默认、校验和迁移逻辑，不再额外读latest；
primary初始配置也与watch snapshot使用同一revision。坏document保留上一代并继续
下一事件，暂时性fencing错误保留队列待重试；没有第二套配置解析/调度算法或写回。
强化后的native专项保留合法6后面的坏值，正确发布6，再完成删除/坏值/有效替换与
取消检查。中间一次测试因移除父模块import后缺少独立StorageEndpoint import未编译，
修正并保留日志，不当作行为red。原数据格式及迁移专项也另行回归。

最终计数、实际revision/cursor/Compacted计数、red/green及源码摘要见
[原生配置观察故障回执](task-16-scheduling-config-faults.json.gz)。
这不是完整换主/网络分区/CLI/TLS故障或其它watch、掉电/原生Rust生产持久化证明，
16与148继续。

## Scheduling完整持久空间与本地策略投影（16-G）

按编号16有限清单继续核验必需持久副作用，增加同一真实replica/ACK/SUCCESS场景
前后完整MetaStore key/value/create/mod revision/version/lease快照（在sidecar退出后
比较，不删除或忽略某类键）。Go不改变该空间；实际Rust red新增一个默认placement
rule和五个scheduler_config记录。此前仅region/config单键不变的证据不足以发现它。

只读Go装配显示MCS使用MemoryKV，而Rust复用domain时误用了共享fenced backend。
现在Scheduling term使用canonical MemoryStore作为领域存储；真实backend继续独立
拥有election/registry及PD配置、store观察。仅把规则/组、region labels、affinity及
scheduler配置的权威输入快照放入本地投影，由原canonical managers建立索引，
不复制调度算法、区域索引或创建私有runtime；此阶段不冒充所有policy的完整live watch。

第一次仅切换MemoryStore消除了越权写入，但标签分裂回归实际收到replica命令，
暴露遗漏PD label输入。导入策略快照后，placement开启场景通过；空rule缓存却因
强行prepare空patch而启动失败。按Go MCS skip-load语义初始化空manager，只有真实
rule/group变更才提交canonical patch，不合成PD默认规则，随后九项命令专项全部通过。
包含placement开启/关闭两种完整namespace对照，实际protobuf、ACK及PD ID断言保留。

相关native compaction回归还暴露旧夹具对这些额外共享写入推进revision的依赖：
移除写入后cursor等于压缩点，未出现Compacted。测试producer现显式写入独立barrier，
要求cursor严格小于实际compact revision；保留真实Compacted和更新cursor断言并清理
barrier。原失败及通过日志均保存，没有放宽成“无错误即可通过”。

当前编号16已维护九项有限收口清单与17–24明确后续责任；本阶段只修复16-G必要的
领域输入及越权持久差异。源码、完整namespace差异、输入及最终窄回归见
[持久空间与投影回执](task-16-scheduling-authority.json.gz)。
SplitRegions/ScatterRegions/RegionBuckets有效输入及16-D/E/F/H所列缺口继续，
不在本阶段扩展无关HTTP、指标、全配置或全workspace test矩阵。

## Scheduling三个有效方法与嵌入缓存证据（16-G）

`SplitRegions` 使用实际 `m` 分裂命令；RPC 在观察到两个真实子区间心跳前
保持未完成，`AskBatchSplit` 从真实 PD 分配三个 ID，之后响应返回新 region ID。
`ScatterRegions` 对实际双 voter 区间产生 leader transfer，只有真实 ACK 后
`GetOperator` 才转为 SUCCESS。比较保留完整 protobuf，只归一 cluster ID；
半关闭后的 EOF/状态也比较。关闭 sidecar 后比较完整持久 namespace，无键排除。

夹具最初用子区间 bootstrap，被 Go 的全范围要求拒绝；改为合法 bootstrap 后
报告子区间。Split 的首次 ID 请求还实际观察到 `PD leader is not found`，这是
分配前拒绝并触发发现，夹具仅对此错误等待重试，不重放已发送的分配。
Scatter 关闭 placement 时双方返回失败 region；debug 日志及只读源码明确
`allowLeader` 依赖 rule fit。正向夹具改用真实 PD 默认规则，不伪造候选或完成状态；
同一 group 的合法无操作响应保留，后续实际请求产生并完成 transfer。

有效 `RegionBuckets` 四帧包含新版本3、旧2、相同3及更新4，Go/Rust ACK 与
EOF 一致，完整 namespace 不变。另一个实际 `SchedulingHost` 在调用者 runtime
运行，连接真实 Go PD backend、参与选举，typed MCS RPC 输入同样四帧。直接读取
该宿主同一 canonical cache 和 HotStat，确认旧/同版本不覆盖、新版本更新且有
hot bucket 记录；PD region 持久条目不变。关闭宿主后调用方 token/runtime 和
PD 仍可用。这是实际嵌入宿主行为，不是另建 domain 内核或只检查 ACK。

原始命令、失败夹具、protobuf、完整 namespace、共享缓存观察与输入摘要见
[有效方法回执](task-16-scheduling-methods.json.gz)。该回执关闭本段三个正向方法缺口；
16-B/D/E/F/H 的明确剩余项仍按有限清单推进，不据此宣称42方法整体完成。

## Keyspace与Router流补齐及TSO三方法（16-B/E/F）

Keyspace消费09既有六方法/HTTP共享manager/持久证据，新增真实Go/Rust前端
取消旧Watch、状态更新、重连当前快照、存活流后端压缩、后续增量及再次重连。
保留完整protobuf及metadata bytes/revision；只归一cluster ID与领域时间。
这是存活流跨压缩证明，不冒充强制后端stale cursor故障；该后端能力由同一
ReconciledWatch故障回执提供。WatchKeyspaces没有请求游标/CloseSend，也没有
正常有限EOF；进程退出终止差异由22拥有。Diagnostics仍消费14/15已准入平台，
不扩展机器、平台或shutdown矩阵。

Router七方法非空缓存既有证据上，新增真实QueryRegion客户端取消与重开：
新流读取同一缓存的完整相同响应并正常半关闭EOF；原流取消不关闭宿主。
StoreWatcher更新/删除、共享SyncRegions恢复及宿主退出继续消费原相关回执。

TSO实际独立Go/Rust进程三方法，覆盖三帧count=1/4/2的非重叠递增时间戳、
FindGroup默认分组/primary、GetMinTS及真正落盘的timestamp条目；组目录未被
改写。HTTP和双向TLS均运行相同业务，TLS无证书/错误CN/非受信证书的GetMinTS
实际请求均拒绝。保留原始时间戳，比较时只归一动态cluster、地址与时间数值，
单独断言分配区间和最小时间戳递增。缺失/错误header也逐一比较原始响应。

实际red发现：Rust unary错误走transport，而Go返回CLUSTER_MISMATCHED业务
header；Rust虚拟默认分组误返回watch总revision而Go返回0。现在共用校验输出
准确错误文本/业务header，TSO流终止首个错误；每组revision随同一批原始Put
记录并原子发布，虚拟默认为0，不被其它group事件或reload推进。
GetMinTS保留caller给出的response group hint。新增真实后端两组事件/reload/
删除专项；嵌入TsoServiceHost使用有效Participant protobuf，实际调用三方法并
保留原双实例/启动失败/关闭流/父runtime隔离断言。11/13分组迁移及时间线持久
证据继续按原范围消费，不重跑无关全量HTTP或TSO压力。

原始RPC、red/green、准入和确切命令见[本阶段回执](task-16-streams-tso.json.gz)。
这不等于138/148索引已交付；16-D/H/I继续按有限清单收口。
