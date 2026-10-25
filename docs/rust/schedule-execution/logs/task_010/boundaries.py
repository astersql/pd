"""Additional runtime boundary observations using only domain input mutation."""
from pathlib import Path
import json
exec(Path('docs/rust/schedule-execution/logs/task_010/freeze.py').read_text().split("cmds=['break healthy_test.go:81'")[0])
fpath=Path('docs/rust/schedule-execution/fixtures/task_010.json');fixture=json.loads(fpath.read_text());cases.extend([c for c in fixture['cases'] if c['route'] not in ['snapshot_busy','snapshot_no_leader']])
cmds=['break region_filters.go:186','continue','clearall','config max-variable-recurse 1','set stores[0].storeStats.rawStats.IsBusy = true','break filters_test.go:235','continue','clearall']
observe(cmds,'snapshot_busy',['snapshotFilter.Select(region)'],busy=True)
run('snapshot_busy',cmds,'^TestSendStateFilter$')
cmds=['break filters_test.go:235','continue','clearall','config max-variable-recurse 1','set region.leader = nil']
observe(cmds,'snapshot_no_leader',['snapshotFilter.Select(region)'],leader=None)
run('snapshot_no_leader',cmds,'^TestSendStateFilter$')
cmds=['break region_filters_test.go:80','continue','call -unsafe testCluster.AddRegionStore(1,1,nil)','call -unsafe testCluster.AddRegionStore(2,1,nil)','call -unsafe testCluster.AddRegionStore(3,1,nil)','call -unsafe testCluster.SetEnablePlacementRules(true)','break region_filters.go:123','continue','clearall','config max-variable-recurse 1','print f.fit','call -unsafe f.Select(region)','print f.fit','set region.meta.Peers = region.meta.Peers[:2]','call -unsafe f.Select(region)','print f.fit']
run('fit_cache',cmds,'^TestRegionReplicatedFilter$')
fixture['cases']=cases;fpath.write_text(json.dumps(fixture,indent=2)+'\n')
# The immutable existing tests need no failpoint enable/injection on these paths.
import subprocess
argv=['docker','run','--rm','-v','/tmp/pd-sched005:/oracle:ro','alpine:3.22','/oracle/filter.test','-test.run','^(TestIsRegionHealthy|TestRegionPendingFilter|TestRegionDownFilter|TestRegionReplicatedFilter|TestRegionEmptyFilter|TestRegionWitnessFilter|TestSendStateFilter|TestAffinitySelect)$','-test.v','-test.timeout=90s']
p=subprocess.run(argv,capture_output=True,text=True);(out/'safe-tests.json').write_text(json.dumps(dict(argv=argv,exit_code=p.returncode,stdout=p.stdout,stderr=p.stderr),indent=2)+'\n');assert p.returncode==0 and p.stdout.count('--- PASS:')==8
