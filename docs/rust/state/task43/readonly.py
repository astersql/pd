"""Compare Go/module inventory, content, status and read-only plans to task start."""
import hashlib
import json
from pathlib import Path
import subprocess

p = Path(__file__).parent
before = json.loads((p / 'before.json').read_text())
paths = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z']).decode().split('\0')
files = {f: hashlib.sha256(Path(f).read_bytes()).hexdigest() for f in paths if f and Path(f).is_file() and (f.endswith('.go') or Path(f).name in ('go.mod','go.sum','go.work','go.work.sum'))}
assert files == before['files'], 'Go/module content or inventory changed'
status = subprocess.check_output(['git', 'status', '--porcelain', '--', '*.go', 'go.mod', 'go.sum', 'go.work', 'go.work.sum']).decode()
assert status == before['go_status']
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest() == digest for f, digest in before['plans'].items())
print(json.dumps(dict(files=len(files), unchanged=True, sha256=hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest(), go_git_status=status, plans_unchanged=True), indent=2))
