"""Run real registered tests against isolated, restored incorrect implementations."""
import json,subprocess,re
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_009');path=Path('pkg/schedule/filter/placement_guard.rs');original=path.read_text();results=[]
mutations=[
 ('safeguard_equality','LocationPolicy::Safeguard => score >= self.safe_score','LocationPolicy::Safeguard => score > self.safe_score'),
 ('improve_equality','LocationPolicy::Improve => score > self.safe_score','LocationPolicy::Improve => score >= self.safe_score'),
 ('rule_bypass','self.fit.replace(self.source, Some(store))','true'),
 ('leader_witness','|p| !p.is_witness','|_p| true'),
 ('leader_missing_peer','.map_or(self.allow_move,','.map_or(true,'),
 ('witness_leader','.is_some_and(|p| p.id != self.region.leader().map_or(0, |l| l.id))','.is_some_and(|_p| true)'),
 ('temporary_witness','is_witness: false','is_witness: p.is_witness'),
 ('supplied_fit','fit.unwrap_or_else(|| {','None::<Arc<RegionFit>>.unwrap_or_else(|| {'),
]
try:
 for name,old,new in mutations:
  assert old in original,name;path.write_text(original.replace(old,new,1));fmt=subprocess.run(['cargo','fmt','--all'],capture_output=True,text=True);assert fmt.returncode==0
  argv=['cargo','test','-p','astersql-pd-schedule-filter','--locked','--lib','task_009','--','--nocapture'];p=subprocess.run(argv,capture_output=True,text=True);raw=p.stdout+p.stderr;(out/(name+'.out')).write_text(raw.rstrip()+'\n');failed=sum(map(int,re.findall(r'; (\d+) failed;',raw)));results.append(dict(name=name,argv=argv,exit_code=p.returncode,failed=failed,stdout=p.stdout,stderr=p.stderr));print(name,p.returncode,failed,flush=True);assert p.returncode==101 and failed>0,(name,raw[-3000:])
finally:
 path.write_text(original);subprocess.run(['cargo','fmt','--all'],check=True);(out/'mutation.json').write_text(json.dumps(results,indent=2)+'\n')
