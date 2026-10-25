"""Sequential behavior mutations, restoring production source even on failure."""
from pathlib import Path
import subprocess,json,signal
root=Path(__file__).resolve().parents[5]
out=Path(__file__).resolve().parent
source=root/'pkg/schedule/checker/affinity_peer.rs'
original=source.read_text()
mutations=[
 ('pause','self.pause.is_paused_at(now as i64)','false'),
 ('learner','roles.insert(learner.store_id, PeerRoleType::Learner);','// mutation: omit preserved learner'),
 ('normalize','manager.expire_affinity_group(&group.group.id, now);','// mutation: omit expiration'),
 ('location','if !best_location(ctx, rules, region, true).await {','if false {'),
 ('leader-evict','if !store.allow_leader_transfer_in()','if false'),
 ('ttl-boundary','now <= *expiry','now < *expiry'),
 ('success-cache','self.recent_merges.insert(related, now.saturating_add(60));','// mutation: omit related ID'),
]
records=[]
def interrupted(signum,frame):raise KeyboardInterrupt(signum)
signal.signal(signal.SIGTERM,interrupted)
try:
 for name,old,new in mutations:
  assert old in original,name
  source.write_text(original.replace(old,new,1))
  fmt=subprocess.run(['python3',str(out/'run.py'),'mutation-'+name+'-fmt','cargo','fmt','--all'],cwd=root,capture_output=True,text=True)
  assert fmt.returncode==0,fmt.stdout+fmt.stderr
  p=subprocess.run(['python3',str(out/'run.py'),'mutation-'+name,'cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_049','--','--nocapture'],cwd=root,capture_output=True,text=True)
  log=(out/('mutation-'+name+'.log')).read_text()
  record=dict(mutation=name,exit_code=p.returncode,assertion_failure='panicked at' in log,compile_error='error[E' in log)
  records.append(record);print(json.dumps(record),flush=True)
  assert p.returncode==101 and record['assertion_failure'] and not record['compile_error'],p.stdout+p.stderr
finally:
 source.write_text(original)
 (out/'mutation-results.json').write_text(json.dumps(records,indent=2)+'\n')
