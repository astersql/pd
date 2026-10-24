"""Read-only existing binary oracle; debugger allocates metadata, never Go files."""
import pathlib, re, subprocess
root = pathlib.Path(__file__).parent
source = pathlib.Path('pkg/schedule/plan/balance_plan_test.go').read_text()
cases = {}
for n in range(1, 6):
    body = source.split(f'TestAnalyzerResult{n}()')[1].split('statuses,')[0]
    rows = []
    for line in body.splitlines():
        if 'plans = append' not in line: continue
        def field(name):
            m = re.search(name + r': suite\.(?:stores|regions)\[(\d)\]', line)
            return int(m[1])+1 if m else 0
        rows.append((field('Source'), field('Region'), field('Target'), int(re.search(r'Step: (\d)',line)[1]), re.search(r'NewStatus\((\w+)\)',line)[1]))
    cases[f'analyzer{n}'] = rows
cases['analyzer6'] = [(0,0,i,2,'StatusStoreDown') for i in range(1,6)] + [(1,1,0,1,'StatusRegionNoLeader')]
cases.update({
 'empty': [], 'success': [(1,0,0,0,'StatusOK')],
 'majority': [(1,0,0,0,s) for s in ['StatusStoreDown','StatusStoreDown','StatusStoreBusy']],
 'tie': [(1,0,0,0,s) for s in ['StatusStoreDown','StatusStoreBusy']],
 'priority': [(1,0,0,0,s) for s in ['StatusStoreScoreDisallowed']*3+['StatusStoreNotMatchRule']],
 'skip_lower': [(1,0,1,2,'StatusStoreNotMatchRule'),(1,0,0,0,'StatusStoreDown')],
 'replace_lower': [(1,0,0,0,'StatusStoreDown'),(1,0,1,2,'StatusStoreNotMatchRule')],
})
# Values fixed from the already verified status API.
codes = dict(StatusOK=0, StatusStoreScoreDisallowed=100, StatusStoreAlreadyHasPeer=101,
 StatusStoreNotMatchRule=102, StatusStoreRemoveLimitThrottled=203,StatusStoreDown=403,
 StatusStoreBusy=400, StatusRegionNotMatchRule=1004, StatusRegionNoLeader=1005)
(root/'inputs.tsv').write_text(''.join(f'{name}\t{s}\t{r}\t{t}\t{step}\t{codes[code]}\n' for name,rows in cases.items() for s,r,t,step,code in rows))
p = subprocess.Popen(['docker','exec','-i','pd-task8-oracle','dlv','exec','/oracle/pd-classic-linux','--allow-non-terminal-interactive','--','--version'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
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
planpkg='github.com/tikv/pd/pkg/schedule/plan'
core='github.com/tikv/pd/pkg/core'
meta='github.com/pingcap/kvproto/pkg/metapb'
try:
    read();cmd('break main.main');cmd('continue');cmd('next')
    stores={0:'nil'};regions={0:'nil'}
    for i in range(1,6):
        v=ptr(cmd(f'call "{core}".NewStoreInfo(nil, nil)'),core+'.StoreInfo')
        m=ptr(cmd('call runtime.mallocgc(4096, nil, true)'),meta+'.Store')
        cmd(f'set {v}.meta = {m}');cmd(f'set {v}.meta.Id = {i}');stores[i]=v
    for i in range(1,4):
        m=ptr(cmd('call runtime.mallocgc(4096, nil, true)'),meta+'.Region')
        cmd(f'set {m}.Id = {i}')
        peer=ptr(cmd('call runtime.mallocgc(4096, nil, true)'),meta+'.Peer')
        cmd(f'set {peer}.Id = {i}');cmd(f'set {peer}.StoreId = {i}')
        v=ptr(cmd(f'call "{core}".NewRegionInfo({m}, {peer}, nil)'),core+'.RegionInfo')
        regions[i]=v
    for name,rows in cases.items():
        log.append('SCENARIO '+name+'\n')
        v=ptr(cmd(f'call "{planpkg}".NewBalanceSchedulerPlan()'),planpkg+'.BalanceSchedulerPlan')
        c=ptr(cmd(f'call "{planpkg}".NewCollector({v}.Clone(nil))'),planpkg+'.Collector')
        v=f'{c}.basePlan.(*plan.BalanceSchedulerPlan)'
        # Collector.Clone keeps fields iff completed; set step 3 then override clone step.
        for s,r,t,step,code in rows:
            cmd(f'set {v}.Step = 3');cmd(f'set {v}.Source = {stores[s]}');cmd(f'set {v}.Target = {stores[t]}');cmd(f'set {v}.Region = {regions[r]}')
            cmd(f'call {v}.SetStatus("{planpkg}".NewStatus({codes[code]}, nil))')
            cmd(f'call {c}.Collect(nil)')
            bucket='schedulablePlans' if code=='StatusOK' else 'unschedulablePlans'
            snapshot=f'{c}.{bucket}[len({c}.{bucket})-1].(*plan.BalanceSchedulerPlan)'
            cmd(f'set {snapshot}.Step = {step}')
        cmd(f'call "{planpkg}".BalancePlanSummary({c}.GetPlans())')
    log.append('SCENARIO invalid_plan\n')
    cmd(f'set {c}.unschedulablePlans[0] = nil')
    cmd(f'call "{planpkg}".BalancePlanSummary({c}.GetPlans())')
finally:
    p.stdin.write('exit\n');p.stdin.flush();log.append(p.stdout.read());p.wait()
    (root/'go.out').write_text('\n'.join(line.rstrip() for line in ''.join(log).splitlines()).rstrip()+'\n')
