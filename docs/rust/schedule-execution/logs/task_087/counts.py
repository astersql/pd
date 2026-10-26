# Summarize only actual Rust test-result records; zero-test groups are not behavior evidence.
from pathlib import Path
import gzip,json,re
logs=Path(__file__).resolve().parent
summary={}
for name in ['red-behavior','green','package','workspace-test']:
 p=logs/f'{name}.log'
 text=p.read_text() if p.exists() else gzip.open(str(p)+'.gz','rt').read()
 groups=[dict(passed=int(a),failed=int(b),ignored=int(c)) for a,b,c in re.findall(r'test result: .*? (\d+) passed; (\d+) failed; (\d+) ignored;',text)]
 summary[name]=dict(passed=sum(g['passed'] for g in groups),failed=sum(g['failed'] for g in groups),ignored=sum(g['ignored'] for g in groups),groups=groups)
summary['fixture_cases']=15
(logs/'counts.json').write_text(json.dumps(summary,indent=2)+'\n')
for key,value in summary.items():
 print(key,{k:v for k,v in value.items() if k!='groups'} if isinstance(value,dict) else value)
