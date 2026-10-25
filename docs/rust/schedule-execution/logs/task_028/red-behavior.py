"""Behavior mutations; always restore production bytes, preserving the same tests."""
from pathlib import Path
import subprocess,json,hashlib,os
out=Path('docs/rust/schedule-execution/logs/task_028')
leader=Path('pkg/schedule/operator/builder_leader.rs');target=Path('pkg/schedule/operator/builder_target.rs')
original={p:p.read_bytes() for p in [leader,target]}
variants=[('origin-leader-regression',target,'peer.store_id == current_leader','peer.store_id == self.origin_leader'),('reverse-count',leader,'ac < bc','ac > bc'),('ignore-nearest',leader,'            nearest,','            0,'),('reverse-first-tie',leader,'self.rank(&next) > self.rank(b)','self.rank(&next) >= self.rank(b)')]
results=[]
try:
 for name,path,before,after in variants:
  for p,data in original.items():p.write_bytes(data)
  text=path.read_text();assert before in text;path.write_text(text.replace(before,after,1))
  fmt=subprocess.run(['cargo','fmt','--all'],capture_output=True,text=True);assert fmt.returncode==0
  argv=['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_028','--','--nocapture']
  p=subprocess.run(argv,capture_output=True,text=True);log=p.stdout+p.stderr;(out/(name+'.out')).write_text(log)
  assert p.returncode==101 and 'test result: FAILED' in log and 'panicked at' in log,log
  results.append(dict(name=name,argv=argv,exit_code=p.returncode,behavior_failure=True,cargo_target_dir=os.environ['CARGO_TARGET_DIR']))
  print(name,p.returncode,flush=True)
finally:
 for p,data in original.items():p.write_bytes(data)
 (out/'red.json').write_text(json.dumps(dict(results=results,production_sha256={str(p):hashlib.sha256(data).hexdigest() for p,data in original.items()}),indent=2)+'\n')
