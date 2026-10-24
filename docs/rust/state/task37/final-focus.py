import json, re, subprocess
from pathlib import Path
p=Path(__file__).parent
summary=[]
commands=[['cargo','fmt','--all'],['cargo','test','-p','astersql-pd-syncer','--locked','--lib','state_task_37_client_apply','--','--nocapture'],['cargo','test','-p','astersql-pd-syncer','--locked'],['cargo','fmt','--all','--','--check']]
for i, command in enumerate(commands):
    with (p/f'final-{i}.out').open('w') as output:
        result=subprocess.run(command,stdout=output,stderr=subprocess.STDOUT)
    data=(p/f'final-{i}.out').read_text()
    counts=[tuple(map(int,x)) for x in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',data)]
    item=dict(command=command,exit=result.returncode,passed=sum(x[0] for x in counts),failed=sum(x[1] for x in counts),ignored=sum(x[2] for x in counts))
    summary.append(item)
    print(item,flush=True)
    (p/'final-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    if result.returncode: raise SystemExit(result.returncode)
