import pathlib, re, subprocess
root=pathlib.Path(__file__).parent
p = subprocess.Popen(['docker','exec','-i','pd-task10-oracle','dlv','exec','/oracle/pd-classic-linux','--allow-non-terminal-interactive','--','--version'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
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
pkg='github.com/tikv/pd/pkg/schedule/config'
try:
    read();cmd('config max-string-len 4096');cmd('config max-array-values 128');cmd('break main.main');cmd('continue');cmd('next')
    c=ptr(cmd('call runtime.mallocgc(4096, nil, true)'),pkg+'.StoreConfig')
    for method in ['GetRegionMaxSize','GetRegionSplitSize','GetRegionMaxKeys','GetRegionSplitKeys','GetRegionBucketSize','IsRaftKV2','GetUnifiedReadPoolMaxThreadCount']:
        cmd(f'call {c}.{method}()')
    for size,merge,keys,mkeys in [(116,20,1640000,200000),(164,20,1640000,200000),(146,50,1460000,500000),(147,50,1470000,500000),(192,96,1920000,960000),(144,48,1440000,480000)]:
        cmd(f'call {c}.CheckRegionSize({size}, {merge})')
        cmd(f'call {c}.CheckRegionKeys({keys}, {mkeys})')
    cmd(f'call {c}.RegionSplitSize = "100KiB"')
    cmd(f'call {c}.RegionMaxSize = "1GiB"')
    cmd(f'call {c}.RegionBucketSize = "bad"')
    cmd(f'call {c}.Adjust()')
    cmd(f'call {c}.SetRegionBucketEnabled(true)')
    for method in ['GetRegionMaxSize','GetRegionSplitSize','GetRegionBucketSize']:
        cmd(f'call {c}.{method}()')
    cmd(f'call {c}.CheckRegionSize(2000, 50)')
    cmd(f'call {c}.Engine = "raft-kv2"')
    cmd(f'call {c}.IsRaftKV2()')
    clone=ptr(cmd(f'call {c}.Clone()'),pkg+'.StoreConfig')
    cmd(f'call {c}.Equal({clone})')
    cmd(f'set {clone}.RegionMaxSizeMB = 999')
    cmd(f'call {c}.Equal({clone})')
    cmd(f'set {clone}.ReadPool.Unified.MaxThreadCount = 12')
    cmd(f'call {c}.Equal({clone})')
    cmd(f'call {clone}.GetUnifiedReadPoolMaxThreadCount()')
    cmd(f'call {c}.GetUnifiedReadPoolMaxThreadCount()')
finally:
    p.stdin.write('exit\n');p.stdin.flush();log.append(p.stdout.read());p.wait()
    (root/'go.out').write_text('\n'.join(line.rstrip() for line in ''.join(log).splitlines()).rstrip()+'\n')
