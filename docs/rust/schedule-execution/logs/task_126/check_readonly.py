"""Compare the recorded starting Go/module and read-only plan content inventory."""
import gzip
import hashlib
import json
from pathlib import Path
import subprocess

folder = Path('docs/rust/schedule-execution/logs/task_126')
before = json.loads(gzip.decompress((folder / 'before.json.gz').read_bytes()))
paths = subprocess.check_output(['rg', '--files', '--hidden', '-g', '!.git', '-g', '!target']).decode().splitlines()
protected = [p for p in paths if p.endswith('.go') or Path(p).name in ['go.mod', 'go.sum', 'go.work', 'go.work.sum'] or p in [
    '.plans/2026-10-25-调度执行内核/plan.md',
    '.plans/2026-10-25-调度执行内核/11-调度管理/plan.md',
]]
after = {p: hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in protected}
changes = [p for p in before.keys() | after.keys() if before.get(p) != after.get(p)]
status = subprocess.check_output(['git', 'status', '--porcelain', '--', '*.go', 'go.mod', 'go.sum', 'go.work', 'go.work.sum', '**/go.mod', '**/go.sum', '**/go.work', '**/go.work.sum']).decode()
check = subprocess.run(['git', 'diff', '--check'], capture_output=True, text=True)
result = dict(inventory_count=len(before), go_module_count=len(before)-2, content_or_path_changes=changes, git_go_module_status=status, diff_check_exit=check.returncode, diff_check_output=check.stdout+check.stderr)
(folder / 'verification.json').write_text(json.dumps(result, indent=2)+'\n')
assert not changes, changes
assert not status, status
assert check.returncode == 0, result
print(json.dumps(result, indent=2))
