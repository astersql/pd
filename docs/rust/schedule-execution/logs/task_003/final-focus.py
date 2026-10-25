import json
import os
import subprocess
from pathlib import Path
out = Path('docs/rust/schedule-execution/logs/task_003')
commands = [['cargo','fmt','--all'], ['cargo','test','-p','astersql-pd-schedule-core',
    '--locked','--lib','task_003','--','--nocapture']]
results = []
for cmd in commands:
    p = subprocess.run(cmd, capture_output=True, text=True)
    results.append(dict(argv=cmd, exit_code=p.returncode, stdout=p.stdout, stderr=p.stderr,
        cargo_target_dir=os.environ['CARGO_TARGET_DIR']))
    (out/'final-focus.json').write_text(json.dumps(results,indent=2)+'\n')
    print(' '.join(cmd), 'exit', p.returncode, flush=True)
    if p.returncode:
        raise SystemExit(p.returncode)
