from pathlib import Path
import json,re,gzip
base=Path(__file__).parent
counts={}
for name in ['red-behavior','green','package','workspace-test']:
 p=base/(name+'.log')
 s=p.read_text() if p.exists() else gzip.open(str(p)+'.gz','rt').read()
 groups=[dict(passed=int(a),failed=int(b),ignored=int(c)) for a,b,c in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',s)]
 counts[name]={k:sum(g[k] for g in groups) for k in ['passed','failed','ignored']}
 counts[name]['groups']=groups
counts['fixture_source_cases']=13
counts['fixture_peer_cases']=4
counts['region_cases']=20
counts['zombie_cases']=6
counts['history_policy_cases']=4
(base/'counts.json').write_text(json.dumps(counts,indent=2)+'\n')
print(json.dumps({k:{a:b for a,b in v.items() if a!='groups'} if isinstance(v,dict) else v for k,v in counts.items()}))
