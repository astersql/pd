"""Prove the fixed behavior test rejects four incorrect checker decisions."""
from pathlib import Path
import subprocess,json
root=Path(__file__).resolve().parents[5]
out=Path(__file__).resolve().parent
source=root/'pkg/schedule/checker/split_checker.rs'
correct=source.read_text()
mutations={
 'ignore-pause':('if self.pause.is_paused_at(now_unix_seconds) {','if false && self.pause.is_paused_at(now_unix_seconds) {'),
 'ignore-config':('keys.is_empty() && conf.is_placement_rules_enabled()','keys.is_empty() || conf.is_placement_rules_enabled()'),
 'lose-label-priority':('keys.is_empty() && conf.is_placement_rules_enabled()','conf.is_placement_rules_enabled()'),
 'drop-boundary':('keys = rules.get_split_keys(&range.start_key, &range.end_key);','keys = rules.get_split_keys(&range.start_key, &range.end_key); keys.pop();'),
}
results=[]
try:
 for name,(old,new) in mutations.items():
  assert old in correct
  source.write_text(correct.replace(old,new))
  subprocess.run(['python3',str(out/'run.py'),name+'-fmt','cargo','fmt','--all'],cwd=root,check=True,stdout=subprocess.DEVNULL)
  r=subprocess.run(['python3',str(out/'run.py'),name,'cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_048','--','--nocapture'],cwd=root,stdout=subprocess.DEVNULL)
  log=(out/(name+'.log')).read_text()
  assert r.returncode==101 and 'panicked at' in log and '1 failed' in log,(name,log)
  results.append(dict(mutation=name,exit_code=r.returncode,behavior_assertion_failed=True))
finally:
 source.write_text(correct)
(out/'mutation-results.json').write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results))
