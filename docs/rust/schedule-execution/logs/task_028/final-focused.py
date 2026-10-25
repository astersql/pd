"""Recheck the final test source after removing an unreachable task027-only branch."""
import subprocess,json,os,re
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_028');rows=[]
commands=[['cargo','fmt','--all'],['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_028','--','--nocapture'],['cargo','test','-p','astersql-pd-schedule-operator','--locked'],['cargo','fmt','--all','--','--check']]
for i,argv in enumerate(commands):
 p=subprocess.run(argv,capture_output=True,text=True);log=p.stdout+p.stderr;(out/f'final-focused-{i}.out').write_text(log)
 counts=[tuple(map(int,m)) for m in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',log)]
 row=dict(argv=argv,exit_code=p.returncode,passed=sum(c[0] for c in counts),failed=sum(c[1] for c in counts),ignored=sum(c[2] for c in counts),cargo_target_dir=os.environ['CARGO_TARGET_DIR']);rows.append(row);print(row,flush=True)
(out/'final-focused.json').write_text(json.dumps(rows,indent=2)+'\n')
assert all(r['exit_code']==0 for r in rows)
