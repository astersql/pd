"""Freeze actual existing Go returns without creating or modifying Go source."""
import hashlib,json,re,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_008');tmp=Path('/tmp/pd-sched005');cases=[];routes=[]
def run(name,cmds,binary,test):
 cmds=[re.sub(r'^set (.* = ".*")$', r'call -unsafe \1', c) for c in cmds];cmds+=['exit'];(out/(name+'.commands')).write_text('\n'.join(cmds)+'\n');(tmp/('task008-'+name+'.commands')).write_text('\n'.join(cmds)+'\n')
 argv=['docker','run','--rm','--cap-add','SYS_PTRACE','--security-opt','seccomp=unconfined','-v',str(tmp)+':/oracle:ro','alpine:3.22','timeout','180','/oracle/dlv','--check-go-version=false','exec','/oracle/'+binary,'--allow-non-terminal-interactive','--init=/oracle/task008-'+name+'.commands','--','-test.run',test,'-test.timeout=175s']
 p=subprocess.run(argv,capture_output=True,text=True);raw=out/(name+'.json');raw.write_text(json.dumps(dict(argv=argv,exit_code=p.returncode,stdout=p.stdout,stderr=p.stderr),indent=2)+'\n')
 assert p.returncode==0 and not re.search(r'commands:\d+:',p.stdout),(name,p.stdout[-6000:])
 chunks=re.split(r'"CASE ([^"]+)"',p.stdout);part=[c for c in cases if c['route']==name];assert len(chunks)==2*len(part)+1
 for case,cid,chunk in zip(part,chunks[1::2],chunks[2::2]):
  assert cid==case['id'];statuses=re.findall(r'StatusCode: (?:\w+ \()?(-?\d+)\)?, DetailedReason: "([^"]*)"',chunk);assert len(statuses)==2,(cid,chunk)
  case['expected']=[dict(status=int(s),detail=d) for s,d in statuses]
 routes.append(dict(name=name,sha256=hashlib.sha256(raw.read_bytes()).hexdigest(),cases=len(part)));print(name,len(part),flush=True)
def observe(cmds,route,kind,store,expr,**data):
 cid=f'{route}/{len(cases):03d}';cases.append(dict(id=cid,route=route,kind=kind,store=store,**data));cmds.append(f'print "CASE {cid}"')
 for d in ['Source','Target']:cmds.append(f'call -unsafe {expr}.{d}(conf,store)')
cmds=['break filters.go:498','continue','clearall','config max-variable-recurse 1','call conf.GetLowSpaceRatio()']
# Existing checker store has one actual label and real raw heartbeat stats.
base=dict(labels=[dict(key='noleader',value='true')],capacity=100*1024**3,available=100*1024**3,regions=100)
def stats(data):
 for field,k in [('store.storeStats.rawStats.Capacity','capacity'),('store.storeStats.rawStats.Available','available'),('store.regionCount','regions')]:cmds.append(f'set {field} = {data[k]}')
for capacity,available,regions in [(0,0,100),(1000,199,100),(1000,200,100),(1000,201,100),(100*1024**3,8*1024**3-1,99),(100*1024**3,8*1024**3,99),(100*1024**3,8*1024**3+1,99),(100*1024**3,8*1024**3+1,100),(1000,0,0),(1000,1001,100)]:
 data=base|dict(capacity=capacity,available=available,regions=regions);stats(data)
 observe(cmds,'constraints','storage',data,'filter.NewStorageThresholdFilter("task008")')
stats(base)
for value in ['tiflash','tikv','','TiFlash','unknown']:
 cmds+=['set store.meta.Labels[0].Key = "engine"',f'set store.meta.Labels[0].Value = "{value}"']
 for special in [True,False]:
  cmds.append('set filter.NotSpecialEngines.Op = "'+('in' if special else 'notIn')+'"')
  observe(cmds,'constraints','engine',base|dict(labels=[dict(key='engine',value=value)]),'filter.NewEngineFilter("task008", filter.'+'NotSpecialEngines'+')',special=special)
