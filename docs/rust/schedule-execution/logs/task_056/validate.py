"""Run final checks in individually acquired shared Cargo slots."""
from pathlib import Path
import subprocess,os
out=Path(__file__).resolve().parent
commands=[('fmt-final',['cargo','fmt','--all']),('focused-final',['cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_056','--','--nocapture']),('package-final',['cargo','test','-p','astersql-pd-schedule-checker','--locked']),('metadata',['cargo','metadata','--format-version','1','--no-deps']),('fmt-check',['cargo','fmt','--all','--','--check']),('workspace-check',['cargo','check','--workspace','--locked']),('workspace-test',['cargo','test','--workspace','--locked'])]
for name,command in commands:
    env=dict(os.environ)
    if name=='workspace-test':env['RUST_TEST_THREADS']='2'
    subprocess.run(['python3',str(out/'run.py'),name,*command],env=env,check=True)
