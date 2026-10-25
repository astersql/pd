"""Prove the frozen fixture detects a missing region-wide exit safety condition."""
from pathlib import Path
import subprocess,json,os
p=Path('pkg/schedule/operator/step_joint.rs');original=p.read_text()
needle='&& !is_in_joint_state(&r.meta().peers)'
assert original.count(needle)==1
try:
 p.write_text(original.replace(needle,''))
 subprocess.run(['cargo','fmt','--all'],check=True)
 argv=['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_022','--','--nocapture']
 run=subprocess.run(argv,capture_output=True,text=True)
 out=Path('docs/rust/schedule-execution/logs/task_022');(out/'red-behavior.out').write_text(run.stdout+run.stderr)
 (out/'red.json').write_text(json.dumps(dict(argv=argv,exit_code=run.returncode,cargo_target_dir=os.environ['CARGO_TARGET_DIR'],mutation='remove region-wide joint check from Leave::is_finish'),indent=2)+'\n')
 assert run.returncode==101 and 'other-joint-after' in run.stdout+run.stderr,run.stdout+run.stderr
finally:
 p.write_text(original)
 subprocess.run(['cargo','fmt','--all'],check=True)
