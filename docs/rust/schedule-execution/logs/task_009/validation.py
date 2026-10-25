"""Required verification; inherits the atomically claimed Cargo slot."""
import json,os,re,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_009');rows=[]
commands=[['cargo','fmt','--all'],['cargo','test','-p','astersql-pd-schedule-filter','--locked','--lib','task_009','--','--nocapture'],['cargo','test','-p','astersql-pd-schedule-filter','--locked'],['cargo','metadata','--format-version','1','--no-deps'],['cargo','fmt','--all','--','--check'],['cargo','check','--workspace','--locked'],['cargo','test','--workspace','--locked']]
for i,argv in enumerate(commands):
 p=subprocess.run(argv,capture_output=True,text=True);raw=p.stdout+p.stderr;(out/f'validation-{i}.out').write_text(raw.rstrip()+'\n');row=dict(argv=argv,exit_code=p.returncode,passed=sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',raw))),failed=sum(map(int,re.findall(r'; (\d+) failed;',raw))),ignored=sum(map(int,re.findall(r'; (\d+) ignored;',raw))),cargo_target_dir=os.environ['CARGO_TARGET_DIR'],stdout=p.stdout,stderr=p.stderr);rows.append(row);(out/'validation.json').write_text(json.dumps(rows,indent=2)+'\n');print(i,argv,'exit',p.returncode,'passed',row['passed'],'ignored',row['ignored'],flush=True);assert p.returncode==0
