import hashlib
import json
from pathlib import Path
import re
import subprocess

root = Path(__file__).resolve().parents[4]
assert Path.cwd() == root
paths = set(subprocess.check_output(['git', 'ls-files', '-co', '--exclude-standard', '-z']).decode().split('\0'))
paths = sorted(p for p in paths if p and (p.endswith('.go') or Path(p).name in ['go.mod', 'go.sum', 'go.work', 'go.work.sum']))
after = {p: hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in paths if Path(p).is_file()}
before = json.loads((Path(__file__).parent / 'go-before.json').read_text())
assert after == before, 'Go/module file content or membership changed'
status = subprocess.check_output(['git', 'status', '--short', '--', '*.go', 'go.mod', 'go.sum', 'go.work', 'go.work.sum']).decode()
assert status == (Path(__file__).parent / 'go-status-before.out').read_text(), status
print(f'{len(after)} Go/module files: same membership, SHA256 and clean Git state')
print('aggregate SHA256=' + hashlib.sha256(json.dumps(after, sort_keys=True).encode()).hexdigest())
for name in ['docs/rust/state/task-26.md', 'pkg/statistics/lib.rs.md', 'pkg/statistics/slow_stat.rs.md']:
    path = Path(name)
    for link in re.findall(r'\]\(([^)]+)\)', path.read_text()):
        assert (path.parent / link.split('#')[0]).exists(), (name, link)
print('documentation relative links resolve')
subprocess.run(['git', 'diff', '--check'], check=True)
subprocess.run(['git', 'diff', '--cached', '--check'], check=True)
print('git diff --check and git diff --cached --check: exit 0')

for name, digest in json.loads((Path(__file__).parent / 'readonly-plans.json').read_text()).items():
    assert hashlib.sha256(Path(name).read_bytes()).hexdigest() == digest, name
print('all recorded readonly plan.md hashes unchanged')
