"""Demonstrate the registered behavior tests reject wrong production behavior."""
import os,json,re,subprocess,signal
from pathlib import Path
p=Path('pkg/schedule/filter/region_health.rs');original=p.read_text();out=Path('docs/rust/schedule-execution/logs/task_010');rows=[]
mutations=[
 ('pending_healthy','is_region_healthy_allow_pending(region) && region.pending_peers().is_empty()','is_region_healthy_allow_pending(region)'),
 ('down_healthy','region.down_peers().is_empty()','true'),
 ('wrong_replica_count','region.meta().peers.len() as i64 == conf.get_max_replicas()','region.meta().peers.len() as i64 >= conf.get_max_replicas()'),
 ('skip_rule_rejection','if !fit.is_satisfied() {','if false {'),
 ('erase_last_fit','return Status::new(StatusCode::REGION_NOT_MATCH_RULE, &[]);','*self.fit.borrow_mut() = None; return Status::new(StatusCode::REGION_NOT_MATCH_RULE, &[]);'),
 ('empty_threshold','self.total_region_count < 100','self.total_region_count <= 100'),
 ('witness_wrong_store','region.get_store_witness(self.store_id).is_none()','region.witnesses().is_empty()'),
 ('ignore_busy',' && !store.is_busy()',''),
 ('affinity_gate','group.regular_scheduling_allowed','true'),
 ('select_reversed','regions\n        .iter()','regions\n        .iter().rev()'),
]
def interrupted(signum, _frame):
 raise SystemExit(128+signum)
signal.signal(signal.SIGTERM,interrupted)
signal.signal(signal.SIGINT,interrupted)
try:
 for name,old,new in mutations:
  assert old in original,name;p.write_text(original.replace(old,new,1));subprocess.run(['cargo','fmt','--all'],check=True,capture_output=True,text=True)
  argv=['cargo','test','-p','astersql-pd-schedule-filter','--locked','--lib','task_010','--','--nocapture'];r=subprocess.run(argv,capture_output=True,text=True);failed=sum(map(int,re.findall(r'test result: FAILED\. \d+ passed; (\d+) failed;',r.stdout)))
  rows.append(dict(name=name,argv=argv,cargo_target_dir=os.environ['CARGO_TARGET_DIR'],exit_code=r.returncode,failed=failed,stdout=r.stdout,stderr=r.stderr));print(name,r.returncode,failed,flush=True)
  assert r.returncode==101 and failed>0,name
finally:
 p.write_text(original);(out/'mutation.json').write_text(json.dumps(rows,indent=2)+'\n')
