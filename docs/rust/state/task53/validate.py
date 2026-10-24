import subprocess,json,re
from pathlib import Path
p=Path(__file__).parent
commands=[['cargo','fmt','--all'],['cargo','test','-p','astersql-pd-syncutil','--locked','--lib','ordered_single_flight_test::singleflight_randomized','--','--nocapture'],['cargo','test','-p','astersql-pd-compat-tests','--locked','--lib','state_theme_','--','--nocapture'],['cargo','test','-p','astersql-pd-compat-tests','--locked'],['cargo','metadata','--format-version','1','--no-deps'],['cargo','fmt','--all','--','--check'],['cargo','check','--workspace','--locked'],['cargo','test','--workspace','--locked']]
summary=[]
for i,cmd in enumerate(commands):
 print('RUN '+' '.join(cmd),flush=True)
 with (p/f'validation-{i}.out').open('w') as f:r=subprocess.run(cmd,stdout=f,stderr=subprocess.STDOUT)
 output=(p/f'validation-{i}.out').read_text();counts=[tuple(map(int,m)) for m in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',output)]
 item=dict(command=cmd,exit=r.returncode,passed=sum(c[0] for c in counts),failed=sum(c[1] for c in counts),ignored=sum(c[2] for c in counts));summary.append(item);print(item,flush=True)
 (p/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 if r.returncode:raise SystemExit(r.returncode)
 if 'state_theme_' in cmd and item['passed']==0:raise SystemExit('zero focused tests')
