"""Collect final, nonzero test counts and all completed command records."""
from pathlib import Path
import re,json
out=Path(__file__).resolve().parent
required=['fmt-transition','focused-final2','package-final','metadata-final','fmt-check-final','workspace-check-final','workspace-test-retry']
for name in required:
 record=json.loads((out/(name+'.json')).read_text())
 assert record['exit_code']==0,(name,record)
def summarize(name):
 text=(out/(name+'.log')).read_text()
 rows=[tuple(map(int,m)) for m in re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored',text)]
 result=dict(passed=sum(r[0] for r in rows),failed=sum(r[1] for r in rows),ignored=sum(r[2] for r in rows),targets=len(rows))
 assert result['passed']>0 and result['failed']==0,result
 result['ignored_tests']=re.findall(r'^test (.*?) \.\.\. ignored',text,re.M)
 return result
summary=dict(focused=summarize('focused-final2'),package=summarize('package-final'),workspace=summarize('workspace-test-retry'),fixture_cases=38,go_schedule_oracle_execution=False)
commands=[]
for p in sorted(out.glob('*.json')):
 if p.name in ['commands.json','test-summary.json']:continue
 value=json.loads(p.read_text())
 if isinstance(value,dict) and 'command' in value:commands.append(dict(phase=p.stem,**value))
(out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
(out/'test-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps(summary,indent=2))
