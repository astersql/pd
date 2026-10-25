"""Verify all Go/module files and read-only plans against entry hashes."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
out = Path('docs/rust/schedule-execution/logs/task_003')
before = json.loads((out/'before.json').read_text())
files = {}
for root, dirs, names in os.walk('.'):
    dirs[:] = [d for d in dirs if d != '.git']
    for name in names:
        if name.endswith('.go') or name in ('go.mod', 'go.sum', 'go.work', 'go.work.sum'):
            p = os.path.join(root, name)[2:]
            files[p] = hashlib.sha256(Path(p).read_bytes()).hexdigest()
status = subprocess.check_output(['git','status','--short','--untracked-files=all','--','*.go',
    'go.mod','go.sum','go.work','go.work.sum','**/go.mod','**/go.sum'], text=True)
result = dict(before_count=len(before['files']), after_count=len(files),
    changed=[p for p in sorted(set(files)|set(before['files'])) if files.get(p)!=before['files'].get(p)],
    before_digest=hashlib.sha256(json.dumps(before['files'],sort_keys=True).encode()).hexdigest(),
    after_digest=hashlib.sha256(json.dumps(files,sort_keys=True).encode()).hexdigest(),
    before_status=before['status'], after_status=status,
    plans_checked=len(before['plans']),
    changed_plans=[p for p,h in before['plans'].items() if hashlib.sha256(Path(p).read_bytes()).hexdigest()!=h])
(out/'go-verification.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
assert not result['changed'] and status == before['status'] and not result['changed_plans']
