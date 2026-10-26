from pathlib import Path
import os,json,hashlib,subprocess
root=Path.cwd();logs=root/'docs/rust/schedule-execution/logs/task_108'
before=json.loads((logs/'readonly-before.json').read_text())
after={}
for directory,dirs,files in os.walk(root):
 dirs[:]=[d for d in dirs if d not in ('target','.git')]
 for name in files:
  if name.endswith('.go') or name in ('go.mod','go.sum','go.work','go.work.sum'):
   p=Path(directory)/name
   after[str(p.relative_to(root))]=hashlib.sha256(p.read_bytes()).hexdigest()
for name in before:
 if name.endswith('/plan.md'):after[name]=hashlib.sha256((root/name).read_bytes()).hexdigest()
changed=[name for name in sorted(before.keys()|after.keys()) if before.get(name)!=after.get(name)]
diff=subprocess.run(['git','diff','--check'],capture_output=True,text=True)
status=subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum','**/go.mod','**/go.sum','**/go.work','**/go.work.sum'],text=True)
record=dict(go_files=len(after)-2,readonly_plans=2,changed_files=changed,go_git_status=status,diff_check_exit=diff.returncode,diff_check_output=diff.stdout+diff.stderr)
(logs/'readonly-check.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record))
assert not changed and not status and diff.returncode==0
provenance=json.loads((logs/'provenance.json').read_text())
provenance['fixture_sha256']=hashlib.sha256((root/'docs/rust/schedule-execution/fixtures/task_108.json').read_bytes()).hexdigest()
provenance['readonly_sources']={name:hashlib.sha256((root/name).read_bytes()).hexdigest() for name in ['pkg/schedule/schedulers/scheduler_controller.go','pkg/schedule/schedulers/transfer_witness_leader.go','pkg/schedule/schedulers/evict_leader.go','pkg/schedule/operator/operator_controller.go','pkg/core/region.go']}
(logs/'provenance.json').write_text(json.dumps(provenance,indent=2)+'\n')
