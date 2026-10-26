from pathlib import Path
import subprocess,json,sys,re
logs=Path('docs/rust/schedule-execution/logs/task_109')
commands=[['cargo','fmt','--all'],['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_109','--','--nocapture'],['cargo','test','-p','astersql-pd-schedule-schedulers','--locked'],['cargo','metadata','--format-version','1','--no-deps'],['cargo','fmt','--all','--','--check'],['cargo','check','--workspace','--locked'],['cargo','test','--workspace','--locked']]
results=[]
for i,cmd in enumerate(commands):
 print('COMMAND',cmd,flush=True)
 with (logs/f'final-{i}.log').open('w') as out:
  rc=subprocess.call(cmd,stdout=out,stderr=subprocess.STDOUT)
 text=(logs/f'final-{i}.log').read_text()
 matches=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',text)
 result=dict(command=cmd,exit_code=rc,passed=sum(int(a) for a,b,c in matches),failed=sum(int(b) for a,b,c in matches),ignored=sum(int(c) for a,b,c in matches))
 results.append(result)
 (logs/'commands.json').write_text(json.dumps(results,indent=2)+'\n')
 print(json.dumps(result),flush=True)
 if rc: sys.exit(rc)
