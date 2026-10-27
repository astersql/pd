"""Verify read-only inputs, evidence hashes, and Markdown links without Go/Cargo."""
from pathlib import Path
import subprocess,hashlib,json,re,gzip
root=Path.cwd();folder=root/'docs/rust/schedule-execution/logs/task_152'
base=json.loads((folder/'baseline.json').read_text())
files=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z']).decode().split('\0')
files=sorted(set(p for p in files if p and (p.endswith('.go') or Path(p).name in ['go.mod','go.sum','go.work','go.work.sum'])))
plans=['.plans/2026-10-25-调度执行内核/plan.md','.plans/2026-10-25-调度执行内核/15-主题验收/plan.md']
now={p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in files+plans if Path(p).is_file()}
assert now==base['hashes'],'read-only files changed'
status=subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum','**/go.mod','**/go.sum','**/go.work','**/go.work.sum']).decode()
assert status==base['status'],'Go Git state changed'
assert subprocess.run(['git','diff','--check']).returncode==0
assert subprocess.run(['git','diff','--cached','--check']).returncode==0
records=[]
for stage in ['compile','red','verified']:
 path=folder/f'{stage}.json'
 if not path.exists():continue
 for row in json.loads(path.read_text()):
  log=folder/row['log'];assert hashlib.sha256(log.read_bytes()).hexdigest()==row['sha256']
  assert Path(row['CARGO_TARGET_DIR']).is_absolute()
  records.append({'stage':stage,'command':row['command'],'exit_code':row['exit_code'],'slot':row['slot']})
rows=json.loads((folder/'verified.json').read_text());assert len(rows)==7 and all(r['exit_code']==0 for r in rows);summary=[]
for row in rows:
 totals=[tuple(map(int,m)) for m in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',gzip.open(folder/row['log'],'rt').read())]
 summary.append({'command':row['command'],'exit_code':row['exit_code'],'passed':sum(t[0] for t in totals),'failed':sum(t[1] for t in totals),'ignored':sum(t[2] for t in totals)})
(folder/'verified-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
(folder/'audit.json').write_text(json.dumps({'read_only_files_and_plans_unchanged':True,'checked_files':len(now),'go_git_status_unchanged':True,'markdown_links_valid':True,'git_diff_check_exit_code':0,'verified_logs':records},indent=2)+'\n')
print(json.dumps(summary,indent=2))

provenance=json.loads((folder/'provenance.json').read_text())
for row in provenance['sources']+provenance['fixtures']:
 assert hashlib.sha256(Path(row['path']).read_bytes()).hexdigest()==row['sha256']
assert hashlib.sha256(Path('docs/rust/schedule-execution/oracle-index.json').read_bytes()).hexdigest()==provenance['oracle_index_sha256']
assert not (root/'target/rust-slot-locks/slot-1.lock').exists(), 'own slot still occupied'

for p in [root/'docs/rust/schedule-execution/task-152.md',root/'pkg/server/schedule_admin_transfer_witness_leader.rs.md']:
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  if '://' not in link:assert (p.parent/link.split('#')[0]).exists(),link
