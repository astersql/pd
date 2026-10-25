"""Verify the same behavior tests reject regressions; restore before leaving."""
from pathlib import Path
import subprocess,json,sys
out=Path(__file__).resolve().parent
p=Path('pkg/schedule/checker/replica_strategy.rs');original=p.read_text()
mutations=[('allow_suboptimal_temporary_target','let relaxed = self.state(true, false);','let relaxed = self.state(false, false);'),('ignore_temporary_source','if source_temp {','if false && source_temp {'),('witness_wrong_version','(major, minor) >= (6, 6)','(major, minor) >= (6, 5)'),('ignore_comparison_engine','s.label_value(ENGINE_KEY) == source.label_value(ENGINE_KEY)','s.label_value(ENGINE_KEY) == s.label_value(ENGINE_KEY)')]
results=[]
try:
 for name,before,after in mutations:
  assert before in original
  p.write_text(original.replace(before,after,1))
  fmt=subprocess.run(['cargo','fmt','--all']);assert fmt.returncode==0
  cmd=['cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_040','--','--nocapture']
  result=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  (out/f'mutation-{name}.log').write_text(result.stdout)
  results.append(dict(name=name,command=cmd,exit_code=result.returncode))
  assert result.returncode==101, name
finally:
 p.write_text(original)
 subprocess.run(['cargo','fmt','--all'],check=True)
 (out/'mutations.json').write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results,indent=2))
