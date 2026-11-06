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
