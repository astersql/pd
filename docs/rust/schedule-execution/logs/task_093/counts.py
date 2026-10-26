from pathlib import Path
import re,json,gzip
p=Path('docs/rust/schedule-execution/logs/task_093')
out={}
for name in ['focused','package','workspace-test','red-behavior']:
 f=p/(name+'.log')
 s=f.read_text() if f.exists() else gzip.open(str(f)+'.gz','rt').read()
 rows=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',s)
 out[name]=dict(zip(['passed','failed','ignored'],[sum(int(row[i]) for row in rows) for i in range(3)]))
(p/'counts.json').write_text(json.dumps(out,indent=2)+'\n')
print(json.dumps(out))
