"""Verify behavior assertions detect incorrect guards; always restore production."""
from pathlib import Path
import subprocess,json,os,signal
def interrupted(signum, frame):
    raise KeyboardInterrupt(f"signal {signum}")
signal.signal(signal.SIGTERM, interrupted)
out=Path(__file__).resolve().parent
root=out.parents[4]
assert os.environ['CARGO_TARGET_DIR'].startswith(str(root/'target/rust-slot-'))
p=root/'pkg/schedule/checker/merge_guard.rs'; original=p.read_text()
mutations=[
 ('empty-seam','!a.end_key.is_empty() && a.end_key == b.start_key','a.end_key == b.start_key'),
 ('table-identity','table_identity(&a.start_key).unwrap_or_default()\n                == table_identity(&b.start_key).unwrap_or_default()','table_identity(&a.start_key).unwrap_or_default().table_id\n                == table_identity(&b.start_key).unwrap_or_default().table_id'),
 ('placement','if conf.is_placement_rules_enabled()','if false && conf.is_placement_rules_enabled()'),
 ('label-split','!labels.index().get_split_keys(start, end).is_empty()','false'),
 ('label-deny','== "deny"','== "DENY"'),
 ('peer-store','&& check_peer_store(stores, region, adjacent)','&& (true || check_peer_store(stores, region, adjacent))'),
 ('health','&& is_region_healthy(adjacent)','&& (true || is_region_healthy(adjacent))'),
 ('replication','&& is_region_replicated(context, adjacent)','&& (true || is_region_replicated(context, adjacent))'),
]
records=[]
try:
 for name,old,new in mutations:
  assert old in original,name
  p.write_text(original.replace(old,new))
  subprocess.run(['cargo','fmt','--all'],cwd=root,check=True)
  cmd=['cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_046','--','--nocapture']
  with (out/f'mutation-{name}.log').open('w') as log:
   r=subprocess.run(cmd,cwd=root,stdout=log,stderr=subprocess.STDOUT)
  t=(out/f'mutation-{name}.log').read_text()
  record=dict(name=name,command=cmd,exit_code=r.returncode,assertion_failure='assertion `left == right` failed' in t)
  records.append(record);(out/'mutation-results.json').write_text(json.dumps(records,indent=2)+'\n')
  print(record,flush=True)
  assert r.returncode==101 and record['assertion_failure'],record
finally:
 p.write_text(original)
 subprocess.run(['cargo','fmt','--all'],cwd=root,check=True)
