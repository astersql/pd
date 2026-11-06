# 验收16：额外gRPC注册与责任交接审计

2026-11-06；基准HEAD `128c50050748fac7820b48039759d77a522929ec`。
状态：**已阻塞**。已完成文档范围内的责任修复，尚未获得整项协议验收证据。
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
