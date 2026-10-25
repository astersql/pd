"""Verify protected inputs and final task evidence without invoking Go or Cargo."""
from pathlib import Path
import json,hashlib,subprocess,os
root=Path(__file__).resolve().parents[5];out=Path(__file__).resolve().parent
before=json.loads((out/'before.json').read_text())
changed=[p for p,h in before['hashes'].items() if not (root/p).exists() or hashlib.sha256((root/p).read_bytes()).hexdigest()!=h]
def protected(p):return p.endswith('.go') or Path(p).name in ('go.mod','go.sum','go.work','go.work.sum')
paths=subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z'],cwd=root).decode().split('\0')
added=[p for p in paths if p and protected(p) and p not in before['hashes']]
status=subprocess.check_output(['git','status','--porcelain','--untracked-files=all'],cwd=root).decode()
normal=[]
for p in out.glob('*.log'):
 text=p.read_text();normalized='\n'.join(line.rstrip() for line in text.splitlines()).rstrip()+'\n'
 if text!=normalized:
  normal.append(p.name);p.write_text(normalized)
check=subprocess.run(['git','diff','--check'],cwd=root,capture_output=True,text=True)
record=dict(protected_count=sum(protected(p) for p in before['hashes']),changed_protected_or_plans=changed,added_protected=added,fixture_sha256=hashlib.sha256((root/'docs/rust/schedule-execution/fixtures/task_045.json').read_bytes()).hexdigest(),git_diff_check=check.returncode,git_diff_check_output=check.stdout+check.stderr,normalization={'rule':'trim trailing whitespace/extra final blank lines only','logs':normal},status=status)
(out/'audit.json').write_text(json.dumps(record,indent=2)+'\n')
assert not changed and not added and check.returncode==0,record
print(json.dumps({k:v for k,v in record.items() if k not in ['status','normalization']},indent=2))
