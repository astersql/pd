"""Read-only existing Go binary; isolated Delve calls to real index methods."""
import pathlib, subprocess, threading
root=pathlib.Path(__file__).parent
p=subprocess.Popen(['docker','exec','-i','pd-task15-oracle','dlv','exec','/oracle/pd-classic-linux','--allow-non-terminal-interactive','--','--name','index15','--data-dir','/tmp/index15','--client-urls','http://0.0.0.0:2379','--advertise-client-urls','http://127.0.0.1:2379','--peer-urls','http://127.0.0.1:2380','--log-file','/tmp/index15.log'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
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
def request():
 g={'__file__':str(root.parent/'task14/oracle.py')}
 source=(root.parent/'task14/oracle.py').read_text().split('cases=[]')[0].replace('pd-task14-oracle','pd-task15-oracle')
 exec(source,g)
 for id,index,start,end in [('low',-2,'10','40'),('high',5,'20','30'),('probe',1,'50','60')]:
  r=dict(g['base_rule'],id=id,index=index,data=[dict(start_key=start,end_key=end)])
  try: assert g['http']('/pd/api/v1/config/region-label/rule',r)[0]==200
  except Exception:pass
try:
 read();cmd('break /Users/Shared/work/dir/data/codes/pd/pkg/schedule/labeler/labeler.go:271')
 cmd('condition 1 rule.ID == "probe"')
 worker=threading.Thread(target=request);worker.start();cmd('continue')
 cmd('call l.ruleIndex.delete("keyspaces/0")')
 cmd('call l.ruleIndex.buildRanges()')
 cmd('print l.ruleIndex.rules')
 cmd('call l.ruleIndex.getSplitKeys(nil, nil)')
 key=lambda n:f'l.ruleIndex.ranges.segments[{n}].startKey' if n is not None else 'nil'
 for start,end in [(None,0),(0,1),(1,2),(1,3),(2,3),(3,4),(5,None)]:
  cmd(f'call l.ruleIndex.getRangeRules({key(start)}, {key(end)})')
 cmd(f'call l.ruleIndex.getSplitKeys({key(0)}, {key(3)})')
 cmd('call l.ruleIndex.delete("high")');cmd('call l.ruleIndex.buildRanges()')
 cmd('call l.ruleIndex.getSplitKeys(nil, nil)')
 cmd(f'call l.ruleIndex.getRangeRules({key(0)}, {key(1)})')
 cmd('set l.ruleIndex.rules["low"].Data = l.ruleIndex.rules["probe"].Data')
 cmd('set l.ruleIndex.rules["low"].Index = 7')
 cmd('call l.ruleIndex.set(l.ruleIndex.rules["low"])');cmd('call l.ruleIndex.buildRanges()')
 cmd('call l.ruleIndex.getSplitKeys(nil, nil)')
 cmd(f'call l.ruleIndex.getRangeRules({key(0)}, {key(1)})')
 cmd('call l.ruleIndex.delete("missing")');cmd('call l.ruleIndex.delete("low")');cmd('call l.ruleIndex.delete("probe")');cmd('call l.ruleIndex.buildRanges()')
 cmd('call l.ruleIndex.getSplitKeys(nil, nil)')
 assert 'Command failed:' not in ''.join(log) and '~panic:' not in ''.join(log)
 p.stdin.write('exit\ny\n');p.stdin.flush();log.append(p.stdout.read());p.wait(timeout=15);worker.join(timeout=15)
finally:
 if p.poll() is None:p.kill();p.wait()
 (root/'go.out').write_text('\n'.join(line.rstrip() for line in ''.join(log).splitlines())+'\n')
