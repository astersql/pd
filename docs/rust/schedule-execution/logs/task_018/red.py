"""The same frozen behavior tests must reject a lost multi-target completion."""
from pathlib import Path
import subprocess,json,os
out=Path('docs/rust/schedule-execution/logs/task_018')
p=Path('pkg/schedule/operator/step_leader.rs');original=p.read_text()
try:
 p.write_text(original.replace('self.to_stores.contains(&leader) || leader == self.to_store','leader == self.to_store'))
 subprocess.run(['cargo','fmt','--all'],check=True)
 argv=['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_018','--','--nocapture']
 r=subprocess.run(argv,capture_output=True,text=True);(out/'red-behavior.out').write_text(r.stdout+r.stderr)
 (out/'red.json').write_text(json.dumps(dict(argv=argv,exit_code=r.returncode,cargo_target_dir=os.environ['CARGO_TARGET_DIR'],mutation='omit ToStores from heartbeat completion'),indent=2)+'\n')
 assert r.returncode==101 and 'task_018_behavior_cases ... FAILED' in r.stdout,r.stdout+r.stderr
finally:p.write_text(original)
