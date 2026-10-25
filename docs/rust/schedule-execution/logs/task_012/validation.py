"""Run required baselines under the caller's allocated Cargo slot."""
import subprocess,json,os,re
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_012');results=[]
commands=[['cargo','fmt','--all'],['cargo','test','-p','astersql-pd-schedule-hbstream','--locked','--lib','task_012','--','--nocapture'],['cargo','test','-p','astersql-pd-schedule-hbstream','--locked'],['cargo','metadata','--format-version','1','--no-deps'],['cargo','fmt','--all','--','--check'],['cargo','check','--workspace','--locked'],['cargo','test','--workspace','--locked']]
for i,argv in enumerate(commands):
 p=subprocess.run(argv,capture_output=True,text=True);log=p.stdout+p.stderr;(out/f'validation-{i}.out').write_text(log)
 counts=[tuple(map(int,m)) for m in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',log)]
 row=dict(argv=argv,exit_code=p.returncode,passed=sum(c[0] for c in counts),failed=sum(c[1] for c in counts),ignored=sum(c[2] for c in counts),cargo_target_dir=os.environ['CARGO_TARGET_DIR'],log=f'validation-{i}.out');results.append(row)
 (out/'validation.json').write_text(json.dumps(results,indent=2)+'\n');print(row,flush=True)
 if p.returncode:raise SystemExit(p.returncode)
