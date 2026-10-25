"""Observe the original location constructor's empty-store failure."""
import json,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_009');tmp=Path('/tmp/pd-sched005');text='\n'.join(['break filters_test.go:67','continue','clearall','config max-variable-recurse 3','call -unsafe NewLocationSafeguard("task009",labels,nil,allStores[0])','exit'])+'\n';(out/'fault.commands').write_text(text);(tmp/'task009-fault.commands').write_text(text)
argv=['docker','run','--rm','--cap-add','SYS_PTRACE','--security-opt','seccomp=unconfined','-v',str(tmp)+':/oracle:ro','alpine:3.22','timeout','60','/oracle/dlv','--check-go-version=false','exec','/oracle/filter.test','--allow-non-terminal-interactive','--init=/oracle/task009-fault.commands','--','-test.run','^TestDistinctScoreFilter$']
p=subprocess.run(argv,capture_output=True,text=True);(out/'fault.json').write_text(json.dumps(dict(argv=argv,exit_code=p.returncode,stdout=p.stdout,stderr=p.stderr),indent=2)+'\n');print(p.stdout);assert p.returncode==0 and 'makeslice: cap out of range' in p.stdout
