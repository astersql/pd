"""Run final checks in the caller's atomically acquired repository Cargo slot."""
import json,os,re,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_007');rows=[]
commands=[['cargo','fmt','--all'],['cargo','test','-p','astersql-pd-schedule-filter','--locked','--lib','task_007','--','--nocapture'],['cargo','test','-p','astersql-pd-schedule-filter','--locked'],['cargo','metadata','--format-version','1','--no-deps'],['cargo','fmt','--all','--','--check'],['cargo','check','--workspace','--locked'],['cargo','test','--workspace','--locked']]
for i,cmd in enumerate(commands):
 p=subprocess.run(cmd,capture_output=True,text=True);raw=p.stdout+p.stderr;(out/f'validation-{i}.out').write_text(raw);counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',raw)
 row=dict(argv=cmd,exit_code=p.returncode,passed=sum(int(m[0]) for m in counts),failed=sum(int(m[1]) for m in counts),ignored=sum(int(m[2]) for m in counts),cargo_target_dir=os.environ['CARGO_TARGET_DIR']);rows.append(row);(out/'validation.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(row),flush=True)
 if p.returncode:raise SystemExit(p.returncode)
