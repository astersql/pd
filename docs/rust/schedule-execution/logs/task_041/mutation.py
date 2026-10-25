"""Demonstrate that the frozen behavior test rejects representative regressions."""
from pathlib import Path
import subprocess,json
out=Path(__file__).resolve().parent
root=out.parents[4]
source=root/'pkg/schedule/checker/replica_fault.rs'
original=source.read_text()
mutations={
 'threshold':('< context.config.get_max_store_down_time().as_nanos()','<= context.config.get_max_store_down_time().as_nanos()'),
 'learner-guard':('&& region.learners().is_empty()','&& true'),
 'voter-count':('region.voters().len() as i64','region.meta().peers.len() as i64'),
 'priority':('op.set_priority_level(PriorityLevel::HIGH)','op.set_priority_level(PriorityLevel::MEDIUM)'),
 'down-precedence':('if context.config.is_remove_down_replica_enabled() {','if context.config.is_remove_down_replica_enabled() && !context.stores.get_stores().iter().any(|s| !s.is_up()) {'),
}
records=[]
try:
 for name,(a,b) in mutations.items():
  assert original.count(a)==1,(name,a)
  source.write_text(original.replace(a,b))
  subprocess.run(['python3',str(out/'run.py'),'mutation-fmt-'+name,'cargo','fmt','--all'],cwd=root,check=True,stdout=subprocess.DEVNULL)
  p=subprocess.run(['python3',str(out/'run.py'),'mutation-'+name,'cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_041','--','--nocapture'],cwd=root,stdout=subprocess.DEVNULL)
  log=(out/f'mutation-{name}.log').read_text()
  record=dict(mutation=name,exit_code=p.returncode,assertion_failure='panicked at pkg/schedule/checker/task_041_test.rs' in log and 'FAILED' in log)
  records.append(record)
  assert p.returncode==101 and record['assertion_failure'],record
finally:
 source.write_text(original)
 subprocess.run(['python3',str(out/'run.py'),'restore-fmt','cargo','fmt','--all'],cwd=root,check=True,stdout=subprocess.DEVNULL)
 (out/'mutations.json').write_text(json.dumps(records,indent=2)+'\n')
print(json.dumps(records))
