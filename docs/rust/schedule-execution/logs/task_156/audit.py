"""Trace source ownership to fresh behavioral results; never builds or writes Go."""
from pathlib import Path
import json,gzip,hashlib,re,subprocess,collections
root=Path.cwd();base=root/'docs/rust/schedule-execution';folder=base/'logs/task_156'
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
def load(p):return json.loads(Path(p).read_text())
def save(name,obj):(folder/name).write_text(json.dumps(obj,indent=2,ensure_ascii=False)+'\n')
records=load(folder/'verified.json')
assert all(r['exit_code']==0 for r in records)
for stage in ['inventory','red-registration','initial','corrected','red','verified','performance-diagnostic']:
 for row in load(folder/f'{stage}.json'):
  assert sha(folder/row['log'])==row['sha256']
  assert row['CARGO_TARGET_DIR']==str(root/f"target/rust-slot-{row['slot']}")
red=gzip.open(folder/load(folder/'red.json')[-1]['log'],'rt').read()
assert '0 passed; 3 failed' in red
summary=[]
for r in records:
 log=gzip.open(folder/r['log'],'rt').read()
 counts=[tuple(map(int,m)) for m in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',log)]
 summary.append(dict(command=r['command'],exit_code=0,passed=sum(x[0] for x in counts),failed=sum(x[1] for x in counts),ignored=sum(x[2] for x in counts)))
save('verified-summary.json',summary)
wsrow=next(r for r in records if r['command']=='cargo test --workspace --locked')
log=gzip.open(folder/wsrow['log'],'rt').read()
passing=[t for t in re.findall(r'^test (\S+) \.\.\. ok$',log,re.M) if re.search(r'task_\d{3}',t)]
assert len(passing)==len(set(passing)), 'ambiguous duplicate test names need crate qualification'
inventory=load(base/'source-inventory.json')
packages={p['path'] for p in inventory['packages']}
actual={str(p.relative_to(root)) for d in packages for p in (root/d).glob('*.go') if not p.name.endswith('_test.go')}
assert actual=={r['path'] for r in inventory['production_files']}
# This is an ownership/evidence audit, not statement or branch coverage instrumentation.
# Do not promote source derivation to Go runtime differential evidence.
owners={}
for n in range(2,156):
 tests=[t for t in passing if f'task_{n:03}' in t]
 assert tests,('no fresh behavioral tests for task',n)
 doc=base/f'task-{n:03}.md';fixture=base/f'fixtures/task_{n:03}.json'
 assert doc.is_file() and fixture.is_file()
 owners[n]=dict(evidence=str(doc.relative_to(root)),evidence_sha256=sha(doc),fixture=str(fixture.relative_to(root)),fixture_sha256=sha(fixture),fresh_passing_tests=tests,oracle_classification='source-derived plus Rust behavior, unless explicitly recorded Go execution in original evidence')
rows=[];totals=collections.Counter()
for r in inventory['production_files']:
 assert sha(root/r['path'])==r['sha256'],('source drift requires re-inventory',r['path'])
 assert r['file_tasks'] and all(n in owners for n in r['file_tasks'])
 row=dict(path=r['path'],source_sha256=r['sha256'],file_tasks=r['file_tasks'],items={})
 for field in ['public_symbols','internal_symbols','metrics','persistence_sites','asynchronous_sites','failpoints']:
  items=[]
  for i in r[field]:
   assert i['tasks'] and all(n in owners for n in i['tasks']),(r['path'],field,i)
   items.append(dict(line=i['line'],symbol=i.get('symbol_id',i.get('name',i.get('function',i.get('fq_name',i.get('source'))))),tasks=i['tasks']))
  row['items'][field]=items;totals[field]+=len(items)
 rows.append(row)
for r in inventory['persistence_key_definitions']:
 assert sha(root/r['file'])==r['sha256']
keys=[dict(path=r['file'],sha256=r['sha256'],ownership_tasks=[106,107,113,136],fresh_tests=[t for t in passing if any(f'task_{n:03}' in t for n in [106,107,113,136])]) for r in inventory['persistence_key_definitions']]
with gzip.open(folder/'source-coverage.json.gz','wt') as f:json.dump(dict(semantics='Inventory ownership traced to fresh owner-suite passes; not a claim of individual statement/branch instrumentation or Go execution',source_commit=inventory['source_commit'],no_source_drift=True,packages=sorted(packages),files=rows,tasks=owners,persistence_keys=keys),f,ensure_ascii=False,indent=2)
# Each actual policy's Schedule tests, gates and faults are separate from shared lifecycle.
coverage=load(base/'logs/task_135/scheduler-coverage.json')
config_tasks=[[70,71,137],[72,138],[73,74,139],[75,76,140],[77,78,145],[79,80,81,141],[82,142],[83,84,153],[85,86,87,88,89,90,91,92,93,94,95,144],[96,143],[97,146],[98,147],[99,148],[100,149],[101,150],[102,151],[103,152],[104,105,154]]
matrix=[]
for policy,ids in zip(coverage['policies'],config_tasks,strict=True):
 for t in policy['tests']:assert t in passing,t
 for path,expected in policy['fixtures'].items():assert sha(root/path)==expected,path
 matrix.append(dict(type=policy['type'],schedule_result_tests=policy['tests'],policy_limits_errors_and_configuration_tasks=ids,fresh_policy_tests=[t for t in passing if any(f'task_{n:03}' in t for n in ids)],config_roundtrip='task_156_behavior_cases',io_failure_retry='task_156_io_faults_and_invalid_configuration',pause_expiry_resume='task_156_pause_resume_all_factories',persistence_scope='MemoryStore contract backend; no production durability claim',exception=('Go factory ignores decoder, encodes null; witness queue is ephemeral and gate always true; actual store budget tested by task103' if 'transfer-witness' in policy['type'] else None)))
assert len(matrix)==18
save('scheduler-matrix.json',matrix)
metadata_row=next(r for r in records if r['command']=='cargo metadata --format-version 1 --no-deps')
meta=json.loads(gzip.open(folder/metadata_row['log'],'rt').read())
byname={p['name']:p for p in meta['packages']}
def deps(name):
 return [d['name'] for d in byname[name]['dependencies'] if d['kind'] is None]
domain=[n for n,p in byname.items() if '/pkg/schedule/' in p['manifest_path'] or n in ['astersql-pd-schedule','astersql-pd-cluster-utils','astersql-pd-progress','astersql-pd-operatorutil']]
violations=[]
for start in domain:
 pending=[(start,[start])];seen=set()
 while pending:
  current,path=pending.pop()
  if current in seen:continue
  seen.add(current)
  for d in deps(current):
   if d in ['astersql-pd-proto','astersql-pd-server','astersql-pd-runtime','tonic','prost','axum','hyper','tower']:
    violations.append(path+[d])
   if d in byname:pending.append((d,path+[d]))
save('dependency-audit.json',dict(domain_crates=domain,normal_transitive_dependencies_free_of_proto_transport_server_runtime=not violations,violating_paths=violations,dev_dependencies_are_test_harness_only=True,tree_log=next(r['log'] for r in records if r['command']=='cargo tree --workspace --locked')))
# Replay the measured snapshot baseline; elapsed time/allocation values are recorded, not gated.
performance_row=next(r for r in records if '--test schedule_performance' in r['command'])
performance_log=gzip.open(folder/performance_row['log'],'rt').read()
samples=[json.loads(m) for m in re.findall(r'TASK155 (\{.*\})',performance_log)]
assert len(samples)==36
convergence=[json.loads(m) for m in re.findall(r'CONVERGENCE (\{.*\})',performance_log)]
assert convergence==[dict(final_counts=[334,333,333],regions=1000,rounds=666,scheduler='balance-leader',stores=4)]
previous=load(base/'logs/task_155/performance.json')['runs'][0]
def normalized(x):
 q=dict(x['quality'])
 if x['scheduler']=='balance-region':
  targets=re.findall(r'(?:AddLearner|PromoteLearner) \{ to_store: (\d+)',q['steps'])
  assert len(targets)==2 and targets[0]==targets[1] and 4<=int(targets[0])<=x['stores']
  q['steps']=re.sub(r'((?:AddLearner|PromoteLearner) \{ to_store: )\d+',r'\1EQUAL_SCORE_EMPTY_TARGET',q['steps'])
 return q
key=lambda x:(x['scheduler'],x['cache'],x['stores'],x['regions'],x['sample'])
current_quality={key(x):normalized(x) for x in samples}
previous_quality={key(x):normalized(x) for x in previous}
performance_differences=[dict(case=list(k),current=current_quality[k],previous=previous_quality.get(k)) for k in current_quality if current_quality[k]!=previous_quality.get(k)]
score_quality_matches=all({k:v for k,v in current_quality[key(x)].items() if k!='steps'}=={k:v for k,v in previous_quality[key(x)].items() if k!='steps'} for x in samples)
save('performance-regression.json',dict(samples=samples,convergence=convergence,result_quality_matches_task155_under_its_declared_target_tie_normalization=not performance_differences,score_quality_matches=score_quality_matches,operation_differences_retained=performance_differences,source_tie_interpretation='Existing task155 check_operator permits removal source1..3 and validates kind/steps against actual source; Go balance_region.go sorts scores without ID tie break. Fresh harness passes. No claim of byte-identical cross-process operations.',normalization='Only matching AddLearner/PromoteLearner empty equal-score target IDs, range 4..stores. Times and allocations recorded, not compared.'))

# Include ignored Go files as well as tracked/untracked module files.
names=subprocess.check_output(['rg','--files','--hidden','--no-ignore','-g','*.go','-g','go.mod','-g','go.sum','-g','go.work','-g','go.work.sum','-g','!.git/**']).decode().splitlines()
plans=['.plans/2026-10-25-调度执行内核/plan.md','.plans/2026-10-25-调度执行内核/15-主题验收/plan.md']
before=load(folder/'baseline.json')
assert {p:sha(p) for p in sorted(names+plans)}==before['hashes'],'Go/plan bytes or file set changed'
status=subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum','**/go.mod','**/go.sum','**/go.work','**/go.work.sum']).decode()
assert status==before['status']
assert subprocess.run(['git','diff','--check']).returncode==0
assert subprocess.run(['git','diff','--cached','--check']).returncode==0
save('audit.json',dict(source_commit=inventory['source_commit'],production_files=len(rows),source_packages=len(packages),source_counts=dict(totals),source_drift=[],ownership_tasks_with_fresh_results=len(owners),factories=len(matrix),readonly_files_and_plans=len(before['hashes']),go_status_unchanged=True,all_cargo_commands_exit_zero=True,theme_acceptance_passed=not violations and score_quality_matches,dependency_violations=len(violations),performance_operation_differences=len(performance_differences),red_detected_three_behavior_failures=True,delayed_regression_tasks_freshly_passing={n:owners[n]['fresh_passing_tests'] for n in [58,67]},ignored=[t for t in re.findall(r'^test (\S+) \.\.\. ignored',log,re.M)],limitations=['Ownership/suite-level traceability, not per-symbol instrumentation','No new Go runtime differential; user-approved source derivation','Synthetic snapshots, test/debug performance, in-memory persistence; not production cluster or durability proof']))
for row in load(folder/'provenance.json')['sources']:
 assert sha(root/row['path'])==row['sha256'],row['path']
for link in re.findall(r'\]\(([^)]+)\)',(base/'task-156.md').read_text()):
 if '://' not in link:
  assert (base/link.split('#')[0]).resolve().exists(),link
print(json.dumps(load(folder/'audit.json'),indent=2))

if violations or not score_quality_matches:
 print('BLOCKED: required dependency or performance score-quality audit failed; do not claim theme complete')
 raise SystemExit(1)
