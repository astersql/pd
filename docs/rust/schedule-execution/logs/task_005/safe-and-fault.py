import json,subprocess,hashlib
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_005');tmp=Path('/tmp/pd-sched005')
cmd=['docker','run','--rm','-v',str(tmp)+':/oracle:ro','alpine:3.22','/oracle/filter.test','-test.run','^(TestCandidates|TestDistinctScoreFilter|TestLabelConstraintsFilter|TestRuleFitFilter|TestRuleFitFilterWithPlacementRule|TestSendStateFilter)$','-test.v','-test.timeout=60s']
p=subprocess.run(cmd,capture_output=True,text=True);(out/'safe-tests.json').write_text(json.dumps(dict(argv=cmd,exit_code=p.returncode,stdout=p.stdout,stderr=p.stderr),indent=2)+'\n');print(p.returncode,p.stdout[-1200:])
prefix=(out/'oracle.commands').read_text().split('print "CASE ')[0]
s=prefix+'call -unsafe collector = plan.NewCollector(collector.basePlan)\nset collector.basePlan.(*plan.BalanceSchedulerPlan).Step = 1\ncall -unsafe filter.SelectTargetStores(stores, filters[:1], conf, collector, nil)\nexit\n'
(tmp/'fault.commands').write_text(s);(out/'fault.commands').write_text(s)
cmd=json.loads((out/'oracle.json').read_text())['argv'];cmd=[x.replace('oracle.commands','fault.commands') for x in cmd]
p=subprocess.run(cmd,capture_output=True,text=True);(out/'fault.json').write_text(json.dumps(dict(argv=cmd,exit_code=p.returncode,stdout=p.stdout,stderr=p.stderr),indent=2)+'\n');print(p.stdout[-1000:]);assert '~panic: interface {}(*runtime.TypeAssertionError)' in p.stdout
