"""Freeze real Go production metrics from existing binaries; never compile Go."""
import subprocess,os,select,time,json,re,hashlib
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_011');tmp=Path('/tmp/pd-sched003');commands=[];transcript=[];exchanges=[]
argv=['docker','run','--rm','-i','--cap-add','SYS_PTRACE','--security-opt','seccomp=unconfined','-v',str(tmp)+':/oracle:ro','alpine:3.22','timeout','180','/oracle/dlv','--check-go-version=false','exec','/oracle/cluster.test','--allow-non-terminal-interactive','--','-test.run','^TestSetOfflineStore$','-test.timeout=175s']
p=subprocess.Popen(argv,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
def read():
 b=b'';end=time.time()+170
 while time.time()<end:
  if select.select([p.stdout],[],[],1)[0]:
   x=os.read(p.stdout.fileno(),65536)
   if not x:break
   b+=x
   if b.endswith(b'(dlv) '):break
 s=b.decode();transcript.append(s);return s
def cmd(s):
 commands.append(s);p.stdin.write((s+'\n').encode());p.stdin.flush();s=read();exchanges.append(dict(command=commands[-1],output=s))
 assert not any(x in s for x in ['could not','has no member','not allowed','can not convert','not found','too many arguments','not enough arguments']),s
 return s
cases=[]
def observe(name,ops,ids):
 # Metric map stores labels and the concrete Go Counter value, no calls that create series.
 rows=[]
 for direction in ['source','target']:
  s=cmd('print filter.filter'+direction.title()+'Counter.MetricVec.metricMap.metrics')
  pattern=r'values: \[\]string len: 3, cap: 3, \[\s*"([^"]*)",\s*"([^"]*)",\s*"([^"]*)",\s*\],\s*metric:.*?valBits: (\d+),\s*valInt: (\d+)'
  for scope,kind,store,bits,value in re.findall(pattern,s,re.S):
   assert bits=='0';rows.append(dict(direction=direction,scope=scope,filter=kind,store=store,value=int(value)))
  assert s.count('values: []string len: 3')==sum(r['direction']==direction for r in rows),s
 counts=[int(cmd(f'print {counter}.counter[{a}][{f}][{i}]').split('(dlv)')[0].strip()) for a,f,i in ids]
 cases.append(dict(id=name,operations=ops,pending_keys=ids,pending=counts,metrics=sorted(rows,key=lambda r:tuple(str(x) for x in r.values()))));print(name,len(rows),counts,flush=True)
try:
 read();cmd('break cluster_test.go:300');cmd('continue');cmd('clearall');cmd('config max-variable-recurse 3');cmd('config max-array-values 100');cmd('config max-string-len 1000')
 s=cmd('call -unsafe filter.NewCounter("balance-leader-scheduler")');addr=re.search(r'filter.Counter"\)\((0x[0-9a-f]+)\)',s)[1];counter=f'(*filter.Counter)({addr})'
 filters=cmd('print filter.filters');cmd('call -unsafe filter.ResetFilterMetrics()')
 ids=[(0,11,1),(1,11,2)]
 ops=[['inc','source',11,1],['inc','target',11,2]]
 for a in ops:cmd(f'call -unsafe {counter}.inc(filter.{a[1]},filter.filterType({a[2]}),{a[3]})')
 observe('pending',ops,ids)
 cmd(f'call -unsafe {counter}.inc(filter.source,filter.filterType(11),1)');cmd(f'call -unsafe {counter}.Flush(nil)');observe('flush',[['inc','source',11,1],['flush',False]],ids)
 cmd(f'call -unsafe {counter}.Flush(nil)');observe('empty_flush',[['flush',False]],ids)
 # SetScope has no callers and is linker-eliminated. Set exactly its single domain field.
 cmd(f'call -unsafe {counter}.inc(filter.source,filter.filterType(11),1)');cmd(f'call -unsafe {counter}.scope = "renamed"');cmd(f'call -unsafe {counter}.inc(filter.target,filter.filterType(0),1)');cmd(f'call -unsafe {counter}.Flush(nil)');observe('scope',[['inc','source',11,1],['scope','renamed'],['inc','target',0,1],['flush',False]],ids)
 cmd('call -unsafe filter.DeleteStoreMetrics("1")');observe('delete',[['delete','1']],ids)
 cmd('call -unsafe filter.DeleteStoreMetrics("1")');cmd('call -unsafe filter.DeleteStoreMetrics("99")');observe('delete_again',[['delete','1'],['delete','99']],ids)
 cmd('call -unsafe filter.ResetFilterMetrics()');observe('reset',[['reset']],ids)
 cmd(f'call -unsafe {counter}.scope = "lifecycle"')
 for a,i in [('source',1),('target',2),('source',99)]:cmd(f'call -unsafe {counter}.inc(filter.{a},filter.filterType(0),{i})')
 cmd(f'call -unsafe {counter}.Flush(cluster.ruleManager.storeSetInformer)')
 ids2=[(0,0,1),(1,0,2),(0,0,99)];observe('live_and_missing',[['scope','lifecycle'],['inc','source',0,1],['inc','target',0,2],['inc','source',0,99],['flush',True]],ids2)
 for a,i in [('source',1),('target',1),('target',2),('target',99)]:cmd(f'call -unsafe {counter}.inc(filter.{a},filter.filterType(0),{i})')
 # Mutate only the domain lifecycle input; no code or fault injection changes.
 cmd('print cluster.BasicCluster.StoresInfo.stores[1].meta')
 cmd('set cluster.BasicCluster.StoresInfo.stores[1].meta.NodeState = 3')
 cmd('print cluster.BasicCluster.StoresInfo.stores[1].meta.NodeState')
 cmd('call -unsafe filter.DeleteStoreMetrics("1")');cmd(f'call -unsafe {counter}.Flush(cluster.ruleManager.storeSetInformer)');observe('buried_before_flush',[['inc','source',0,1],['inc','target',0,1],['inc','target',0,2],['inc','target',0,99],['remove',1],['delete','1'],['flush',True]],ids2)
 cmd(f'call -unsafe {counter}.Flush(nil)');observe('no_resurrection',[['flush',False]],ids2)
 descriptors=[cmd('print filter.filter'+d+'Counter.MetricVec.metricMap.desc') for d in ['Source','Target']]
 cmd('exit');code=p.wait();assert code==0
 fixture=dict(source_commit='836fdc7cf3ef34f7eabc9d051418995bb97efabe',initial_stores=[1,2,3,4,5,6],cases=cases,filter_names=re.findall(r'"([^"]+)"',filters),descriptors=[dict(name=re.search(r'fqName: "([^"]+)"',d)[1],help=re.search(r'help: "([^"]+)"',d)[1],labels=re.findall(r'"([^"]+)"',re.search(r'names: \[\]string len: 3, cap: 3, \[([^\]]+)\]',d)[1]),type='COUNTER') for d in descriptors])
 Path('docs/rust/schedule-execution/fixtures/task_011.json').write_text(json.dumps(fixture,indent=2)+'\n')
finally:
 if p.poll() is None:p.terminate();p.wait()
 (out/'oracle.json').write_text(json.dumps(dict(argv=argv,exit_code=p.returncode,commands=commands,exchanges=exchanges,stdout=''.join(transcript)),indent=2)+'\n')
 (out/'oracle.commands').write_text('\n'.join(commands)+'\n')
