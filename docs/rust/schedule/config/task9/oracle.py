import pathlib, re, subprocess
root=pathlib.Path(__file__).parent
p = subprocess.Popen(['docker','exec','-i','pd-task9-oracle','dlv','exec','/oracle/pd-classic-linux','--allow-non-terminal-interactive','--','--version'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
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
    c=ptr(cmd('call runtime.mallocgc(4096, nil, true)'),pkg+'.ScheduleConfig')
    b=ptr(cmd('call runtime.mallocgc(4096, nil, true)'), '[4096]uint8')
    for i,v in enumerate(b'{} {}'): cmd(f'set {b}[{i}] = {v}')
    cmd(f'call {c}.UnmarshalJSON((*{b})[:5])')
    cmd(f'call {c}.UnmarshalJSON((*{b})[:2])')
    for payload in ['{"default-store-limit":{"add-peer":null},"low-space-ratio":null}', '{"enable-joint-consensus":true}', '{"enable-joint-consensus":"false","max-snapshot-count":0}']:
        log.append('SCENARIO '+payload+'\n')
        for i,v in enumerate(payload.encode()): cmd(f'set {b}[{i}] = {v}')
        cmd(f'call {c}.UnmarshalJSON((*{b})[:{len(payload)}])')
    meta=ptr(cmd('call runtime.mallocgc(4096, nil, true)'), 'github.com/tikv/pd/pkg/utils/configutil.ConfigMetaData')
    cmd(f'call {c}.Adjust({meta}, false)')
    cmd(f'print {c}')
    payload='{"schedulers-v2":[{"type":"balance-region","args":["a"]}],"store-limit":{"7":{"add-peer":3}}}'
    for i,v in enumerate(payload.encode()): cmd(f'set {b}[{i}] = {v}')
    cmd(f'call {c}.UnmarshalJSON((*{b})[:{len(payload)}])')
    clone=ptr(cmd(f'call {c}.Clone()'),pkg+'.ScheduleConfig')
    cmd(f'set {clone}.MaxSnapshotCount = 999')
    cmd(f'print {c}.MaxSnapshotCount')
    cmd(f'call {clone}.Schedulers[0].Type = "changed"')
    cmd(f'print {c}.Schedulers[0].Type')
    cmd(f'set {clone}.StoreLimit[7] = {c}.DefaultStoreLimit')
    cmd(f'print {c}.StoreLimit[7].AddPeer')
    label=ptr(cmd('call runtime.mallocgc(4096, nil, true)'), 'github.com/pingcap/kvproto/pkg/metapb.StoreLabel')
    labels=ptr(cmd('call runtime.mallocgc(4096, nil, true)'), '[1]*github.com/pingcap/kvproto/pkg/metapb.StoreLabel')
    cmd(f'set {labels}[0] = {label}')
    cmd(f'call {label}.Key = "zone"')
    for value in ['', '-_./', '$abc', 'a b', 'é', 'a\\n']:
        cmd(f'call {label}.Value = "{value}"')
        cmd(f'call "{pkg}".ValidateLabels((*{labels})[:1])')
    for key in ['z1','z_','a/b','$abc','$','é','a\\n']:
        cmd(f'call "{pkg}".ValidateLabelKey("{key}")')
finally:
    p.stdin.write('exit\n');p.stdin.flush();log.append(p.stdout.read());p.wait()
    (root/'go.out').write_text('\n'.join(line.rstrip() for line in ''.join(log).splitlines()).rstrip()+'\n')
