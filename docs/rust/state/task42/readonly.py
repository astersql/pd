"""Verify all tracked/untracked Go sources and module manifests against task start."""
import hashlib
import json
from pathlib import Path
import subprocess

paths = subprocess.check_output([
    'git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'
]).decode().split('\0')
manifest = {
    p: hashlib.sha256(Path(p).read_bytes()).hexdigest()
    for p in paths
    if p and Path(p).is_file() and (
        p.endswith('.go') or Path(p).name in ('go.mod', 'go.sum', 'go.work', 'go.work.sum')
    )
}
digest = hashlib.sha256(json.dumps(manifest, sort_keys=True).encode()).hexdigest()
assert len(manifest) == 1101
assert digest == 'ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4'
status = subprocess.check_output([
    'git', 'status', '--porcelain', '--', '*.go', 'go.mod', 'go.sum', 'go.work', 'go.work.sum'
]).decode()
assert status == ''
print(json.dumps(dict(files=len(manifest), unchanged=True, sha256=digest, go_git_status=status), indent=2))
