"""Verify the same real tests detect broken observation/identity gates."""
from pathlib import Path
import subprocess,json
root=Path(__file__).resolve().parents[5]
source=root/'pkg/schedule/checker/split_scatter_observe.rs'
original=source.read_text()
run=Path(__file__).with_name('run.py')
mutations={
 'source-equality': ('split_scatter_region_version(Some(&source)) < pending.split_version','split_scatter_region_version(Some(&source)) != pending.split_version'),
 'stale-ready': ('std::cmp::Ordering::Greater => return SplitScatterObservation::StaleResult,','std::cmp::Ordering::Greater => return SplitScatterObservation::Ready,'),
 'ignore-deadline': ('self.pending.get(&expected.region_id) == Some(expected)','self.pending.get(&expected.region_id).is_some_and(|p| p.region_id == expected.region_id && p.group == expected.group && p.source_region_id == expected.source_region_id && p.split_version == expected.split_version)'),
 'one-bound': ('[left, right].iter().all','[left, right].iter().any'),
}
results=[]
try:
 for name,(old,new) in mutations.items():
  assert old in original
  source.write_text(original.replace(old,new))
  subprocess.run(['python3',str(run),'mutation-'+name+'-fmt','cargo','fmt','--all'],check=True)
  code=subprocess.run(['python3',str(run),'mutation-'+name,'cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_053','--','--nocapture']).returncode
  log=run.with_name('mutation-'+name+'.log').read_text()
  assert code==101 and 'assertion' in log and 'FAILED' in log
  results.append({'mutation':name,'exit_code':code,'behavior_assertion_failed':True})
finally:
 source.write_text(original)
 run.with_name('mutation-results.json').write_text(json.dumps(results,indent=2)+'\n')
