"""Read-only existing Go binary; isolated Delve calls to real index methods."""
import pathlib, subprocess, threading
root=pathlib.Path(__file__).parent
p=subprocess.Popen(['docker','exec','-i','pd-task16-oracle','dlv','exec','/oracle/pd-classic-linux','--allow-non-terminal-interactive','--','--name','index16','--data-dir','/tmp/index16','--client-urls','http://0.0.0.0:2379','--advertise-client-urls','http://127.0.0.1:2379','--peer-urls','http://127.0.0.1:2380','--log-file','/tmp/index16.log'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
log=[]
def read():
 s=''
 while not s.endswith('(dlv) '):
  c=p.stdout.read(1)
  if not c:break
  s+=c
  if s.endswith('[Y/n]?'):p.stdin.write('n\n');p.stdin.flush()
 log.append(s);print(s,flush=True);return s
def cmd(s):
 log.append('COMMAND '+s+'\n');p.stdin.write(s+'\n');p.stdin.flush();return read()
def boundary(mode,id):
 if id>=1<<24: mode+=1;id=0
 return bytes([mode,(id>>16)&255,(id>>8)&255,id&255,0,0,0,0,251])
def request():
 g={'__file__':str(root.parent/'task14/oracle.py')}
 source=(root.parent/'task14/oracle.py').read_text().split('cases=[]')[0].replace('pd-task14-oracle','pd-task16-oracle')
 exec(source,g)
 for id,modes,name in [(42,b'rx','keyspaces/42'),(43,b'x','keyspaces/43'),(255,b'x','keyspaces/255'),(256,b'x','keyspaces/256'),(16777215,b'rx','keyspaces/16777215'),(1,b'x','keyspaces/01'),(42,b'x','probe')]:
  r=dict(id=name,index=0,labels=[dict(key='id',value=str(id) if name!='keyspaces/01' else '01')],rule_type='key-range',data=[dict(start_key=boundary(m,id).hex(),end_key=boundary(m,id+1).hex()) for m in modes])
  try: assert g['http']('/pd/api/v1/config/region-label/rule',r)[0]==200
  except Exception:pass
try:
 read();cmd('break /Users/Shared/work/dir/data/codes/pd/pkg/schedule/labeler/labeler.go:271')
 cmd('condition 1 rule.ID == "probe"')
 worker=threading.Thread(target=request);worker.start();cmd('continue')
 cmd('call l.ruleIndex.delete("keyspaces/0")');cmd('call l.ruleIndex.buildRanges()')
 cmd('print l.ruleIndex.generic')
 cmd('call l.ruleIndex.keyspaces.Contains(l.ruleIndex.rules["keyspaces/42"])')
 cmd('call l.ruleIndex.keyspaces.Contains(l.ruleIndex.rules["keyspaces/01"])')
 cmd('call l.ruleIndex.keyspaces.Add(l.ruleIndex.rules["keyspaces/42"])')
 cmd('call l.ruleIndex.getSplitKeys(nil, nil)')
 # Reuse validated slices; no Go slice allocation or source injection.
 def kr(id,n,field):return f'l.ruleIndex.rules["keyspaces/{id}"].Data.([]*labeler.KeyRangeRule)[{n}].{field}'
 for id,n in [(42,0),(42,1),(43,0),(255,0),(256,0),(16777215,0),(16777215,1)]:
  start,end=kr(id,n,'StartKey'),kr(id,n,'EndKey')
  cmd(f'call l.ruleIndex.getRangeRules({start}, {end})')
  cmd(f'call l.ruleIndex.keyspaces.matchRule({start}[:4], {start})')
 cmd('set l.ruleIndex.rules["probe"].ID = l.ruleIndex.rules["keyspaces/42"].ID')
 cmd('call l.ruleIndex.keyspaces.Replace(l.ruleIndex.rules["keyspaces/42"],l.ruleIndex.rules["probe"])')
 cmd('call l.ruleIndex.keyspaces.Contains(l.ruleIndex.rules["probe"])')
 cmd('call l.ruleIndex.keyspaces.Remove("keyspaces/42",l.ruleIndex.rules["keyspaces/42"])')
 cmd('call l.ruleIndex.keyspaces.Remove("keyspaces/42",l.ruleIndex.rules["probe"])')
 cmd('set l.ruleIndex.rules["probe"].Index = 1')
 cmd('call l.ruleIndex.keyspaces.Add(l.ruleIndex.rules["probe"])')
 cmd('set l.ruleIndex.rules["probe"].Index = 0')
 cmd('call l.ruleIndex.keyspaces.Add(l.ruleIndex.rules["keyspaces/42"])')
 cmd('call l.ruleIndex.getSplitKeys(nil, nil)')
 assert 'Command failed:' not in ''.join(log) and '~panic:' not in ''.join(log)
 p.stdin.write('exit\ny\n');p.stdin.flush();log.append(p.stdout.read());p.wait(timeout=15);worker.join(timeout=15)
finally:
 if p.poll() is None:p.kill();p.wait()
 import re
 (root/'go.out').write_text(re.sub(r'0x[0-9a-fA-F]+','ADDR','\n'.join(line.rstrip() for line in ''.join(log).splitlines()))+'\n')
