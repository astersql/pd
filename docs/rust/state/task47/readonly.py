import hashlib
import json
import subprocess
from pathlib import Path
p = Path(__file__).parent
before = json.loads((p / 'before.json').read_text())
files = subprocess.check_output(['rg', '--files', '--hidden', '-g', '!.git', '-g', '!target', '-g', '*.go', '-g', 'go.mod', '-g', 'go.sum', '-g', 'go.work', '-g', 'go.work.sum'], text=True).splitlines()
current = {s: hashlib.sha256(Path(s).read_bytes()).hexdigest() for s in sorted(files)}
assert current == before['files'], 'Go/module file list or content changed'
status = subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=all', '--', '*.go', 'go.mod', 'go.sum', 'go.work', 'go.work.sum'], text=True)
assert status == before['status'], 'Go/module Git status changed'
for path, digest in before['plans'].items():
    assert hashlib.sha256(Path(path).read_bytes()).hexdigest() == digest, path
subprocess.run(['git', 'diff', '--check'], check=True)
result = dict(files=len(current), aggregate_sha256=hashlib.sha256(json.dumps(current, sort_keys=True).encode()).hexdigest(), contents_and_status_unchanged=True, readonly_plans_unchanged=True)
(p / 'readonly.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result))
