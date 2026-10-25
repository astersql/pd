import json,hashlib,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_005');tmp=Path('/tmp/pd-sched005')
cmds=['break *0x1592a30','condition 1 collector != nil','continue','clearall','config max-variable-recurse 0','config max-array-values 10','print "INPUT stores"','print stores']
for i in range(4):cmds+= [f'call stores[{i}].GetID()']
cmds+= ['call -unsafe filters[1] = filters[0]', 'call -unsafe filters[0] = filter.NewExcludedFilter("task005", filter.NewSnapshotSendFilter(stores[:1], 1).(*filter.SnapshotSenderFilter).senders, filter.NewSnapshotSendFilter(stores[1:2], 1).(*filter.SnapshotSenderFilter).senders)']
cases=[]
for name,mode,nf,step,removed in [('source_empty','Source',0,0,False),('target_empty','Target',0,0,False),('unavailable_empty','UnavailableTarget',0,0,False),('excluded_source','Source',1,0,False),('excluded_target','Target',1,0,False),('excluded_unavailable','UnavailableTarget',1,0,False),('target_step2','Target',1,2,False),('first_excluded','Target',2,2,True),('first_state','Target',2,2,True)]:
 cmds += [f'print "CASE {name}"','call -unsafe collector = plan.NewCollector(collector.basePlan)',f'set collector.basePlan.(*plan.BalanceSchedulerPlan).Step = {step}']
 if removed:
  cmds+=['set stores[1].meta.NodeState = 3']
 if name=='first_excluded':
  # Keep the real StoreStateFilter in slot 1; only evaluate one removed input.
  cmds+=['call filters[1].Target(conf, stores[1])']
 if name=='first_state':
  cmds+=['call -unsafe filters[0] = filters[1]']
 stores='stores[1:2]' if removed else 'stores'
 cmds += [f'call -unsafe filter.Select{mode}Stores({stores}, filters[:{nf}], conf, collector, nil)','print collector.unschedulablePlans']
 count=1 if nf else 0
 if count:
  cmds += ['call collector.unschedulablePlans[0].GetStep()','call collector.unschedulablePlans[0].GetStatus()']
  resource_step=2 if mode=='UnavailableTarget' else step
  cmds += [f'call collector.unschedulablePlans[0].GetResource({resource_step})']
 cases.append(dict(name=name,mode=mode,filters=nf,base_step=step,removed=removed))
cmds+=['exit'];(tmp/'oracle.commands').write_text('\n'.join(cmds)+'\n')
cmd=['docker','run','--rm','--cap-add','SYS_PTRACE','--security-opt','seccomp=unconfined','-v',str(tmp)+':/oracle:ro','alpine:3.22','timeout','60','/oracle/dlv','--check-go-version=false','exec','/oracle/schedulers.test','--allow-non-terminal-interactive','--init=/oracle/oracle.commands','--','-test.run','^TestBalanceLeaderSchedulerTestSuite$/^TestBalanceLimit$','-test.timeout=55s']
p=subprocess.run(cmd,capture_output=True,text=True)
(out/'oracle.json').write_text(json.dumps(dict(argv=cmd,exit_code=p.returncode,stdout=p.stdout,stderr=p.stderr),indent=2)+'\n');(out/'oracle.commands').write_text((tmp/'oracle.commands').read_text());(out/'case-inputs.json').write_text(json.dumps(cases,indent=2)+'\n');print(p.stdout);print(p.stderr)
