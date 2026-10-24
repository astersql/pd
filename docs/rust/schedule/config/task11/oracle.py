import pathlib, re, subprocess
root=pathlib.Path(__file__).parent
p = subprocess.Popen(['docker','exec','-i','pd-task11-oracle','dlv','exec','/oracle/pd-classic-linux','--allow-non-terminal-interactive','--','--version'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
log=[]
def read():
    s=''
    while not s.endswith('(dlv) '):
        c=p.stdout.read(1)
        if not c: break
        s+=c
    log.append(s)
    return s
def cmd(s):
    log.append('COMMAND '+s+'\n');p.stdin.write(s+'\n');p.stdin.flush()
    out=read()
    if 'Command failed:' in out: raise RuntimeError(out)
    return out
def ptr(out,typ):
    m=re.search(r'\)\((0x[0-9a-f]+)\)',out)
    if not m: m=re.search(r'unsafe.Pointer\((0x[0-9a-f]+)\)',out)
    return f'(*"{typ}")({m[1]})'
pkg='github.com/tikv/pd/server/config'
try:
    read();cmd('config max-string-len 4096');cmd('config max-array-values 128');cmd('break main.main');cmd('continue');cmd('next')
    c=ptr(cmd('call runtime.mallocgc(65536, nil, true)'),pkg+'.Config')
    o=ptr(cmd(f'call "{pkg}".NewPersistOptions({c})'),pkg+'.PersistOptions')
    cmd(f'call {o}.GetMaxReplicas()')
    cmd(f'call {o}.GetRegionScheduleLimit()')
    cmd(f'call {o}.IsSchedulerDisabled("balance-leader-scheduler")')
    cmd(f'call {o}.AddSchedulerCfg("balance-leader-scheduler", nil)')
    cmd(f'call {o}.RemoveSchedulerCfg("balance-leader-scheduler")')
    cmd(f'call {o}.IsSchedulerDisabled("balance-leader-scheduler")')
    cmd(f'call {o}.SetLabelProperty("reject-leader", "zone", "east")')
    cmd(f'call {o}.CheckLabelProperty("reject-leader", nil)')
    l=ptr(cmd('call runtime.mallocgc(128, nil, true)'), 'github.com/pingcap/kvproto/pkg/metapb.StoreLabel')
    cmd(f'call {l}.Key = "zone"');cmd(f'call {l}.Value = "east"')
    a=ptr(cmd('call runtime.mallocgc(128, nil, true)'), '[1]*github.com/pingcap/kvproto/pkg/metapb.StoreLabel')
    cmd(f'set {a}[0] = {l}')
    cmd(f'call {o}.CheckLabelProperty("reject-leader", (*{a})[:])')
    cmd(f'call {l}.Value = "East"')
    cmd(f'call {o}.CheckLabelProperty("reject-leader", (*{a})[:])')
    cmd(f'call {o}.CheckLabelProperty("other", (*{a})[:])')
    cmd(f'call {c}.Schedule.MaxMergeRegionSize = 20')
    cmd(f'call {o}.SetScheduleConfig(&{c}.Schedule)')
    cmd(f'call {o}.GetMaxMergeRegionKeys()')
    cmd(f'call {o}.GetMaxAffinityMergeRegionSize()')
    cmd(f'call {o}.GetLabelPropertyConfig()')
    cmd('call "github.com/tikv/pd/pkg/schedule/config".RegisterScheduler("custom-scheduler")')
    cmd('call "github.com/tikv/pd/pkg/schedule/config".RegisterScheduler("custom-scheduler")')
finally:
    p.stdin.write('exit\n');p.stdin.flush();log.append(p.stdout.read());p.wait()
    (root/'go.out').write_text('\n'.join(line.rstrip() for line in ''.join(log).splitlines()).rstrip()+'\n')
