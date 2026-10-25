"""The registered differential test must reject a defect in each filter family."""
import json,re,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_008');p=Path('pkg/schedule/filter/store_constraints.rs');original=p.read_text();rows=[]
variants=[('capacity-bypass','!store.is_low_space(conf.get_low_space_ratio())','true'),('label-bypass','match_label_constraints(Some(store), &self.constraints)','true'),('engine-inverted','self.constraint.match_store(store),','!self.constraint.match_store(store),'),('special-source-no-evacuation','store.is_low_space(conf.get_low_space_ratio()) || !self.constraint.match_store(store)','!self.constraint.match_store(store)'),('isolation-case-sensitive','equal_fold(store.label_value(&self.location_labels[i]), value)','store.label_value(&self.location_labels[i]) == value'),('hot-stopping-bypass','|| store.evicted_as_stopping_store()','|| false')]
for name,old,new in variants:
 try:
  assert old in original,name;p.write_text(original.replace(old,new,1));subprocess.run(['cargo','fmt','--all'],check=True,capture_output=True)
  argv=['cargo','test','-p','astersql-pd-schedule-filter','--locked','--lib','task_008','--','--nocapture']
  r=subprocess.run(argv,capture_output=True,text=True);raw=r.stdout+r.stderr;(out/f'mutation-{name}.out').write_text(raw)
  m=re.search(r'test result: FAILED\. (\d+) passed; (\d+) failed;',raw);assert r.returncode==101 and m and int(m[2])>0,raw
  assert 'task_008_test::task_008_behavior_cases ... FAILED' in raw
  rows.append(dict(name=name,argv=argv,exit_code=r.returncode,passed=int(m[1]),failed=int(m[2])));print(name,'rejected',flush=True)
 finally:p.write_text(original)
(out/'mutation.json').write_text(json.dumps(rows,indent=2)+'\n')
