import hashlib
import json
import pathlib
import re
import subprocess

root = pathlib.Path.cwd()
evidence = root / 'docs/rust/schedule/integration/task41'
paths = subprocess.check_output(['git', 'ls-files', '-co', '--exclude-standard', '-z']).split(b'\0')
after = {
    p.decode(): hashlib.sha256((root / p.decode()).read_bytes()).hexdigest()
    for p in set(paths)
    if p and (p.endswith(b'.go') or pathlib.Path(p.decode()).name in
              ['go.mod', 'go.sum', 'go.work', 'go.work.sum'])
}
assert after == json.loads((evidence / 'go-before.json').read_text())
status = subprocess.check_output([
    'git', 'status', '--porcelain', '--untracked-files=all', '--', '*.go',
    'go.mod', 'go.sum', 'go.work', 'go.work.sum', '**/go.mod', '**/go.sum',
])
assert status == (evidence / 'go-status-before.out').read_bytes()
print(f'{len(after)} Go/module files: file set, SHA256, git status unchanged')
for name in ['docs/rust/schedule/integration/41-证据.md',
             'tests/rust-compat/schedule_foundation.rs.md']:
    path = root / name
    for target in re.findall(r'\]\(([^)]+)\)', path.read_text()):
        assert (path.parent / target).exists(), (name, target)
print('all relative documentation links valid')
subprocess.run(['git', 'diff', '--check'], check=True)
print('git diff --check exit=0')
