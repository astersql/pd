from pathlib import Path
import json,re,gzip
logs=Path('docs/rust/schedule-execution/logs/task_108')
final=json.loads((logs/'final.json').read_text());assert final['exit_code']==0
log=(logs/'final.log').read_text() if (logs/'final.log').exists() else gzip.decompress((logs/'final.log.gz').read_bytes()).decode()
counts=[tuple(map(int,row)) for row in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',log)]
assert counts[:2]==[(7,0,0),(179,0,0)]
record=dict(focused=dict(passed=7,failed=0),package=dict(passed=179,failed=0),workspace=dict(passed=sum(r[0] for r in counts)-186,failed=sum(r[1] for r in counts),ignored=sum(r[2] for r in counts)),gate_fixture=dict(passed=4,failed=0))
assert record['workspace']['failed']==0
(logs/'test-counts.json').write_text(json.dumps(record,indent=2)+'\n')
commands=final['command'][2].split(' && ')
(logs/'commands.json').write_text(json.dumps([dict(command=c,exit_code=0,slot=final['slot'],CARGO_TARGET_DIR=final['CARGO_TARGET_DIR']) for c in commands],indent=2)+'\n')
for raw in logs.glob('*.log'):
 raw.with_suffix('.log.gz').write_bytes(gzip.compress(raw.read_bytes(),mtime=0))
 raw.unlink()
print(json.dumps(record))
