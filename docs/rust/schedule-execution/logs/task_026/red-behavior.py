"""Prove frozen assertions reject four independent wrong target policies."""
import hashlib,json,os,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_026')
p=Path('pkg/schedule/operator/builder_target.rs');original=p.read_bytes();s=original.decode();results=[]
mutations=[
 ('duplicate-accepted','Some(p) if self.target_peers.contains_key(&p.store_id) =>','Some(p) if false =>'),
 ('force-bypasses-missing-store','let Some(store) = self.context.stores.get_store(peer.store_id) else {','if ignore_cluster_limit { return true; }\n        let Some(store) = self.context.stores.get_store(peer.store_id) else {'),
 ('placement-unchecked','&& match_label_constraints(Some(&store), &r.label_constraints)','&& true'),
 ('all-leaders-accepted','if matches!(peer.role, PeerRole::Learner | PeerRole::DemotingVoter) || peer.is_witness','if false'),
]
try:
 for name,old,new in mutations:
  assert s.count(old)==1,(name,s.count(old))
  p.write_text(s.replace(old,new))
  subprocess.run(['cargo','fmt','--all'],check=True)
  argv=['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_026','--','--nocapture']
  r=subprocess.run(argv,capture_output=True,text=True)
  (out/f'red-{name}.out').write_text(r.stdout+r.stderr)
  assert r.returncode==101 and 'assertion' in r.stdout+r.stderr and 'FAILED' in r.stdout,(name,r.returncode,r.stdout,r.stderr)
  results.append(dict(mutation=name,argv=argv,exit_code=r.returncode,cargo_target_dir=os.environ['CARGO_TARGET_DIR']))
finally:
 p.write_bytes(original)
 subprocess.run(['cargo','fmt','--all'],check=True)
 (out/'red.json').write_text(json.dumps(dict(results=results,restored_sha256=hashlib.sha256(p.read_bytes()).hexdigest()),indent=2)+'\n')
