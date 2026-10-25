"""Recheck the restored implementation after the deliberately failing mutation."""
from pathlib import Path
import subprocess,json,os,re
out=Path('docs/rust/schedule-execution/logs/task_020');results=[]
for name,argv in [('focused',['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_020','--','--nocapture']),('package',['cargo','test','-p','astersql-pd-schedule-operator','--locked']),('fmt',['cargo','fmt','--all','--','--check'])]:
 p=subprocess.run(argv,capture_output=True,text=True);log=p.stdout+p.stderr;(out/f'after-red-{name}.out').write_text(log)
 counts=[tuple(map(int,m)) for m in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',log)]
 results.append(dict(argv=argv,exit_code=p.returncode,passed=sum(c[0] for c in counts),failed=sum(c[1] for c in counts),cargo_target_dir=os.environ['CARGO_TARGET_DIR']))
 (out/'after-red.json').write_text(json.dumps(results,indent=2)+'\n');print(results[-1],flush=True)
 if p.returncode:raise SystemExit(p.returncode)
