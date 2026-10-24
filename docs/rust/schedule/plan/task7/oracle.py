import subprocess,re,pathlib
p=subprocess.Popen(['docker','exec','-i','pd-task7-oracle','dlv','exec','/oracle/pd-classic-linux','--allow-non-terminal-interactive','--','--version'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,bufsize=0)
log=[]
def read():
 s=''
 while not s.endswith('(dlv) '):
  c=p.stdout.read(1)
  if not c: break
  s+=c
 log.append(s);return s
def cmd(s):
 log.append('COMMAND '+s+'\n');p.stdin.write(s+'\n');p.stdin.flush();return read()
read();cmd('break main.main');cmd('continue');cmd('next')
s=cmd('call "github.com/tikv/pd/pkg/schedule/plan".NewBalanceSchedulerPlan()')
a=re.search(r'\)\((0x[0-9a-f]+)\)',s).group(1)
v='(*"github.com/tikv/pd/pkg/schedule/plan.BalanceSchedulerPlan")('+a+')'
cmd('call '+v+'.GetResource(0)')
s=cmd('call "github.com/tikv/pd/pkg/core".NewStoreInfo(nil, nil)')

store=re.search(r'\)\((0x[0-9a-f]+)\)',s).group(1)
cmd('set '+v+'.Source = (*"github.com/tikv/pd/pkg/core.StoreInfo")('+store+')')
cmd('set '+v+'.Target = '+v+'.Source')
for step in [-1,0,1,2,3]:
 cmd('set '+v+'.Step = '+str(step))
 cmd('call '+v+'.Clone(nil)')
cmd('call '+v+'.GetResource(0)')
cmd('call '+v+'.GetResource(2)')
cmd('call '+v+'.SetResourceWithStep(nil, 99)')
cmd('call '+v+'.GetResource(99)')
cmd('call '+v+'.GetResource(100)')
cmd('call '+v+'.SetResourceWithStep(nil, 0)')
s=cmd('call "github.com/tikv/pd/pkg/schedule/plan".NewCollector(' + v + '.Clone(nil))')
m=re.search(r'\)\((0x[0-9a-f]+)\)',s)
if m:
 c='(*"github.com/tikv/pd/pkg/schedule/plan.Collector")('+m.group(1)+')'
 cmd('call '+c+'.Collect(nil)')
 cmd('call '+c+'.GetPlans()')
 cmd('call '+c+'.basePlan.SetStatus("github.com/tikv/pd/pkg/schedule/plan".NewStatus(400, nil))')
 cmd('call '+c+'.Collect(nil)')
 cmd('call '+c+'.basePlan.SetStatus("github.com/tikv/pd/pkg/schedule/plan".NewStatus(0, nil))')
 cmd('call '+c+'.Collect(nil)')
 cmd('call '+c+'.GetPlans()')
 for i in range(3): cmd('call '+c+'.GetPlans()['+str(i)+'].GetStatus()')
 cmd('call (*"github.com/tikv/pd/pkg/schedule/plan.Collector")(0).Collect(nil)')
 cmd('call (*"github.com/tikv/pd/pkg/schedule/plan.Collector")(0).GetPlans()')
p.stdin.write('exit\n');p.stdin.flush();log.append(p.stdout.read());p.wait()
pathlib.Path('docs/rust/schedule/plan/task7/go.out').write_text(''.join(log))
