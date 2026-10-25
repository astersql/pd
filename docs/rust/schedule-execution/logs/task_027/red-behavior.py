"""Prove fixture assertions reject bad behavior; always restore production bytes."""
from pathlib import Path
import subprocess,json,hashlib,os
out=Path('docs/rust/schedule-execution/logs/task_027')
p=Path('pkg/schedule/operator/builder_simple.rs');original=p.read_bytes();s=original.decode();results=[]
mutations=[('leader-transfer','if to != 0 && to != self.leader {','if false && to != 0 && to != self.leader {'),('promotion','if !n.is_learner() {','if false && !n.is_learner() {'),('light-peer','is_light_weight: options.add_light_peer,','is_light_weight: false,'),('snapshot-source','send_store: self.origin_leader,','send_store: 0,')]
try:
 for name,before,after in mutations:
  assert before in s,name
  p.write_text(s.replace(before,after,1))
  fmt=subprocess.run(['cargo','fmt','--all'],capture_output=True,text=True)
  (out/f'red-{name}-fmt.out').write_text(fmt.stdout+fmt.stderr)
  assert fmt.returncode==0
  argv=['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_027_behavior_cases','--','--nocapture']
  r=subprocess.run(argv,capture_output=True,text=True);log=r.stdout+r.stderr;(out/f'red-{name}.out').write_text(log)
  assert r.returncode==101 and 'panicked at' in log and 'test result: FAILED' in log,(name,log)
  results.append(dict(mutation=name,argv=argv,exit_code=r.returncode,behavior_assertion_failed=True,cargo_target_dir=os.environ['CARGO_TARGET_DIR']))
  p.write_bytes(original)
finally:
 p.write_bytes(original)
 (out/'red.json').write_text(json.dumps(dict(production_sha256=hashlib.sha256(original).hexdigest(),results=results),indent=2)+'\n')
