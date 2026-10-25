"""Run the required checks in one owned slot; preserve exact commands and statuses."""
import subprocess,json,os,sys
from pathlib import Path
out=Path(__file__).resolve().parent
commands=[['cargo','fmt','--all'],['cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_039','--','--nocapture'],['cargo','test','-p','astersql-pd-schedule-checker','--locked'],['cargo','metadata','--format-version','1','--no-deps'],['cargo','fmt','--all','--','--check'],['cargo','check','--workspace','--locked'],['cargo','test','--workspace','--locked']]
results=[]
for cmd in commands:
    print('COMMAND',json.dumps(cmd),flush=True)
    result=subprocess.run(cmd)
    results.append({'command':cmd,'exit_code':result.returncode,'CARGO_TARGET_DIR':os.environ['CARGO_TARGET_DIR'],'RUST_TEST_THREADS':os.environ.get('RUST_TEST_THREADS')})
    (out/'commands.json').write_text(json.dumps(results,indent=2)+'\n')
    if result.returncode:sys.exit(result.returncode)
