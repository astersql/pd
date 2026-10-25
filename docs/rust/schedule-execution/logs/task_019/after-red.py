"""Verify the restored implementation after the negative control."""
from pathlib import Path
import subprocess,json,os,re
out=Path('docs/rust/schedule-execution/logs/task_019');results=[]
for argv in [['cargo','fmt','--all','--','--check'],['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_019','--','--nocapture']]:
 p=subprocess.run(argv,capture_output=True,text=True);log=p.stdout+p.stderr
 name='after-red-'+('test' if 'test' in argv else 'fmt')+'.out';(out/name).write_text(log)
 results.append(dict(argv=argv,exit_code=p.returncode,log=name,cargo_target_dir=os.environ['CARGO_TARGET_DIR']))
 assert p.returncode==0,log
 if 'test' in argv:assert '4 passed; 0 failed' in log
(out/'after-red.json').write_text(json.dumps(results,indent=2)+'\n')
