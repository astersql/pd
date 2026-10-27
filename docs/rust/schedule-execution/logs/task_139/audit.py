"""Verify task139 evidence, protected Go files, readonly plans and shared slots."""
import gzip,hashlib,json,os,pathlib,re,subprocess,tomllib
root=pathlib.Path.cwd();folder=root/'docs/rust/schedule-execution/logs/task_139'
def digest(p):return hashlib.sha256(p.read_bytes()).hexdigest()
before=json.loads(gzip.open(folder/'initial.json.gz','rt').read());files={}
for base,dirs,names in os.walk(root):
 dirs[:]=[x for x in dirs if x not in ['.git','target','.tools']]
 for n in names:
  if n.endswith('.go') or n in ['go.mod','go.sum','go.work','go.work.sum']:
   p=pathlib.Path(base)/n;files[str(p.relative_to(root))]=digest(p)
assert files==before['files'],'protected Go/module contents or file set changed'
assert subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum'],text=True)==before['git_status']
plans=['.plans/2026-10-25-调度执行内核/plan.md','.plans/2026-10-25-调度执行内核/15-主题验收/plan.md']
for p in plans:assert (root/p).read_bytes()==subprocess.check_output(['git','show',before['head']+':'+p])
remote=lambda v:{(p['name'],p['version']):(p['source'],p.get('checksum')) for p in v['package'] if 'source' in p}
assert remote(tomllib.loads(subprocess.check_output(['git','show',before['head']+':Cargo.lock'],text=True)))==remote(tomllib.loads((root/'Cargo.lock').read_text()))
provenance=json.loads((folder/'provenance.json').read_text())
assert digest(root/'docs/rust/schedule-execution/fixtures/task_139.json')==provenance['fixture_sha256']
assert all(provenance['source_matches_baseline'].values())
assert digest(root/'docs/rust/schedule-execution/oracle-index.json')==provenance['oracle_index_sha256']
for p,h in provenance['additional_readonly_evidence'].items():assert digest(root/p)==h,p
for p,h in provenance['sources'].items():assert digest(root/p)==h,p
for p in folder.glob('*.json'):
 data=json.loads(p.read_text())
 if not isinstance(data,list):continue
 for x in data:assert digest(folder/x['log'])==x['sha256']
final=json.loads((folder/'final.json').read_text());assert len(final)==8
summaries=[]
for x in final:
 assert x['exit_code']==0,x['command']
 assert x['CARGO_TARGET_DIR']==str(root/f"target/rust-slot-{x['slot']}")
 totals=[0,0,0]
 for summary in x['test_summaries']:
  m=re.search(r'(\d+) passed; (\d+) failed; (\d+) ignored',summary)
  if m:totals=[a+int(b) for a,b in zip(totals,m.groups())]
 summaries.append(dict(command=x['command'],exit_code=x['exit_code'],passed=totals[0],failed=totals[1],ignored=totals[2]))
 lock=root/'target/rust-slot-locks'/f"slot-{x['slot']}.lock"/'owner'
 assert not lock.exists() or 'session=task-139' not in lock.read_text(),'own slot retained'
assert summaries[1]['passed']==5 and summaries[2]['passed']==63
assert summaries[3]['passed']==192
assert summaries[-1]['passed']>=1860 and summaries[-1]['failed']==0 and summaries[-1]['ignored']==1
focus=gzip.open(folder/'final-1.log.gz','rt').read()
for marker in ['HTTP cases=42 passed','6/6 configuration-to-decision','body I/O cases=2 passed','fault cases=4 passed','partial/snapshot cases=7 passed']:assert marker in focus
red=json.loads((folder/'red.json').read_text())[-1];assert red['exit_code']==101
assert 'left: 404' in gzip.open(folder/red['log'],'rt').read()
for p in [root/'docs/rust/schedule-execution/task-139.md',root/'pkg/server/schedule_admin_balance_witness.rs.md']:
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  if '://' not in link:
   target=(p.parent/link.split('#')[0]).resolve()
   if target!=folder/'audit.json':assert target.exists(),(p,link)
subprocess.run(['git','diff','--check'],check=True)
reuses=[json.loads(line) for line in (folder/'go-reuse.jsonl').read_text().splitlines()]
assert reuses and all(x['sha256']=='57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5' and x['action']=='copy_existing_etcd_fixture' for x in reuses)
(folder/'audit.json').write_text(json.dumps(dict(readonly_file_count=len(files),protected_files_and_status_equal=True,readonly_plans=plans,remote_dependencies_unchanged=True,own_locks_released=True,validation=summaries,source_hashes_and_fixture_verified=True,markdown_links='passed',diff_check_exit=0,effective_task_cases=61),indent=2)+'\n')
print(json.dumps(summaries,indent=2));print('protected sources, provenance, links, logs and slot cleanup passed')
