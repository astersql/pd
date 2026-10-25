"""Run the required baseline inside the caller's shared Cargo slot."""
from pathlib import Path
import subprocess,os,json,re,signal
out=Path(__file__).resolve().parent
root=out.parents[4]
assert os.environ['CARGO_TARGET_DIR'].startswith(str(root/'target/rust-slot-'))
commands=[
 ['cargo','fmt','--all'],
 ['cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_046','--','--nocapture'],
 ['cargo','test','-p','astersql-pd-schedule-checker','--locked'],
 ['cargo','metadata','--format-version','1','--no-deps'],
 ['cargo','fmt','--all','--','--check'],
 ['cargo','check','--workspace','--locked'],
 ['cargo','test','--workspace','--locked'],
]
records=[]
for i,cmd in enumerate(commands):
 with (out/f'validation-{i}.log').open('w') as log:
  # Same process group as the slot runner: interrupted stages wait for all children.
  p=subprocess.Popen(cmd,cwd=root,stdout=log,stderr=subprocess.STDOUT)
  try:code=p.wait()
  except BaseException:
   p.terminate()
   try:p.wait(timeout=10)
   except subprocess.TimeoutExpired:p.kill();p.wait()
   raise
 text=(out/f'validation-{i}.log').read_text()
 counts=[tuple(map(int,m)) for m in re.findall(r'test result: .*? (\d+) passed; (\d+) failed; (\d+) ignored;',text)]
 record=dict(command=cmd,exit_code=code,CARGO_TARGET_DIR=os.environ['CARGO_TARGET_DIR'],RUST_TEST_THREADS=os.environ.get('RUST_TEST_THREADS'),passed=sum(t[0] for t in counts),failed=sum(t[1] for t in counts),ignored=sum(t[2] for t in counts))
 records.append(record)
 (out/'commands.json').write_text(json.dumps(records,indent=2)+'\n')
 print(json.dumps(record),flush=True)
 if code:raise SystemExit(code)
