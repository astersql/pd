"""Compare every Go/module file, including ignored/untracked files, to entry snapshot."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

baseline = json.loads(Path('docs/rust/schedule-execution/logs/task_002/before.json').read_text())
files = {}
for root, dirs, names in os.walk('.'):
    dirs[:] = [d for d in dirs if d != '.git']
    for name in names:
        if name.endswith('.go') or name in ('go.mod', 'go.sum', 'go.work', 'go.work.sum'):
            path = os.path.join(root, name)[2:]
            files[path] = hashlib.sha256(Path(path).read_bytes()).hexdigest()
status = subprocess.check_output(['git', 'status', '--short', '--untracked-files=all', '--',
    '*.go', 'go.mod', 'go.sum', 'go.work', 'go.work.sum', '**/go.mod', '**/go.sum'], text=True)
changed = [p for p in sorted(set(files) | set(baseline['files'])) if files.get(p) != baseline['files'].get(p)]
plans = json.loads(Path('docs/rust/schedule-execution/logs/task_001/before.json').read_text())['plans']
plan_changes = [p for p, digest in plans.items() if hashlib.sha256(Path(p).read_bytes()).hexdigest() != digest]
result = {
    'before_count': len(baseline['files']), 'after_count': len(files), 'changed': changed,
    'before_digest': hashlib.sha256(json.dumps(baseline['files'], sort_keys=True).encode()).hexdigest(),
    'after_digest': hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest(),
    'before_status': baseline['status'], 'after_status': status,
    'plans_checked': len(plans), 'plan_changes': plan_changes,
}
Path('docs/rust/schedule-execution/logs/task_002/go-verification.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
assert not changed and status == baseline['status'] and not plan_changes
