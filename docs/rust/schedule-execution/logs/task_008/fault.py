"""Observe the real Go constructor's empty-location input fault."""
import json,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_008');tmp=Path('/tmp/pd-sched005')
cmds=['break filters_test.go:464','continue','clearall','config max-variable-recurse 2','call NewIsolationFilter("task008", "", nil, testCluster.BasicCluster.StoresInfo.GetRegionStores(testCase.region))','exit']
(out/'fault.commands').write_text('\n'.join(cmds)+'\n');(tmp/'task008-fault.commands').write_text('\n'.join(cmds)+'\n')
argv=['docker','run','--rm','--cap-add','SYS_PTRACE','--security-opt','seccomp=unconfined','-v',str(tmp)+':/oracle:ro','alpine:3.22','timeout','60','/oracle/dlv','--check-go-version=false','exec','/oracle/filter.test','--allow-non-terminal-interactive','--init=/oracle/task008-fault.commands','--','-test.run','^TestIsolationFilter$','-test.timeout=55s']
p=subprocess.run(argv,capture_output=True,text=True);(out/'fault.json').write_text(json.dumps(dict(argv=argv,exit_code=p.returncode,stdout=p.stdout,stderr=p.stderr),indent=2)+'\n');print(p.returncode,p.stdout[-4000:]);assert p.returncode==0 and 'panic' in p.stdout
