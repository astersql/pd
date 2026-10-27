"""Verify protected source, plans, Cargo manifests and local evidence links."""
import gzip, hashlib, json, os, pathlib, re, subprocess
root = pathlib.Path.cwd()
folder = root / 'docs/rust/schedule-execution/logs/task_144'
before = json.loads(gzip.open(folder / 'before.json.gz', 'rt').read())
files = {}
for parent, dirs, names in os.walk(root):
    dirs[:] = [n for n in dirs if n not in {'target', '.git', '.tools'}]
    for name in names:
        path = pathlib.Path(parent) / name
        if path.suffix == '.go' or name in {'go.mod', 'go.sum', 'go.work', 'go.work.sum'}:
            files[str(path.relative_to(root))] = hashlib.sha256(path.read_bytes()).hexdigest()
assert before['files'] == files
status = subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=all']).decode()
def protected_lines(value):
    return [line for line in value.splitlines() if line.endswith(('.go', 'go.mod', 'go.sum', 'go.work', 'go.work.sum'))]
assert protected_lines(before['status']) == protected_lines(status)
assert all(hashlib.sha256((root / path).read_bytes()).hexdigest() == digest for path, digest in before['plans'].items())
assert not subprocess.check_output(['git', 'diff', 'HEAD', '--', 'Cargo.toml', 'Cargo.lock', 'pkg/server/Cargo.toml'])
result = dict(go_module_source_files=len(files), content_and_file_set_unchanged=True, status_unchanged=True,
              read_only_plans_unchanged=True, cargo_manifests_and_lock_unchanged=True,
              git_diff_check=subprocess.run(['git', 'diff', '--check']).returncode, links=[])
assert result['git_diff_check'] == 0
for path in [root / 'docs/rust/schedule-execution/task-144.md', root / 'pkg/server/schedule_admin_hot_region.rs.md', root / 'pkg/proto/build.rs.md']:
    for link in re.findall(r'\]\(([^)]+)\)', path.read_text()):
        assert (path.parent / link).exists(), link
        result['links'].append(dict(source=str(path.relative_to(root)), target=link))
proto_inputs=[]
for directory in (root/'target/rust-slot-1/debug/build').glob('astersql-pd-proto-*/out/proto-source'):
    entries=[p for p in directory.rglob('*') if p.is_file()]
    assert entries and all(p.suffix == '.proto' for p in entries)
    proto_inputs.append(dict(directory=str(directory.relative_to(root)), files=len(entries), only_proto=True))
assert proto_inputs
result['protocol_extraction']=proto_inputs
(folder / 'audit.json').write_text(json.dumps(result, indent=2) + '\n')
print('protected files, plans, manifests and Markdown links unchanged/valid; diff check=0')
