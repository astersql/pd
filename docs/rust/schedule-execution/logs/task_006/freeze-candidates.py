import json,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_006')
s=['config max-variable-recurse 0','config max-array-values 12']
# Observe existing assertions and random samples without changing the test.
fields={85:['cs.Stores'],94:['minStore.meta.Id','maxStore.meta.Id'],97:['cs.Stores'],103:['cs.Stores'],106:['store.meta.Id'],111:['cs.Stores'],115:['cs.Stores']}
for i,(line,exprs) in enumerate(fields.items(),1):
 s += [f'trace candidates_test.go:{line}',f'on {i} print "LINE {line}"']
 for expr in exprs:s += [f'on {i} print {expr}']
 if line in [97,103,111,115]:
  for j in range({97:7,103:7,111:3,115:2}[line]):s += [f'on {i} print cs.Stores[{j}].meta.Id']
s += ['trace math/rand/v2/rand.go:246','on 8 print i','on 8 print j','continue','exit']
commands='\n'.join(s)+'\n';(out/'candidates.commands').write_text(commands);Path('/tmp/pd-sched005/task006.commands').write_text(commands)
cmd=['docker','run','--rm','--cap-add','SYS_PTRACE','--security-opt','seccomp=unconfined','-v','/tmp/pd-sched005:/oracle:ro','alpine:3.22','timeout','60','/oracle/dlv','--check-go-version=false','exec','/oracle/filter.test','--allow-non-terminal-interactive','--init=/oracle/task006.commands','--','-test.run','^TestCandidates$','-test.v','-test.timeout=55s']
r=subprocess.run(cmd,capture_output=True,text=True);(out/'candidates.json').write_text(json.dumps(dict(argv=cmd,exit_code=r.returncode,stdout=r.stdout,stderr=r.stderr),indent=2)+'\n');print(r.stdout);assert r.returncode==0 and '--- PASS: TestCandidates' in r.stdout
