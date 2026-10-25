"""Mutate pending completion, demand assertion failure, then restore source."""
from pathlib import Path
import subprocess,os,json
p=Path('pkg/schedule/operator/step_witness.rs');original=p.read_text()
try:
 bad=original.replace('&& r.get_pending_peer(p.id).is_none()', '')
 assert bad!=original
 p.write_text(bad)
 subprocess.run(['cargo','fmt','--all'],check=True)
 argv=['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_021_behavior_cases','--','--nocapture']
 r=subprocess.run(argv,capture_output=True,text=True)
 out=Path('docs/rust/schedule-execution/logs/task_021');(out/'red-behavior.out').write_text(r.stdout+r.stderr)
 (out/'red.json').write_text(json.dumps(dict(argv=argv,exit_code=r.returncode,cargo_target_dir=os.environ['CARGO_TARGET_DIR'],mutation='ignore pending snapshot in BecomeNonWitness::is_finish'),indent=2)+'\n')
 assert r.returncode==101 and 'pending-data' in r.stdout+r.stderr
finally:
 p.write_text(original)
 subprocess.run(['cargo','fmt','--all'],check=True)
