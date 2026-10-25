"""Prove the same registered behavior assertions reject independently wrong implementations."""
from pathlib import Path
import subprocess,json,os,re
out=Path('docs/rust/schedule-execution/logs/task_011');path=Path('pkg/schedule/filter/filter_metrics.rs');original=path.read_text();results=[]
changes=[
 ('repeat_flush','self.pending.drain()','self.pending.clone()'),
 ('scope_ignored','self.scope = scope.into();','let _ = scope.into();'),
 ('removed_resurrection','c.get_store(id).is_none_or(|s| s.is_removed())','c.get_store(id).is_none()'),
 ('missing_counted','c.get_store(id).is_none_or(|s| s.is_removed())','c.get_store(id).is_some_and(|s| s.is_removed())'),
 ('target_cleanup_skipped','if id.to_string() != store_id {','if id.to_string() != store_id || *action == FilterAction::Target {'),
 ('scope_cleanup_skipped','if id.to_string() != store_id {','if id.to_string() != store_id || scope == "renamed" {'),
 ('source_reset_skipped','self.inner.source.reset();',''),
 ('reset_stale_handles','series.clear();',''),
 ('registration_leak','registry.unregister(Box::new(source))?;',''),
 ('direction_swapped','FilterAction::Target => &metrics.inner.target,','FilterAction::Target => &metrics.inner.source,'),
]
try:
 for name,old,new in changes:
  assert original.count(old)==1,(name,original.count(old))
  path.write_text(original.replace(old,new))
  f=subprocess.run(['cargo','fmt','--all'],capture_output=True,text=True);assert f.returncode==0
  argv=['cargo','test','-p','astersql-pd-schedule-filter','--locked','--lib','task_011','--','--nocapture']
  p=subprocess.run(argv,capture_output=True,text=True);log=p.stdout+p.stderr;(out/('mutation-'+name+'.out')).write_text(log)
  failed=sum(map(int,re.findall(r'test result: FAILED\. \d+ passed; (\d+) failed',log)))
  results.append(dict(name=name,argv=argv,exit_code=p.returncode,failed=failed,cargo_target_dir=os.environ['CARGO_TARGET_DIR']))
  assert p.returncode==101 and failed>0,(name,log)
  print(name,failed,flush=True)
finally:
 path.write_text(original)
 subprocess.run(['cargo','fmt','--all'],check=True)
 (out/'mutation.json').write_text(json.dumps(results,indent=2)+'\n')
