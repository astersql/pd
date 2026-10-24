import json,re,subprocess
from pathlib import Path
p=Path(__file__).parent
summary=json.loads((p/'summary.json').read_text())
assert len(summary)==7 and all(x['exit']==0 for x in summary), 'finish baseline first'
(p/'focus-zero.json').write_text(json.dumps(summary[1],indent=2)+'\n')
(p/'validation-1-zero.out').write_text((p/'validation-1.out').read_text())
command=['cargo','test','-p','astersql-pd-cluster','--locked','--lib','state_task_46_heartbeat_effects','--','--nocapture']
with (p/'validation-1.out').open('w') as output:
    result=subprocess.run(command,stdout=output,stderr=subprocess.STDOUT)
counts=[tuple(map(int,x)) for x in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',(p/'validation-1.out').read_text())]
item=dict(command=command,exit=result.returncode,passed=sum(x[0] for x in counts),failed=sum(x[1] for x in counts),ignored=sum(x[2] for x in counts))
assert result.returncode==0 and item['passed']>0 and item['failed']==0 and item['ignored']==0,item
summary[1]=item
(p/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(item)
