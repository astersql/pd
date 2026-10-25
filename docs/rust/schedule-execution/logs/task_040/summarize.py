"""Count actual Cargo test summaries without counting filtered or ignored tests."""
from pathlib import Path
import re,json
out=Path(__file__).resolve().parent
summary={}
for name,index in [('focused',1),('checker',2),('workspace',6)]:
 text=(out/f'validation-{index}.log').read_text()
 counts=re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;',text)
 assert counts and all(state=='ok' for state,*_ in counts)
 summary[name]=dict(passed=sum(int(p) for _,p,f,i in counts),failed=sum(int(f) for _,p,f,i in counts),ignored=sum(int(i) for _,p,f,i in counts))
summary['task_040_cases']={'table':40,'comparison_boundaries':3,'witness_gate':6}
summary['workspace_ignored']=[s for s in (out/'validation-6.log').read_text().splitlines() if '... ignored' in s]
(out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps(summary,indent=2))
