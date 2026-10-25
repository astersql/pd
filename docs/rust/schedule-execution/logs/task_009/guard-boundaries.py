"""Freeze guards independently of isolation rejection, and nil-leader witness behavior."""
import json
from pathlib import Path
scope={};exec(Path('docs/rust/schedule-execution/logs/task_009/freeze.py').read_text().split("labels=['zone'")[0],scope)
f=Path('docs/rust/schedule-execution/fixtures/task_009.json');data=json.loads(f.read_text());data['cases']=[c for c in data['cases'] if c['route'] not in ('leader_witness','nil_leader')];data['routes']=[r for r in data['routes'] if r['name'] not in ('leader_witness','nil_leader')];scope['cases']=data['cases'];scope['routes']=data['routes'];observe=scope['observe'];run=scope['run']
base=next(c for c in data['cases'] if c['route']=='default_rules');base={k:base[k] for k in ['stores','peers','leader','rules']};base['peers']=[p|dict(is_witness=True) if p['store_id']==1 else p for p in base['peers']]
cmds=['break filters.go:717','continue','clearall','config max-variable-recurse 1','set f.region.meta.Peers[0].IsWitness = true']
for allow in [False,True]:observe(cmds,'leader_witness',f'newRuleLeaderFitFilter("task009",f.cluster,f.ruleManager,f.region,f.oldFit,1,{str(allow).lower()})','store',kind='leader',source=1,target=1,allow_move=allow,**base)
run('leader_witness',cmds,'filter.test','^TestRuleFitFilter$')
# This route starts at an actual witness target; source fit remains the real fit.
base=next(c for c in data['cases'] if c['route']=='witness');base={k:base[k] for k in ['stores','peers','leader','rules']};base['leader']=None
cmds=['break balance_witness_test.go:116','continue','call -unsafe suite.tc.SetEnablePlacementRules(true)','clearall','break filters.go:824','continue','clearall','config max-variable-recurse 1','set f.region.leader = nil']
for target in range(1,5):observe(cmds,'nil_leader','f',f'f.cluster.StoresInfo.GetStore({target})',kind='witness',source=4,target=target,**base)
run('nil_leader',cmds,'schedulers.test','^TestBalanceWitnessSchedulerTestSuite$/^TestTransferWitnessOut$')
f.write_text(json.dumps(data,indent=2)+'\n')
