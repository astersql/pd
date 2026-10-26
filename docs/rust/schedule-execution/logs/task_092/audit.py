# Read-only audit of Go/module/plan files, including untracked files.
from pathlib import Path
import os, json, gzip, hashlib, subprocess
root = Path.cwd().resolve()
logs = root / 'docs/rust/schedule-execution/logs/task_092'
with gzip.open(logs / 'before.json.gz', 'rt') as stream:
    before = json.load(stream)
after = {}
for base, dirs, names in os.walk(root):
    dirs[:] = [name for name in dirs if name not in ['.git', 'target']]
    for name in names:
        path = Path(base) / name
        if path.suffix == '.go' or name in ['go.mod', 'go.sum', 'go.work', 'go.work.sum', 'plan.md']:
            after[str(path.relative_to(root))] = hashlib.sha256(path.read_bytes()).hexdigest()
changed = [path for path in before.keys() | after.keys() if before.get(path) != after.get(path)]
status = subprocess.check_output(['git', 'status', '--short', '--', '*.go', 'go.mod', 'go.sum', 'go.work', 'go.work.sum']).decode()
diff_code = subprocess.run(['git', 'diff', '--check']).returncode
source = '.plans/2026-10-22-pd-rust-渐进迁移主题/6-迁移调度执行内核.md'
source_code = subprocess.run(['git', 'diff', '--exit-code', '--', source]).returncode
record = dict(command='python3 docs/rust/schedule-execution/logs/task_092/audit.py', files=len(before), changed=changed, go_status=status, diff_check_exit=diff_code, source_topic_diff_exit=source_code)
(logs / 'audit.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record))
assert not changed and not status and diff_code == source_code == 0
