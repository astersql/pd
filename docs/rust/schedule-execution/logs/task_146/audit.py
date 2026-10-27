"""Read-only source and plan audit against the pre-task snapshot."""
import hashlib, json, re, subprocess
from pathlib import Path
folder = Path('docs/rust/schedule-execution/logs/task_146')
baseline = json.loads((folder / 'baseline.json').read_text())
files = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z']).decode().split('\0')
selected = sorted(set(f for f in files if f and (f.endswith('.go') or Path(f).name in ['go.mod', 'go.sum', 'go.work', 'go.work.sum'])))
hashes = {f: hashlib.sha256(Path(f).read_bytes()).hexdigest() if Path(f).is_file() else None for f in selected}
status = subprocess.check_output(['git', 'status', '--porcelain', '--', '*.go', 'go.mod', 'go.sum', 'go.work', 'go.work.sum']).decode()
plans = {p: hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in baseline['plans']}
checked_links = []
for name in ['docs/rust/schedule-execution/task-146.md', 'pkg/server/schedule_admin_label.rs.md']:
    p = Path(name)
    for target in re.findall(r'\]\(([^)]+)\)', p.read_text()):
        if '://' not in target:
            assert (p.parent / target.split('#')[0]).exists(), (name, target)
            checked_links.append([name, target])
check = subprocess.run(['git', 'diff', '--check'], capture_output=True, text=True)
result = dict(go_file_count=len(hashes), go_set_and_content_unchanged=hashes == baseline['go_files'], go_status_unchanged=status == baseline['go_status'], readonly_plans_unchanged=plans == baseline['plans'], checked_links=checked_links, diff_check_exit=check.returncode, diff_check_stdout=check.stdout, diff_check_stderr=check.stderr)
(folder / 'audit.json').write_text(json.dumps(result, indent=2) + '\n')
assert hashes == baseline['go_files']
assert status == baseline['go_status']
assert plans == baseline['plans']
assert check.returncode == 0
print(json.dumps({k:v for k,v in result.items() if k != 'checked_links'}))
