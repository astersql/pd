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
for name in ['docs/rust/state/task-15.md', 'pkg/statistics/region_classify.rs.md', 'pkg/statistics/lib.rs.md']:
    path = Path(name)
    for link in re.findall(r'\]\(([^)]+)\)', path.read_text()):
        assert (path.parent / link.split('#')[0]).exists(), (name, link)
print('documentation relative links resolve')
subprocess.run(['git', 'diff', '--check'], check=True)
print('git diff --check: exit 0')
