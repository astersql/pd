"""Audit task155 evidence and summarize two measured runs; never invokes Cargo/Go."""
from pathlib import Path
import subprocess,hashlib,json,re,gzip,statistics,collections
root=Path.cwd();folder=root/'docs/rust/schedule-execution/logs/task_155'
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
base=json.loads((folder/'baseline.json').read_text())
files=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z']).decode().split('\0')
files=sorted(set(p for p in files if p and (p.endswith('.go') or Path(p).name in ['go.mod','go.sum','go.work','go.work.sum'])))
plans=['.plans/2026-10-25-调度执行内核/plan.md','.plans/2026-10-25-调度执行内核/15-主题验收/plan.md']
assert {p:sha(p) for p in files+plans if Path(p).is_file()}==base['hashes']
status=subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum','**/go.mod','**/go.sum','**/go.work','**/go.work.sum']).decode()
assert status==base['status']
assert subprocess.run(['git','diff','--check']).returncode==0
assert subprocess.run(['git','diff','--cached','--check']).returncode==0
records=[]
for stage in ['compile','initial','setup','setup2','setup3','refined','cached','convergence','red','serial-workspace','recovery','runner-failure','verified']:
 for row in json.loads((folder/f'{stage}.json').read_text()):
  assert sha(folder/row['log'])==row['sha256']
  assert Path(row['CARGO_TARGET_DIR']).is_absolute()
  records.append(dict(stage=stage,command=row['command'],exit_code=row['exit_code'],slot=row['slot']))
red=json.loads((folder/'red.json').read_text())[-1]
assert red['exit_code']==101
redtext=gzip.open(folder/red['log'],'rt').read()
assert 'left: 0' in redtext and 'left: Some(5)' in redtext and '1 passed; 2 failed' in redtext
rows=json.loads((folder/'verified.json').read_text());assert len(rows)==8 and all(r['exit_code']==0 for r in rows)
summary=[]
for row in rows:
 totals=[tuple(map(int,m)) for m in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',gzip.open(folder/row['log'],'rt').read())]
 summary.append(dict(command=row['command'],exit_code=row['exit_code'],passed=sum(t[0] for t in totals),failed=sum(t[1] for t in totals),ignored=sum(t[2] for t in totals)))
provenance=json.loads((folder/'provenance.json').read_text())
for row in provenance['sources']+provenance['fixtures']:assert sha(row['path'])==row['sha256']
assert sha('tests/rust-compat/schedule_performance_test.rs')==provenance['test_source_sha256']
assert sha('docs/rust/schedule-execution/oracle-index.json')==provenance['oracle_index_sha256']
runs=[];convergence=[]
for i in [1,7]:
 s=gzip.open(folder/rows[i]['log'],'rt').read()
 samples=[json.loads(m) for m in re.findall(r'TASK155 (\{.*\})',s)]
 assert len(samples)==36
 runs.append(samples)
 conv=[json.loads(m) for m in re.findall(r'CONVERGENCE (\{.*\})',s)]
 assert conv==[dict(final_counts=[334,333,333],regions=1000,rounds=666,scheduler='balance-leader',stores=4)]
 convergence.append(conv[0])
key=lambda x:(x['scheduler'],x['cache'],x['stores'],x['regions'],x['sample'])
# Fixture permits equal-score empty targets across reconstructed snapshots only.
# Retain raw steps; normalize only both matching destination fields of a region move.
def normalized(x):
 q=dict(x['quality'])
 if x['scheduler']=='balance-region':
  targets=re.findall(r'(?:AddLearner|PromoteLearner) \{ to_store: (\d+)',q['steps'])
  assert len(targets)==2 and targets[0]==targets[1]
  assert 4<=int(targets[0])<=x['stores']
  q['steps']=re.sub(r'((?:AddLearner|PromoteLearner) \{ to_store: )\d+', r'\1EQUAL_SCORE_EMPTY_TARGET', q['steps'])
 return q
first={key(x):normalized(x) for x in runs[0]};second={key(x):normalized(x) for x in runs[1]}
assert len(first)==36 and first==second,'result quality changed beyond declared target tie normalization'
raw_first={key(x):x['quality'] for x in runs[0]}
raw_second={key(x):x['quality'] for x in runs[1]}
raw_differences=[dict(case=list(k),first=raw_first[k],second=raw_second[k]) for k in raw_first if raw_first[k]!=raw_second[k]]
groups=collections.defaultdict(list)
for run in runs:
 for x in run:groups[key(x)[:-1]].append(x)
stats=[]
for k,samples in sorted(groups.items()):
 assert len(samples)==6
 entry=dict(zip(['scheduler','cache','stores','regions'],k));entry['samples']=len(samples)
 for field in ['elapsed_ns','allocations','allocated_bytes']:
  values=[x[field] for x in samples];entry[field]=dict(min=min(values),median=statistics.median(values),max=max(values))
 stats.append(entry)
(folder/'performance.json').write_text(json.dumps(dict(runs=runs,groups=stats,quality_identical_after_declared_target_tie_normalization=True,raw_target_tie_differences=raw_differences,convergence=convergence),indent=2)+'\n')
(folder/'verified-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
for p in [root/'docs/rust/schedule-execution/task-155.md']:
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  if '://' not in link:
   target=(p.parent/link.split('#')[0]).resolve()
   assert target.exists() or target==(folder/'audit.json').resolve(),link
(folder/'audit.json').write_text(json.dumps(dict(read_only_files_and_plans_unchanged=True,checked_files=len(base['hashes']),go_git_status_unchanged=True,markdown_links_valid=True,git_diff_check_exit_code=0,verified_logs=records),indent=2)+'\n')
print(json.dumps(summary,indent=2));print(json.dumps(stats,indent=2))
