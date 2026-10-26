# Called by run.py; all Cargo invocations inherit its owned absolute target slot.
from pathlib import Path
import subprocess,json,re,sys,os
logs=Path('docs/rust/schedule-execution/logs/task_110')
commands=[['cargo','fmt','--all'],['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_110','--','--nocapture'],['cargo','test','-p','astersql-pd-schedule-schedulers','--locked'],['cargo','metadata','--format-version','1','--no-deps'],['cargo','fmt','--all','--','--check'],['cargo','check','--workspace','--locked'],['cargo','test','--workspace','--locked']]
records=[]
for i,cmd in enumerate(commands):
 with (logs/f'final-{i}.log').open('w') as output:
  result=subprocess.run(cmd,stdout=output,stderr=subprocess.STDOUT)
 text=(logs/f'final-{i}.log').read_text()
 counts=re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored',text)
 record=dict(command=cmd,exit_code=result.returncode,passed=sum(int(x[0]) for x in counts),failed=sum(int(x[1]) for x in counts),ignored=sum(int(x[2]) for x in counts),CARGO_TARGET_DIR=os.environ['CARGO_TARGET_DIR'])
 records.append(record);(logs/'commands.json').write_text(json.dumps(records,indent=2)+'\n');print(json.dumps(record),flush=True)
 if result.returncode:sys.exit(result.returncode)
