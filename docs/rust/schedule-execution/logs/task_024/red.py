"""Prove the registered behavior test detects advancing an unfinished step."""
import json,os,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_024');source=Path('pkg/schedule/operator/operator.rs');original=source.read_text()
try:
 old='if !self.steps[i].is_finish(region) {'
 assert original.count(old)==1
 source.write_text(original.replace(old,'if self.steps[i].is_finish(region) {'))
 subprocess.run(['cargo','fmt','--all'],check=True)
 argv=['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_024','--','--nocapture']
 p=subprocess.run(argv,capture_output=True,text=True)
 (out/'red-behavior.out').write_text(p.stdout+p.stderr)
 (out/'red.json').write_text(json.dumps(dict(argv=argv,exit_code=p.returncode,cargo_target_dir=os.environ['CARGO_TARGET_DIR'],mutation='invert acknowledgement predicate in Operator::check',restored=True),indent=2)+'\n')
 assert p.returncode==101 and 'assertion' in p.stdout+p.stderr
finally:
 source.write_text(original)
 subprocess.run(['cargo','fmt','--all'],check=True)
