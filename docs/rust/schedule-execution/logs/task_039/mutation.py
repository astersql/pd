"""Run final tests against deliberate regressions, restoring source on all exits.
Invoked only through run.py so every Cargo child inherits the owned slot.
"""
from pathlib import Path
import subprocess,os,json
assert Path(os.environ['CARGO_TARGET_DIR']).is_absolute()
p=Path('pkg/schedule/checker/role_checkers.rs')
original=p.read_text()
mutations=[
 ('ignore-learner-pause','if self\n            .pause', 'if false && self\n            .pause'),
 ('joint-priority','operator.set_priority_level(PriorityLevel::HIGH)','operator.set_priority_level(PriorityLevel::MEDIUM)'),
 ('stop-at-rejected-learner','Err(error) => result.errors.push(error),','Err(error) => { result.errors.push(error); break; },'),
]
results=[]
try:
 for name,before,after in mutations:
  assert before in original,name
  p.write_text(original.replace(before,after,1))
  # Rust mutation must be formatted before verification as well.
  subprocess.run(['cargo','fmt','--all'],check=True)
  cmd=['cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_039','--','--nocapture']
  result=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  print(name, 'exit',result.returncode,flush=True);print(result.stdout,flush=True)
  results.append(dict(name=name,command=cmd,exit_code=result.returncode))
  assert result.returncode==101,(name,'mutation survived')
finally:
 p.write_text(original)
 subprocess.run(['cargo','fmt','--all'],check=True)
Path(__file__).with_name('mutations.json').write_text(json.dumps(results,indent=2)+'\n')
