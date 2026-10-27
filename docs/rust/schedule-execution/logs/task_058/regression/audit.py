"""Verify fresh task058 regression, including ignored Go source snapshots."""
from pathlib import Path
import json,hashlib,subprocess,gzip,re,os
root=Path.cwd(); folder=root/'docs/rust/schedule-execution/logs/task_058/regression'
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
load=lambda p:json.loads(Path(p).read_text())
before=load(folder/'before.json')
current={}
for directory,dirs,names in os.walk('.'):
 dirs[:]=[d for d in dirs if d not in ('.git','target')]
 for name in names:
  p=Path(directory)/name
  if p.suffix=='.go' or p.name in ('go.mod','go.sum','go.work','go.work.sum'):current[str(p)]=sha(p)
assert current==before['go_files']
for p,h in before['plans'].items():assert sha(p)==h,p
# The last task156 audit predates this run and also froze ignored generated Go files.
reference=root/'docs/rust/schedule-execution/logs/task_156/baseline.json'
upstream=load(reference)['hashes']
expected={p:h for p,h in upstream.items() if Path(p).suffix=='.go' or Path(p).name in ('go.mod','go.sum','go.work','go.work.sum')}
names=subprocess.check_output(['rg','--files','--hidden','--no-ignore','-g','*.go','-g','go.mod','-g','go.sum','-g','go.work','-g','go.work.sum','-g','!.git/**']).decode().splitlines()
assert {p:sha(p) for p in names}==expected
status=subprocess.check_output(['git','status','--porcelain','-z','--untracked-files=all']).decode().split('\0')
isgo=lambda row: row and (Path(row[3:]).suffix=='.go' or Path(row[3:]).name in ('go.mod','go.sum','go.work','go.work.sum'))
assert [s for s in status if isgo(s)]==[s for s in before['git_status'] if isgo(s)]
records=load(folder/'final.json');assert len(records)==6 and all(r['exit_code']==0 for r in records)
summary=[]
for row in records:
 log=folder/row['log'];assert sha(log)==row['sha256']
 assert row['CARGO_TARGET_DIR']==str(root/f"target/rust-slot-{row['slot']}")
 text=gzip.open(log,'rt').read();counts=[tuple(map(int,m)) for m in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',text)]
 summary.append(dict(command=row['command'],exit_code=row['exit_code'],passed=sum(x[0] for x in counts),failed=sum(x[1] for x in counts),ignored=sum(x[2] for x in counts)))
assert summary[0]['passed']==8 and summary[1]['passed']==66
assert summary[-1]['passed']>1900 and summary[-1]['failed']==0
workspace=gzip.open(folder/records[-1]['log'],'rt').read()
ignored=re.findall(r'^test (\S+) \.\.\. ignored',workspace,re.M)
assert len(ignored)==1 and ignored[0]=='tso_processes',ignored
reuse=[json.loads(line) for line in (folder/'go-reuse.jsonl').read_text().splitlines()]
assert reuse and all(r['action']=='atomic_hardlink_existing_etcd_fixture' and r['sha256']=='57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5' for r in reuse)

for cmd in [['git','diff','--check'],['git','diff','--cached','--check']]:subprocess.run(cmd,check=True)
for doc in [root/'docs/rust/schedule-execution/task-058.md',root/'pkg/schedule/checker/checker_metrics.rs.md']:
 for link in re.findall(r'\]\(([^)]+)\)',doc.read_text()):
  if '://' not in link:assert (doc.parent/link.split('#')[0]).exists(),link
result=dict(workspace_head=before['head'],go_files_excluding_target=len(current),all_go_files_including_ignored=len(expected),go_unchanged=True,upstream_ignored_go_baseline=str(reference.relative_to(root)),upstream_baseline_sha256=sha(reference),readonly_plans_unchanged=True,go_git_status_unchanged=True,all_six_commands_passed=True,summary=summary,ignored=ignored,etcd_fixture_hardlink_requests=len(reuse),links_valid=True,diff_check_exit=0)
(folder/'audit.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
