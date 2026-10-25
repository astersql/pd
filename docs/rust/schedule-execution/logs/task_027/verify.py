"""Compare every protected Go/module/plan byte and status with task admission."""
from pathlib import Path
import hashlib,json,subprocess
out=Path('docs/rust/schedule-execution/logs/task_027')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before=json.loads((out/'readonly-before.json').read_text())
def protected(p):
 return p.suffix=='.go' or p.name in ['go.mod','go.sum','go.work','go.work.sum','plan.md']
after={str(p):sha(p) for p in Path('.').rglob('*') if '.git' not in p.parts and 'target' not in p.parts and p.is_file() and protected(p)}
changes={p:[before.get(p),after.get(p)] for p in before.keys()|after.keys() if before.get(p)!=after.get(p)}
status=subprocess.check_output(['git','status','--porcelain=v1','-z'])
def entries(raw):
 return sorted(s.decode() for s in raw.split(b'\0') if s and protected(Path(s[3:].decode())))
status_equal=entries((out/'status-before.out').read_bytes())==entries(status)
red=json.loads((out/'red.json').read_text())
production=sha(Path('pkg/schedule/operator/builder_simple.rs'))
result=dict(readonly_files=len(before),go_module_files=sum(1 for p in before if Path(p).name!='plan.md'),readonly_changes=changes,readonly_status_equal=status_equal,diff_check_exit_code=subprocess.run(['git','diff','--check']).returncode,fixture_sha256=sha(Path('docs/rust/schedule-execution/fixtures/task_027.json')),production_sha256=production,mutation_restored=production==red['production_sha256'])
(out/'verification.json').write_text(json.dumps(result,indent=2)+'\n')
print(result)
assert not changes and status_equal and result['diff_check_exit_code']==0 and result['mutation_restored']
