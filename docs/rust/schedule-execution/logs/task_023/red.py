"""Prove the frozen range cases reject an epoch/unchanged-range regression."""
from pathlib import Path
import subprocess,os,json
source=Path('pkg/schedule/operator/step_range.rs');original=source.read_text()
out=Path('docs/rust/schedule-execution/logs/task_023')
try:
 changed=original.replace('region.meta().key_range.start_key != self.start_key','region.meta().key_range.start_key == self.start_key')
 assert changed!=original
 source.write_text(changed)
 subprocess.run(['cargo','fmt','--all'],check=True)
 argv=['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_023','--','--nocapture']
 p=subprocess.run(argv,capture_output=True,text=True);log=p.stdout+p.stderr
 (out/'red-behavior.out').write_text(log)
 (out/'red.json').write_text(json.dumps(dict(argv=argv,exit_code=p.returncode,cargo_target_dir=os.environ['CARGO_TARGET_DIR'],mutation='invert split start-key comparison',detected='assertion' in log and 'task_023_behavior_cases' in log),indent=2)+'\n')
 assert p.returncode==101 and 'assertion' in log and 'task_023_behavior_cases' in log
finally:
 source.write_text(original)
 subprocess.run(['cargo','fmt','--all'],check=True)
