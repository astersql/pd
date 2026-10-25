"""Freeze excess healthy replicas to distinguish equality from at-least semantics."""
from pathlib import Path
import json
exec(Path('docs/rust/schedule-execution/logs/task_010/freeze.py').read_text().split("cmds=['break healthy_test.go:81'")[0])
fpath=Path('docs/rust/schedule-execution/fixtures/task_010.json');f=json.loads(fpath.read_text());cases.extend([c for c in f['cases'] if not c['route'].startswith('replication_extra')])
for rules in [False,True]:
 route='replication_extra'+str(rules);cmds=['break healthy.go:46']+['continue']*(11+int(rules))+['clearall','config max-variable-recurse 1','set region.learners = nil','set region.meta.Peers[0].Role = 0']
 observe(cmds,route,['IsRegionReplicated(cluster,region)','NewRegionReplicatedFilter(cluster).Select(region)'],peers=4,learner=False,pending=False,down=False,rules=rules)
 run(route,cmds,'^TestIsRegionHealthy$')
f['cases']=cases;fpath.write_text(json.dumps(f,indent=2)+'\n')
