"""Replay the threshold mutation under a slot claimed by slot-run.py."""
import json,os,subprocess
from pathlib import Path
assert Path(os.environ['CARGO_TARGET_DIR']).is_absolute()
p=Path('pkg/schedule/operator/influence.rs');original=p.read_text()
out=Path('docs/rust/schedule-execution/logs/task_017')
try:
 assert 'region_size > SMALL_REGION_THRESHOLD' in original
 p.write_text(original.replace('region_size > SMALL_REGION_THRESHOLD','region_size >= SMALL_REGION_THRESHOLD'))
 argv=['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_017','--','--nocapture']
 r=subprocess.run(argv,capture_output=True,text=True)
 (out/'red-threshold.out').write_text(r.stdout+r.stderr)
 (out/'red.json').write_text(json.dumps(dict(argv=argv,exit_code=r.returncode,mutation='> threshold changed to >=',log='red-threshold.out'))+'\n')
 assert r.returncode==101 and 'task_017_behavior_cases ... FAILED' in r.stdout
finally:
 p.write_text(original)
 subprocess.run(['cargo','fmt','--all'],check=True)
