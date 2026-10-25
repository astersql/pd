"""Verify behavioral assertions reject independent faults, restoring source."""
from pathlib import Path
import subprocess,json,os
out=Path(__file__).resolve().parent
root=out.parents[4]
p=root/'pkg/schedule/checker/merge_decision.rs'
original=p.read_text()
changes=[
 ('startup','now < self.start_time.saturating_add(interval as i128)','false'),
 ('source-split','self.split_cache.contains_key(&region.meta().id)','false'),
 ('target-split','!self.split_cache.contains_key(&adjacent.meta().id)','true'),
 ('one-way','!conf.is_one_way_merge_enabled()','true'),
 ('target-order','prev.unwrap().stats().approximate_size < target.stats().approximate_size','prev.unwrap().stats().approximate_size > target.stats().approximate_size'),
 ('source-health','!is_region_healthy(region)','false'),
 ('ttl-boundary','*deadline >= now','*deadline > now'),
 ('config-reset','if interval != self.interval','if false'),
 ('target-cap','target.stats().approximate_size > cap','false'),
]
results=[]
try:
 for name,old,new in changes:
  assert original.count(old)==1,(name,original.count(old))
  p.write_text(original.replace(old,new))
  subprocess.run(['cargo','fmt','--all'],cwd=root,check=True,stdout=subprocess.DEVNULL)
  cmd=['cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_047','--','--nocapture']
  r=subprocess.run(cmd,cwd=root,capture_output=True,text=True)
  log=r.stdout+r.stderr
  (out/f'mutation-{name}.log').write_text(log)
  assert r.returncode==101 and 'test result: FAILED' in log and 'panicked at' in log,name
  results.append(dict(mutation=name,command=cmd,exit_code=r.returncode,behavior_assertion_failed=True))
  (out/'mutation-results.json').write_text(json.dumps(results,indent=2)+'\n')
 print(f'{len(results)} behavioral mutations rejected')
finally:
 p.write_text(original)
 subprocess.run(['cargo','fmt','--all'],cwd=root,check=True)
