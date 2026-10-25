"""Run the required validation phase; inherit the caller's shared Cargo slot."""
import json
import os
import re
import subprocess
from pathlib import Path
out = Path('docs/rust/schedule-execution/logs/task_003')
assert os.environ['CARGO_TARGET_DIR'].startswith(str(Path.cwd() / 'target/rust-slot-'))
commands = [
    ['cargo', 'fmt', '--all'],
    ['cargo', 'test', '-p', 'astersql-pd-schedule-core', '--locked', '--lib', 'task_003', '--', '--nocapture'],
    ['cargo', 'test', '-p', 'astersql-pd-schedule-core', '--locked'],
    ['cargo', 'metadata', '--format-version', '1', '--no-deps'],
    ['cargo', 'fmt', '--all', '--', '--check'],
    ['cargo', 'check', '--workspace', '--locked'],
    ['cargo', 'test', '--workspace', '--locked'],
]
results = []
for i, cmd in enumerate(commands):
    print('RUN', ' '.join(cmd), flush=True)
    p = subprocess.run(cmd, capture_output=True, text=True)
    (out / f'validation-{i}.out').write_text(p.stdout + p.stderr)
    counts = [tuple(map(int, m)) for m in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', p.stdout)]
    result = dict(argv=cmd, exit_code=p.returncode, cargo_target_dir=os.environ['CARGO_TARGET_DIR'],
        output=f'validation-{i}.out', passed=sum(x[0] for x in counts), failed=sum(x[1] for x in counts), ignored=sum(x[2] for x in counts))
    results.append(result)
    (out / 'validation.json').write_text(json.dumps(results, indent=2) + '\n')
    print(json.dumps(result), flush=True)
    if p.returncode:
        print(p.stderr[-4000:], flush=True)
        raise SystemExit(p.returncode)
