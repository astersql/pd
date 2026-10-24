import subprocess, json, re
from pathlib import Path
p=Path(__file__).parent
commands=[['cargo','fmt','--all'],['cargo','test','-p','astersql-pd-cluster','--locked','--lib','state_task_41_store_heartbeat','--','--nocapture'],['cargo','test','-p','astersql-pd-cluster','--locked'],['cargo','metadata','--format-version','1','--no-deps'],['cargo','fmt','--all','--','--check'],['cargo','check','--workspace','--locked'],['cargo','test','--workspace','--locked']]
summary=[]
for i,command in enumerate(commands):
 print('RUN '+' '.join(command),flush=True)
 with (p/f'validation-{i}.out').open('w') as output:
  result=subprocess.run(command,stdout=output,stderr=subprocess.STDOUT,text=True)
 result.stdout=(p/f'validation-{i}.out').read_text()
 if result.stdout: (p/f'validation-{i}.out').write_text(result.stdout.rstrip()+'\n')
 counts=[tuple(map(int,x)) for x in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',result.stdout)]
 item=dict(command=command,exit=result.returncode,passed=sum(x[0] for x in counts),failed=sum(x[1] for x in counts),ignored=sum(x[2] for x in counts))
 summary.append(item); print(item,flush=True)
 (p/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 if result.returncode: raise SystemExit(result.returncode)
