"""Observe existing expiry via Delve; no source writes, no failpoints."""
import pathlib, subprocess, threading
root=pathlib.Path(__file__).parent
p=subprocess.Popen(['docker','exec','-i','pd-task14-oracle','dlv','exec','/oracle/pd-classic-linux','--allow-non-terminal-interactive','--','--name','boundary14','--data-dir','/tmp/boundary14-final4','--client-urls','http://0.0.0.0:2379','--advertise-client-urls','http://127.0.0.1:2379','--peer-urls','http://127.0.0.1:2380','--log-file','/tmp/boundary14-final4.log'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
log=[]
def read():
    s=''
    while not s.endswith('(dlv) '):
        c=p.stdout.read(1)
        if not c:break
        s+=c
        if s.endswith('[Y/n]?'): p.stdin.write('n\n');p.stdin.flush()
    log.append(s);print(s,flush=True);return s
def cmd(s):
    log.append('COMMAND '+s+'\n');p.stdin.write(s+'\n');p.stdin.flush();return read()
def request():
    g={'__file__':__file__}
    exec((root/'oracle.py').read_text().split('cases=[]')[0],g)
    rule=g['base_rule'];rule['labels'][0].update(ttl='1h',start_at='Fri Jan  1 00:00:00 UTC 2100')
    try: g['http']('/pd/api/v1/config/region-label/rule',rule)
    except Exception: pass
try:
    read();cmd('sources labeler/rules.go');cmd('break /Users/Shared/work/dir/data/codes/pd/pkg/schedule/labeler/rules.go:230')
    cmd('condition 1 rule.ID == "r"')
    worker=threading.Thread(target=request);worker.start()
    cmd('continue')
    cmd('print rule.Labels[0].expire')
    cmd('call rule.Labels[0].expireBefore(*rule.Labels[0].expire)')
    cmd('call rule.expireBefore(*rule.minExpire)')
    cmd('call rule.Labels[0].expire.Add(1)')
    cmd('call rule.Labels[0].expireBefore(rule.Labels[0].expire.Add(1))')
    cmd('call rule.expireBefore(rule.minExpire.Add(1))')
    cmd('call rule.Labels[0].expireBefore(rule.Labels[0].expire.Add(-1))')
    assert ''.join(log).count('~r0: false') == 3
    assert ''.join(log).count('~r0: true') == 2
    p.stdin.write('exit\ny\n');p.stdin.flush();log.append(p.stdout.read());p.wait(timeout=15)
    worker.join(timeout=15)
finally:
    if p.poll() is None:p.kill();p.wait()
    (root/'boundary.out').write_text('\n'.join(line.rstrip() for line in ''.join(log).splitlines())+'\n')
