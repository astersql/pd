import subprocess,json,hashlib
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_006')
s=['config max-variable-recurse 3','config max-array-values 12', 'break filter/comparer.go:34','continue','call conf.GetRegionScoreFormulaVersion()','call conf.GetHighSpaceRatio()','call conf.GetLowSpaceRatio()','clearall']
for i,(line,ret) in enumerate([(34,1),(36,-1),(38,0),(50,1),(52,-1),(54,0)],2):
 s += [f'trace filter/comparer.go:{line}',f'on {i} print "RETURN {ret}"',f'on {i} print sa',f'on {i} print sb']
 for name in ['a','b']:
  for field in ['meta.Id','meta.Labels','regionSize','regionWeight','storeStats.rawStats.Capacity','storeStats.rawStats.Available','storeStats.rawStats.UsedSize']:
   s += [f'on {i} print {name}.{field}']
 if line>=50:
  s += [f'condition {i} len(regionStores) == 2', f'on {i} print locationLabels', f'on {i} print regionStores[0].meta.Id', f'on {i} print regionStores[0].meta.Labels', f'on {i} print regionStores[1].meta.Id', f'on {i} print regionStores[1].meta.Labels']
s+=['continue','exit'];commands='\n'.join(s)+'\n';(out/'comparers.commands').write_text(commands);Path('/tmp/pd-sched005/task006.commands').write_text(commands)
cmd=['docker','run','--rm','--cap-add','SYS_PTRACE','--security-opt','seccomp=unconfined','-v','/tmp/pd-sched005:/oracle:ro','alpine:3.22','timeout','60','/oracle/dlv','--check-go-version=false','exec','/oracle/checker.test','--allow-non-terminal-interactive','--init=/oracle/task006.commands','--','-test.run','^TestReplicaCheckerTestSuite$/^(TestBasic|TestDistinctScore)$','-test.timeout=55s']
r=subprocess.run(cmd,capture_output=True,text=True);(out/'comparers.json').write_text(json.dumps(dict(argv=cmd,exit_code=r.returncode,stdout=r.stdout,stderr=r.stderr),indent=2)+'\n');print(r.stdout[:5000]);print('bytes',len(r.stdout));assert ': Process ' in r.stdout and 'has exited with status 0' in r.stdout; assert not r.stderr
