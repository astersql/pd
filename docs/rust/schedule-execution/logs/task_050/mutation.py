"""Verify frozen assertions reject incorrect decisions; always restore source."""
from pathlib import Path
import subprocess,json
root=Path(__file__).resolve().parents[5]
out=Path(__file__).resolve().parent
p=root/'pkg/schedule/checker/affinity_merge.rs'
original=p.read_text()
mutations=[('startup','60_000_000_000','0'),('size','> max_size','> i64::MAX'),('target-cache','self.recently_merged(target.meta().id, seconds)','false'),('one-way','if !conf.is_one_way_merge_enabled()','if true'),('kind','OpKind::AFFINITY, allocate','OpKind::MERGE, allocate')]
results=[]
try:
 for name,old,new in mutations:
  assert old in original,name
  p.write_text(original.replace(old,new))
  subprocess.run(['python3',str(out/'run.py'),f'mutation-{name}-fmt','cargo','fmt','--all'],cwd=root,check=True,stdout=subprocess.DEVNULL)
  r=subprocess.run(['python3',str(out/'run.py'),f'mutation-{name}','cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_050','--','--nocapture'],cwd=root,stdout=subprocess.DEVNULL)
  log=(out/f'mutation-{name}.log').read_text()
  results.append(dict(mutation=name,exit_code=r.returncode,behavior_failure='test result: FAILED' in log))
  assert r.returncode==101 and 'test result: FAILED' in log,name
finally:
 p.write_text(original)
 (out/'mutation-results.json').write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results))
