"""Audit final evidence, protected sources/plans, locked dependencies and log integrity."""
import gzip,hashlib,json,os,pathlib,re,subprocess,tomllib
root=pathlib.Path.cwd();folder=root/'docs/rust/schedule-execution/logs/task_135'
def digest(p):return hashlib.sha256(p.read_bytes()).hexdigest()
before=json.loads(gzip.open(folder/'initial.json.gz','rt').read());files={}
for base,dirs,names in os.walk(root):
 dirs[:]=[x for x in dirs if x not in ['.git','target','.tools']]
 for n in names:
  if n.endswith('.go') or n in ['go.mod','go.sum','go.work','go.work.sum']:
   p=pathlib.Path(base)/n;files[str(p.relative_to(root))]=digest(p)
assert files==before['files'],'Go/module file set or contents changed'
status=subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum'],text=True)
assert status==before['git_status']
plans=['.plans/2026-10-25-调度执行内核/plan.md','.plans/2026-10-25-调度执行内核/15-主题验收/plan.md']
for p in plans:assert (root/p).read_bytes()==subprocess.check_output(['git','show','HEAD:'+p]),p
original=tomllib.loads(subprocess.check_output(['git','show',before['head']+':Cargo.lock'],text=True))
current=tomllib.loads((root/'Cargo.lock').read_text())
remote=lambda v:{(p['name'],p['version']):(p['source'],p.get('checksum')) for p in v['package'] if 'source' in p}
assert remote(original)==remote(current),'remote dependency changed'
final=json.loads((folder/'final.json').read_text());assert len(final)==8
for x in final:
 assert x['exit_code']==0,x['command']
 assert digest(folder/x['log'])==x['sha256']
assert 'task135 effective closed-loop cases=21' in gzip.open(folder/'final-1.log.gz','rt').read()
assert json.loads((folder/'scheduler-coverage.json').read_text())['scheduler_count']==18
sums=[]
for x in final:
 totals=[0,0,0]
 for summary in x['test_summaries']:
  m=re.search(r'(\d+) passed; (\d+) failed; (\d+) ignored',summary)
  if m:totals=[a+int(b) for a,b in zip(totals,m.groups())]
 sums.append(dict(command=x['command'],passed=totals[0],failed=totals[1],ignored=totals[2]))
assert sums[1]['passed']==3 and sums[2]['passed']==192 and sums[3]['passed']==23
assert sums[-1]['passed']>1800 and sums[-1]['failed']==0 and sums[-1]['ignored']==1
for x in final:
 lock=root/'target/rust-slot-locks'/f"slot-{x['slot']}.lock"/'owner'
 assert not lock.exists() or 'session=task-135' not in lock.read_text(),'own slot lock retained'
for p in [root/'docs/rust/schedule-execution/task-135.md']:
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  if '://' not in link:
   target=(p.parent/link.split('#')[0]).resolve()
   if target!=folder/'audit.json':assert target.exists(),(p,link)
subprocess.run(['git','diff','--check'],check=True)
provenance=json.loads((folder/'provenance.json').read_text())
assert digest(root/'docs/rust/schedule-execution/fixtures/task_135.json')==provenance['fixture_sha256']
assert all(provenance['source_matches_baseline'].values())
reuses=[json.loads(line) for line in (folder/'go-reuse.jsonl').read_text().splitlines()]
assert reuses and all(x['sha256']==provenance['existing_etcd_io_binary_sha256'] and x['action']=='copy_existing_etcd_fixture' for x in reuses)
assert all(pathlib.Path(x['source']).parent==pathlib.Path(x['destination']).parent==pathlib.Path(final[0]['CARGO_TARGET_DIR']) for x in reuses)

(folder/'audit.json').write_text(json.dumps(dict(readonly_file_count=len(files),file_set_and_hashes_equal=True,go_git_status=status,readonly_plans=plans,remote_dependencies_unchanged=True,own_locks_released=True,validation=sums,markdown_links='passed',diff_check_exit=0),indent=2)+'\n')
print(json.dumps(sums,indent=2));print('protected Go/module files, plans, logs, links, lock versions and slot cleanup passed')
