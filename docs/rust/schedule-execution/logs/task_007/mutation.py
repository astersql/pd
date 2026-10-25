"""Prove the same registered tests reject three concrete safety regressions."""
import json,re,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_007');p=Path('pkg/schedule/filter/store_state.rs');original=p.read_text();rows=[]
variants=[('removed-bypass','store.is_removed(),','false,'),('snapshot-equality','> conf.get_max_snapshot_count()', '>= conf.get_max_snapshot_count()'),('leader-budget-bypass','!store\n                            .is_available(Type::TRANSFER_LEADER_IN, self.operator_level)\n                            .await', 'false')]
# Select the exact rustfmt rendering instead of silently missing a mutation.
for i,(name,old,new) in enumerate(variants):
 try:
  if name=='leader-budget-bypass':
   match=re.search(r'!store\s*\.is_available\(Type::TRANSFER_LEADER_IN, self.operator_level\)\s*\.await',original);assert match;old=match.group()
  assert old in original,(name,old)
  p.write_text(original.replace(old,new,1));fmt=subprocess.run(['cargo','fmt','--all'],capture_output=True,text=True);assert fmt.returncode==0,fmt.stderr
  cmd=['cargo','test','-p','astersql-pd-schedule-filter','--locked','--lib','task_007','--','--nocapture'];r=subprocess.run(cmd,capture_output=True,text=True);raw=r.stdout+r.stderr;(out/f'mutation-{name}.out').write_text(raw)
  m=re.search(r'test result: FAILED\. (\d+) passed; (\d+) failed;',raw);assert r.returncode==101 and m and int(m[2])>0,raw
  assert 'task_007_test::task_007_behavior_cases ... FAILED' in raw
  rows.append(dict(name=name,argv=cmd,exit_code=r.returncode,passed=int(m[1]),failed=int(m[2])))
 finally:p.write_text(original)
(out/'mutation.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows,indent=2))
