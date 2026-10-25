"""Run the required checks inside a slot already held by slot-run.py."""
import json,os,re,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_008');rows=[]
commands=[['cargo','fmt','--all'],['cargo','test','-p','astersql-pd-schedule-filter','--locked','--lib','task_008','--','--nocapture'],['cargo','test','-p','astersql-pd-schedule-filter','--locked'],['cargo','metadata','--format-version','1','--no-deps'],['cargo','fmt','--all','--','--check'],['cargo','check','--workspace','--locked'],['cargo','test','--workspace','--locked']]
for i,argv in enumerate(commands):
 with (out/f'validation-{i}.out').open('w') as f:p=subprocess.run(argv,stdout=f,stderr=subprocess.STDOUT)
 raw=(out/f'validation-{i}.out').read_text()
 if raw.endswith('\n\n'):
  rp=out/'validation-raw.json';saved=json.loads(rp.read_text()) if rp.exists() else {};saved[f'validation-{i}.out']=raw;rp.write_text(json.dumps(saved,ensure_ascii=False,indent=2)+'\n');(out/f'validation-{i}.out').write_text(raw.rstrip()+'\n')
 counts=re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;',raw)
 totals=[sum(int(m[j]) for m in counts) for j in range(3)]
 row=dict(argv=argv,exit_code=p.returncode,passed=totals[0],failed=totals[1],ignored=totals[2],cargo_target_dir=os.environ['CARGO_TARGET_DIR']);rows.append(row);(out/'validation.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(row),flush=True)
 if p.returncode:raise SystemExit(p.returncode)
