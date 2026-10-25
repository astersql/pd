"""Run existing read-only Go binaries under Delve; freeze actual status returns."""
import hashlib,json,re,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_009');tmp=Path('/tmp/pd-sched005');cases=[];routes=[]
def run(name,commands,binary,test):
 commands+=['exit'];text='\n'.join(commands)+'\n';(out/(name+'.commands')).write_text(text);(tmp/('task009-'+name+'.commands')).write_text(text)
 argv=['docker','run','--rm','--cap-add','SYS_PTRACE','--security-opt','seccomp=unconfined','-v',str(tmp)+':/oracle:ro','alpine:3.22','timeout','180','/oracle/dlv','--check-go-version=false','exec','/oracle/'+binary,'--allow-non-terminal-interactive','--init=/oracle/task009-'+name+'.commands','--','-test.run',test,'-test.timeout=175s']
 p=subprocess.run(argv,capture_output=True,text=True);raw=out/(name+'.json');raw.write_text(json.dumps(dict(argv=argv,exit_code=p.returncode,stdout=p.stdout,stderr=p.stderr),indent=2)+'\n');assert p.returncode==0 and not re.search(r'commands:\d+:',p.stdout),(name,p.stdout[-4000:])
 parts=re.split(r'"CASE ([^"]+)"',p.stdout);cs=[c for c in cases if c['route']==name];assert len(parts)==2*len(cs)+1
 for c,cid,chunk in zip(cs,parts[1::2],parts[2::2]):
  assert c['id']==cid;res=re.findall(r'StatusCode: (?:\w+ \()?(-?\d+)\)?, DetailedReason: "([^"]*)"',chunk);assert len(res)==2,(cid,chunk);c['expected']=[dict(status=int(s),detail=d) for s,d in res]
 routes.append(dict(name=name,sha256=hashlib.sha256(raw.read_bytes()).hexdigest(),cases=len(cs)));print(name,len(cs),flush=True)
def observe(cmds,route,expr,store,**data):
 cid=route+'/'+str(len(cases));cases.append(dict(id=cid,route=route,**data));cmds.append(f'print "CASE {cid}"')
 for direction in ['Source','Target']:cmds.append(f'call -unsafe {expr}.{direction}(nil,{store})')
labels=['zone','rack','host'];locs=[['z1','r1','h1'],['z1','r1','h2'],['z1','r2','h1'],['z2','r1','h1'],['z2','r2','h1'],['z3','r1','h1']]
for i,(members,source) in enumerate([([1,2,3],1),([1,3,4],1),([1,4,6],4)]):
 route='location'+str(i);cmds=['break filters_test.go:67']+['continue']*(i+1)+['clearall','config max-variable-recurse 1']
 for target in range(1,7):
  for kind,expr in [('safeguard','ls'),('improve','li')]:observe(cmds,route,expr,f'allStores[{target-1}]',kind=kind,labels=labels,stores=[dict(id=j+1,labels=dict(zip(labels,x))) for j,x in enumerate(locs)],members=members,source=source,target=target)
 run(route,cmds,'filter.test','^TestDistinctScoreFilter$')
for explicit in [False,True]:
 route='explicit_rules' if explicit else 'default_rules'
 cmds=['break filters.go:'+('768' if explicit else '717'),'continue','clearall','config max-variable-recurse 1','print f.oldFit','print f.region.meta']
 stores=[dict(id=i+1,labels=dict(dc='dc1' if i<4 else 'dc2',zone='z'+str(i+1),host='h'+str(i+1))) for i in range(6)] if explicit else [dict(id=i+1,labels=dict(zone=z)) for i,z in enumerate(['z1','z1','z2','z2','z3','z4','z2'])]
 peers=[dict(id=i,store_id=i,is_witness=(i==7)) for i in [1,3,5,7]] if not explicit else [dict(id=p,store_id=s,is_witness=False) for p,s in [(1,1),(2,2),(3,3),(4,5),(5,6)]]
 rules=[dict(group_id='pd',id='default',role='voter',count=3,location_labels=['zone'])] if not explicit else [dict(group_id='test',id='r'+str(i+1),role=role,count=count,location_labels=['dc','zone','host'],label_constraints=[dict(key='dc',op='in',values=[dc])]) for i,(role,count,dc) in enumerate([('leader',1,'dc1'),('voter',2,'dc1'),('voter',2,'dc2')])]
 base=dict(stores=stores,peers=peers,leader=1,rules=rules)
 for source in [1,99]:
  for target in range(1,len(stores)+1):
   for kind,allow in [('peer',False),('leader',False),('leader',True)]:
    ctor=f'newRuleFitFilter("task009",f.cluster,f.ruleManager,f.region,f.oldFit,{source})' if kind=='peer' else f'newRuleLeaderFitFilter("task009",f.cluster,f.ruleManager,f.region,f.oldFit,{source},{str(allow).lower()})'
    observe(cmds,route,ctor,f'f.cluster.StoresInfo.GetStore({target})',kind=kind,source=source,target=target,allow_move=allow,**base)
 run(route,cmds,'filter.test','^TestRuleFitFilterWithPlacementRule$' if explicit else '^TestRuleFitFilter$')
route='witness';cmds=['break balance_witness_test.go:116','continue','call -unsafe suite.tc.SetEnablePlacementRules(true)','clearall','break filters.go:824','continue','clearall','config max-variable-recurse 1','print f.oldFit','print f.region.meta','print f.region.leader']
base=dict(stores=[dict(id=i,labels={}) for i in range(1,5)],peers=[dict(id=i,store_id=i,is_witness=(i==4)) for i in range(1,5)],leader=3,rules=[dict(group_id='pd',id='default',role='voter',count=4)])
# Preserve actual allocated peer IDs in this run for leader identity comparisons.
cmds+=['config max-variable-recurse 3','print f.region.meta.Peers','config max-variable-recurse 1']
for source in [4,99]:
 cmds.append(f'set f.srcStore = {source}')
 for target in range(1,5):observe(cmds,route,'f',f'f.cluster.StoresInfo.GetStore({target})',kind='witness',source=source,target=target,**base)
# Existing real store reused with a missing-member ID; no algorithm mutation.
cmds+=['set store.meta.Id = 99'];observe(cmds,route,'f','store',kind='witness',source=99,target=99,**(base|dict(stores=base['stores']+[dict(id=99,labels={})])))
run(route,cmds,'schedulers.test','^TestBalanceWitnessSchedulerTestSuite$/^TestTransferWitnessOut$')
Path('docs/rust/schedule-execution/fixtures/task_009.json').write_text(json.dumps(dict(source_commit='836fdc7cf3ef34f7eabc9d051418995bb97efabe',routes=routes,cases=cases),indent=2)+'\n')