for value in ['', 'hotRegion','reserved','unknown','HotRegion']:
 cmds+=['set store.meta.Labels[0].Key = "specialUse"',f'set store.meta.Labels[0].Value = "{value}"']
 for low in [False,True]:
  data=base|dict(available=0 if low else base['capacity'],labels=[dict(key='specialUse',value=value)]);stats(data)
  for allow in [[],['reserved'],['hotRegion']]:
   arg='nil'
   if allow:cmds.append(f'set filter.NotSpecialEngines.Values[0] = "{allow[0]}"');arg='filter.NotSpecialEngines.Values'
   observe(cmds,'constraints','special_use',data,f'filter.NewSpecialUseFilter("task008", {arg})',allow=allow)
cmds+=['set filter.NotSpecialEngines.Values[0] = "tiflash"'];stats(base)
run('constraints',cmds,'checker.test','^TestReplicaCheckerTestSuite$/^TestReplaceOfflinePeer$')
cmds=['break filters.go:626','continue','clearall','config max-variable-recurse 1']
for bits in range(8):
 for i,field in enumerate(['slowStoreEvicted','stoppingStoreEvicted','slowTrendEvicted']):cmds.append(f'set store.{field}.v = {(bits>>i)&1}')
 observe(cmds,'hot','hot',dict(labels=[],evicted=bits),'NewHotRegionEvictedTargetFilter("task008")')
cmds=[c.replace('(conf,store)','(nil,store)') for c in cmds]
run('hot',cmds,'filter.test','^TestHotRegionEvictedTargetFilter$')
# LabelConstraint Source has a real single constraint and one store label.
cmds=['break filters.go:655','continue','clearall','config max-variable-recurse 1']
for key,value in [('id','1'),('ID','1'),('id','2'),('id',''),('engine','tikv'),('$x','1'),('exclusive','1')]:
 cmds += [f'set store.meta.Labels[0].Key = "{key}"',f'set store.meta.Labels[0].Value = "{value}"']
 for op in ['in','notIn','exists','notExists','invalid']:
  cmds += [f'set f.constraints[0].Op = "{op}"']
  observe(cmds,'labels','label',dict(labels=[dict(key=key,value=value)]),'f',constraints=[dict(key='id',op=op,values=['1'])])
cmds+=['set f.constraints = f.constraints[:0]']
observe(cmds,'labels','label',dict(labels=[dict(key='exclusive',value='1')]),'f',constraints=[])
cmds+=['set store.meta.Labels = store.meta.Labels[:0]']
observe(cmds,'labels','label',dict(labels=[]),'f',constraints=[])
cmds=[c.replace('(conf,store)','(nil,store)') for c in cmds]
run('labels',cmds,'filter.test','^TestLabelConstraintsFilter$')
# Real isolation constructor and region stores from existing table; freeze every
# original case by observing the f/store arguments at successive target entries.
cmds=['break filters.go:1035','continue','clearall','config max-variable-recurse 1','print f','print store.meta.Labels']
for i,key in enumerate(['zone','rack','host']):cmds.append(f'set store.meta.Labels[{i}].Key = "{key}"')
cmds+=['set store.meta.Labels[1].Value = "r1"','set store.meta.Labels[2].Value = "h1"']
# First original region stores have zones z1,z2. Constraint snapshots are domain inputs.
for value in ['z1','Z1','z2','z3','','K','k','Σ','ς','ß','ss','İ','i']:
 cmds += [f'set store.meta.Labels[0].Value = "{value}"']
 observe(cmds,'isolation','isolation',dict(labels=[dict(key='zone',value=value),dict(key='rack',value='r1'),dict(key='host',value='h1')]),'f',locations=['zone','rack','host'],level='zone',region_labels=[[dict(key='zone',value='z1')],[dict(key='zone',value='z2')]])
