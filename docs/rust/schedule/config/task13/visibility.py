"""Pause existing Go process at publication before persistence.
Debugger reads locals only; no calls, mutation, injection, or source writes.
"""
import json, pathlib, subprocess, threading
def request(path, value=None, allow_error=False):
    args=['docker','exec',container,'wget','-qO-']
    if value is not None: args += ['--header=Content-Type: application/json','--post-data='+json.dumps(value)]
    p=subprocess.run(args+['http://127.0.0.1:2379'+path],text=True,capture_output=True)
    if allow_error: return {'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr}
    p.check_returncode()
    return json.loads(p.stdout)

root=pathlib.Path(__file__).parent
p=subprocess.Popen(['docker','exec','-i','pd-task13-oracle','dlv','exec','/oracle/pd-classic-linux','--allow-non-terminal-interactive','--','--name','visibility','--data-dir','/tmp/visibility-final2','--client-urls','http://127.0.0.1:2379','--peer-urls','http://127.0.0.1:2380','--log-file','/tmp/visibility.log'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
log=[]
def read():
    s=''
    while not s.endswith('(dlv) '):
        c=p.stdout.read(1)
        if not c: raise RuntimeError(s)
        s+=c
    log.append(s)
    return s
def cmd(s):
    log.append('COMMAND '+s+'\n');p.stdin.write(s+'\n');p.stdin.flush();out=read()
    if 'Command failed:' in out: raise RuntimeError(out)
    return out
try:
    read();cmd('break main.main');cmd('continue');cmd('config max-variable-recurse 2');cmd('config max-string-len 256')
    cmd('break persist_options.go:122')
    result=[]
    container='pd-task13-oracle'
    def send():
        import time
        for _ in range(60):
            try: request('/pd/api/v1/config');break
            except Exception: time.sleep(.5)
        result.append(request('/pd/api/v1/config/schedule',{'max-snapshot-count':102},True))
    t=threading.Thread(target=send);t.start()
    cmd('continue')
    assert cmd('print current.MaxSnapshotCount').splitlines()[0] == '64'
    assert cmd('print next.MaxSnapshotCount').splitlines()[0] == '102'
    cmd('next')
    published=cmd('print o.schedule.v')
    assert 'MaxSnapshotCount: 102' in published,published

    # Alarm cannot be requested while all threads are paused. The oracle already
    # ran a real NOSPACE rollback; this trace exercises successful publication.
    cmd('clearall')
finally:
    if p.poll() is None:
        p.stdin.write('exit\nn\n');p.stdin.flush()
        try:p.wait(timeout=5)
        except subprocess.TimeoutExpired:p.kill();p.wait()
    (root/'visibility.out').write_text(''.join(log))
