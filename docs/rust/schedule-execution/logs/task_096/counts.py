from pathlib import Path
import re,json,gzip
log=Path(__file__).parent
out={}
for name in ('focused-final','package','workspace-test'):
 p=log/f'{name}.log'
 text=p.read_text() if p.exists() else gzip.decompress((log/f'{name}.log.gz').read_bytes()).decode()
 matches=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',text)
 out[name]={'passed':sum(int(x[0]) for x in matches),'failed':sum(int(x[1]) for x in matches),'ignored':sum(int(x[2]) for x in matches),'groups':len(matches)}
(log/'counts.json').write_text(json.dumps(out,indent=2)+'\n')
print(json.dumps(out,indent=2))
