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
counts['table_driven_checks'] = dict(frozen_thresholds=10, history_modes=4, invalid_priorities=3, candidate_actions=7, engine_modes=4, reverse_leader_modes=2, reverse_membership=4, eviction_recovery_outcomes=6)
counts['table_driven_total'] = sum(counts['table_driven_checks'].values())
(base/'counts.json').write_text(json.dumps(counts,indent=2)+'\n')
print(json.dumps({k:{a:b for a,b in v.items() if a!='groups'} if isinstance(v,dict) else v for k,v in counts.items()}))
