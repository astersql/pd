# Read-only audit; invoke from the repository root after the final test run.
from pathlib import Path
import subprocess,hashlib,json,gzip,re
logs=Path('docs/rust/schedule-execution/logs/task_106')
before=json.loads(gzip.decompress((logs/'go-before.json.gz').read_bytes()))
files=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z']).decode().split('\0')
after={f:hashlib.sha256(Path(f).read_bytes()).hexdigest() for f in files if f and (f.endswith('.go') or Path(f).name in ['go.mod','go.sum','go.work','go.work.sum']) and Path(f).is_file()}
assert after==before,'Go/module file inventory or content changed'
plans=['.plans/2026-10-25-调度执行内核/plan.md','.plans/2026-10-25-调度执行内核/08-内置调度器/plan.md']
assert not subprocess.check_output(['git','diff','--',*plans])
subprocess.run(['git','diff','--check'],check=True)
for f in ['docs/rust/schedule-execution/task-106.md','pkg/schedule/schedulers/registry.rs.md','pkg/schedule/schedulers/base.rs.md','pkg/schedule/schedulers/balance_range_job.rs.md']:
 p=Path(f)
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  if '://' in link or link.startswith('#'):continue
  assert (p.parent/link.split('#')[0]).exists(),(f,link)
record=dict(command=['python3',str(logs/'verify.py')],exit_code=0,go_files_checked=len(after),all_content_hashes_equal=True,readonly_plans_unchanged=True,diff_check=True,document_links=True)
(logs/'readonly-check.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record))
