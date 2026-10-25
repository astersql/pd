from pathlib import Path
import subprocess,json
base=Path('docs/rust/schedule-execution/logs/task_002')
commands=[('fmt-final',['cargo','fmt','--all']),('green',['cargo','test','-p','astersql-pd-schedule-core','--locked','--lib','task_002','--','--nocapture']),('crate',['cargo','test','-p','astersql-pd-schedule-core','--locked']),('metadata',['cargo','metadata','--format-version','1','--no-deps']),('fmt-check',['cargo','fmt','--all','--','--check']),('workspace-check',['cargo','check','--workspace','--locked']),('workspace-test',['cargo','test','--workspace','--locked'])]
results=[]
# One lease for this entire synchronous validation phase.
for name,cmd in commands:
 with (base/(name+'.out')).open('w') as log:
  result=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
 results.append({'command':cmd,'exit_code':result.returncode,'log':name+'.out'})
 (base/'validation.json').write_text(json.dumps(results,indent=2)+'\n')
 print(name,result.returncode,flush=True)
 if result.returncode: raise SystemExit(result.returncode)