# Modify the actual input constraint strings to exercise Unicode and missing labels.
for constraint,value in [('K','k'),('Σ','ς'),('ß','ss'),('İ','i'),('',''),('','z3')]:
 cmds += [f'set f.constraintSet[0][0] = "{constraint}"',f'set store.meta.Labels[0].Value = "{value}"']
 observe(cmds,'isolation','isolation',dict(labels=[dict(key='zone',value=value),dict(key='rack',value='r1'),dict(key='host',value='h1')]),'f',locations=['zone','rack','host'],level='zone',region_labels=[[dict(key='zone',value=constraint)],[dict(key='zone',value='z2')]])
cmds+=['set f.constraintSet = f.constraintSet[:0]']
observe(cmds,'isolation','isolation',dict(labels=[dict(key='zone',value='z3'),dict(key='rack',value='r1'),dict(key='host',value='h1')]),'f',locations=['zone','rack','host'],level='zone',region_labels=[])
cmds=[c.replace('(conf,store)','(nil,store)') for c in cmds]
run('isolation',cmds,'filter.test','^TestIsolationFilter$')
for level,depth,region_values in [('rack',2,[['z1','r1'],['z1','r2'],['z3','r3']]),('host',3,[['z1','r1','h1'],['z1','r2','h1'],['z2','r1','h1']])]:
 route='isolation_'+level
 cmds=['break filters.go:1035',f'condition 1 len(f.constraintSet[0]) == {depth}','continue','clearall','config max-variable-recurse 1','print f']
 for i,key in enumerate(['zone','rack','host']):cmds.append(f'set store.meta.Labels[{i}].Key = "{key}"')
 for values in [['z1','r1','h1'],['Z1','R1','H1'],['z1','r1','h2'],['z1','r2','h1'],['z1','r3','h1'],['z2','r1','h1'],['z3','r3','h1'],['','','']]:
  for i,v in enumerate(values):cmds.append(f'set store.meta.Labels[{i}].Value = "{v}"')
  labels=lambda vs:[dict(key=k,value=v) for k,v in zip(['zone','rack','host'],vs)]
  observe(cmds,route,'isolation',dict(labels=labels(values)),'f',locations=['zone','rack','host'],level=level,region_labels=[labels(vs) for vs in region_values])
 cmds=[c.replace('(conf,store)','(nil,store)') for c in cmds]
 run(route,cmds,'filter.test','^TestIsolationFilter$')
cmds=['break filters_test.go:464','continue','clearall','config max-variable-recurse 1']
for level in ['zone','rack','host','unknown','','ZONE']:
 expr=f'NewIsolationFilter("task008", "{level}", opt.GetLocationLabels(), testCluster.BasicCluster.StoresInfo.GetRegionStores(testCase.region))'
 observe(cmds,'constructors','isolation',dict(labels=[dict(key='zone',value='z1'),dict(key='rack',value='r1'),dict(key='host',value='h1')]),expr,locations=['zone','rack','host'],level=level,region_labels=[[dict(key='zone',value='z1'),dict(key='rack',value='r1'),dict(key='host',value='h1')],[dict(key='zone',value='z2'),dict(key='rack',value='r1'),dict(key='host',value='h1')]])
observe(cmds,'constructors','isolation',dict(labels=[dict(key='zone',value='z1'),dict(key='rack',value='r1'),dict(key='host',value='h1')]),'NewIsolationFilter("task008", "", nil, nil)',locations=[],level='',region_labels=[])
cmds=[c.replace('(conf,store)','(nil,testCluster.BasicCluster.StoresInfo.GetStore(1))') for c in cmds]
run('constructors',cmds,'filter.test','^TestIsolationFilter$')
f=dict(source_commit='836fdc7cf3ef34f7eabc9d051418995bb97efabe',mode='classic/default',low_space_ratio=0.8,routes=routes,cases=cases)
Path('docs/rust/schedule-execution/fixtures/task_008.json').write_text(json.dumps(f,ensure_ascii=False,indent=2)+'\n')
