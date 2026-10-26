from pathlib import Path
import subprocess,json
p=Path('pkg/schedule/schedulers/scheduler_control.rs')
original=p.read_text()
results=[]
try:
 for name,before,after in [('ttl_boundary','now < self.delay_until','now <= self.delay_until'),('reload_delay','let storage = &self.context.storage;','self.schedulers.get_mut(name).unwrap().delay_until = 0;\n        let storage = &self.context.storage;')]:
  # The second mutation must occur before borrowing the live scheduler.
  if name=='reload_delay':
   before='let s = &mut self\n            .schedulers'
   after='self.schedulers.get_mut(name).unwrap().delay_until = 0;\n        '+before
  assert before in original,name
  p.write_text(original.replace(before,after,1))
  subprocess.run(['cargo','fmt','--all'],check=True)
  r=subprocess.run(['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_107','--','--nocapture'],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  Path(f'docs/rust/schedule-execution/logs/task_107/red-{name}.log').write_text(r.stdout)
  results.append(dict(mutation=name,exit_code=r.returncode,assertion_failed='assertion' in r.stdout and 'FAILED' in r.stdout))
  assert r.returncode==101 and results[-1]['assertion_failed'],r.stdout
finally:
 p.write_text(original)
 Path('docs/rust/schedule-execution/logs/task_107/red-results.json').write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results))
