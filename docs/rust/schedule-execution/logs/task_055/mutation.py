"""Exercise real assertions against independently broken dispatcher branches."""
from pathlib import Path
import subprocess, json, sys
here=Path(__file__).resolve().parent
root=here.parents[4]
source=root/'pkg/schedule/checker/checker_dispatch.rs'
original=source.read_text()
mutations=[
 ('capacity','self.pending.len() > DEFAULT_PENDING_REGION_CACHE_SIZE','self.pending.len() >= DEFAULT_PENDING_REGION_CACHE_SIZE'),
 ('cache','out.rule_cache_hit = conf.is_placement_rules_cache_enabled()','out.rule_cache_hit = false && conf.is_placement_rules_cache_enabled()'),
 ('success','if op.status() == SUCCESS {','if true {'),
]
results=[]
try:
 for name,old,new in mutations:
  assert original.count(old)==1
  source.write_text(original.replace(old,new))
  code=subprocess.call([sys.executable,str(here/'run.py'),'mutation-'+name,'cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_055','--','--nocapture'],cwd=root)
  results.append(dict(mutation=name,exit_code=code))
  assert code==101,results[-1]
finally:
 source.write_text(original)
 (here/'mutation-results.json').write_text(json.dumps(results,indent=2)+'\n')
