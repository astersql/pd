"""Read source text only; produce the scheduling admission inventory, never Go code."""
import hashlib, json, pathlib, re, subprocess

ROOT = pathlib.Path('/Users/Shared/work/dir/data/codes/pd')
PLAN = ROOT / '.plans/2026-10-25-调度执行内核'
OUT = pathlib.Path(__file__).parent
SOURCE = '836fdc7cf3ef34f7eabc9d051418995bb97efabe'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def mask(s):
    pattern = r'//[^\n]*|/\*[\s\S]*?\*/|`[^`]*`|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\''
    return re.sub(pattern, lambda m: ''.join('\n' if c=='\n' else ' ' for c in m[0]), s)
def line(s,pos): return s.count('\n',0,pos)+1
def balanced(m,start,left='(',right=')'):
    depth=0
    for i in range(start,len(m)):
        if m[i]==left: depth+=1
        elif m[i]==right:
            depth-=1
            if depth==0:return i+1
    raise ValueError(('unbalanced',start))
def imports(s):
    blocks=re.findall(r'^import\s*\(([\s\S]*?)^\)',s,re.M)
    blocks+=re.findall(r'^import\s+([^\n(][^\n]*)',s,re.M)
    return sorted(set(re.findall(r'"([^"\n]+)"','\n'.join(blocks))))

tasks={int(m[1]):str(p.relative_to(ROOT)) for p in PLAN.rglob('*.md') if (m:=re.match(r'(\d+)-',p.name))}
coverage={p:[int(n) for n in re.findall(r'\d+',ids)] for p,ids in re.findall(r'\| `([^`]+\.go)` \| ([^|]+) \|',(PLAN/'来源覆盖清单.md').read_text())}
packages=sorted({str(pathlib.PurePosixPath(p).parent) for p in coverage})
actual={str(p.relative_to(ROOT)) for d in packages for p in (ROOT/d).glob('*.go') if not p.name.endswith('_test.go')}
assert actual==set(coverage)

step_tasks={'TransferLeader':18,'AddPeer':19,'AddLearner':19,'PromoteLearner':19,'RemovePeer':20,'DemoteVoter':20,'BecomeWitness':21,'BecomeNonWitness':21,'BatchSwitchWitness':21,'PromoteNonWitness':21,'ChangePeerV2Enter':22,'ChangePeerV2Leave':22,'PromoteLearnerForJointConsensus':22,'DemoteVoterForJointConsensus':22,'MergeRegion':23,'SplitRegion':23}
def owner(path,name='',receiver=''):
    ids=coverage[path]; stem=pathlib.PurePosixPath(path).stem; text=(receiver+' '+name).lower()
    def choose(*values):return sorted({n for n in values if n in ids}) or [ids[0]]
    if len(ids)==1:return ids
    if path.endswith('core/cluster_informer.go'):
        if any(x in text for x in ['schedulercluster','checkercluster','schedulerconfig','checkerconfig','storeconfig','updateregionslabel']):return [4]
        if 'basiccluster'==name.lower() or receiver=='BasicCluster':return [2]
        return [3]
    if path.endswith('operator/step.go'):
        for typ,n in step_tasks.items():
            if receiver==typ or name==typ:return [n]
        return ids
    if stem=='builder':
        if 'witness' in text:return [30]
        if any(x in text for x in ['joint','v2','demote','promote']):return [29]
        if any(x in text for x in ['leader','light']):return [28]
        if any(x in text for x in ['check','validate','target','newbuilder','builder']):return choose(26,27)
        return [27]
    if stem=='create_operator':return [32] if any(x in text for x in ['split','merge','witness']) else [31]
    if path.endswith('operator/operator.go'):return [25] if any(x in text for x in ['cancel','replace','record','history','bury','additional','expired']) else [24]
    if stem=='operator_controller':
        if any(x in text for x in ['dispatch','schedulecommand','stale','push','hbstream']):return [36]
        if any(x in text for x in ['influence','limit','ack','cost']):return [35]
        if any(x in text for x in ['history','record','callback','buried','bury','exceed','cleanup']):return [37]
        return [34]
    if stem=='filters':
        if any(x in text for x in ['placement','safeguard','fit','rule','distinct']):return [9]
        if any(x in text for x in ['storage','engine','label','isolation','specialuse']):return [8]
        if any(x in text for x in ['state','health','busy','pending','snapshot','slow','witness','pause']):return [7]
        return [5]
    if path.endswith('handler/handler.go'):
        if any(x in text for x in ['scatterregions','scatterregion','scatterresponse']):return [125] if 'regions' in text else [121,125]
        if 'splitregions' in text:return [126]
        if any(x in text for x in ['merge','splitregion']):return [120]
        if any(x in text for x in ['handleoperatorcreation']):return [121]
        if any(x in text for x in ['addtransfer','addadd','addremove']):return [119]
        if any(x in text for x in ['scheduler','checker','diagnostic']):return [122]
        if any(x in text for x in ['hot','load','buckets']):return [123]
        if any(x in text for x in ['accelerate','limit','labeler','getregion','getstore']):return [124]
        return [118]
    if stem=='region_scatterer':
        if any(x in text for x in ['scatterregions','batch','retry','retrylater']):return [61]
        if any(x in text for x in ['state','delta','internal','scatterwith','operator']):return [60]
        return [59]
    if stem=='region_splitter':
        if any(x in text for x in ['retry','cancel','timeout','wait']):return [64]
        if any(x in text for x in ['splitregions','splitregion','scan','handler']):return [63]
        return [62]
    if stem=='coordinator':
        if any(x in text for x in ['init','load','restore']):return [114]
        if any(x in text for x in ['run','stop','close','patrol','check','collect']) and 'new' not in text:return [115]
        if any(x in text for x in ['hot','stat','metric']):return [116]
        return [113]
    if stem=='cluster' and path.startswith('pkg/cluster'):
        if any(x in text for x in ['overlap','clean','remove']):return [128]
        if any(x in text for x in ['observe','collect','statistics','statistic']):return [129]
        return [127]
    if path.startswith('pkg/progress/'):
        if stem=='indicator':return [131] if any(x in text for x in ['window','adjust']) else [130]
        return [132] if any(x in text for x in ['manager','gc','progressby','progresses','reset','finished','action','average','setpatrol']) else [130]
    if stem=='heartbeat_streams':
        if any(x in text for x in ['operation','newoperation','changepeer','transferleader','split','merge','witness']):return [12]
        return [14] if any(x in text for x in ['run','close','stop','drain','keepalive']) else [13]
    if path.startswith('pkg/schedule/checker/'):
        if stem=='checker_controller':
            if any(x in text for x in ['suspect','range']):return [57]
            if any(x in text for x in ['patrol','run','stop','close','duration']):return [56]
            if 'metric' in text:return [58]
            return [55]
        if stem=='replica_checker':return [41] if any(x in text for x in ['down','offline','health','check']) else [42]
        if stem=='rule_checker':
            if any(x in text for x in ['orphan','improve','replace','location']):return [45]
            return [44] if any(x in text for x in ['role','witness','leader','promote','demote']) else [43]
        if stem=='merge_checker':return [47] if any(x in text for x in ['target','split','cache','record','interval']) else [46]
        if stem=='affinity_checker':return [50] if 'merge' in text else [49]
        if stem=='split_scatter':return [54] if any(x in text for x in ['expire','clean','retry','cancel','stop','dispatch']) else [53]
    if path.startswith('pkg/schedule/schedulers/'):
        if stem=='scheduler_controller':
            if any(x in text for x in ['diagnostic','diagnose']):return [109]
            if any(x in text for x in ['metric','collect','reset']):return [110]
            if any(x in text for x in ['run','stop','wait','schedulecontroller','scheduleroutine']):return [108]
            return [107]
        if stem=='utils':
            if any(x in text for x in ['load','pending','rank','lpcmp','ldcmp','influence']):return [69]
            return [68] if any(x in text for x in ['range','retry','quota','threshold','role']) else [67]
        if stem=='scheduler':
            if any(x in text for x in ['register','createscheduler','decoder','slice','json']):return [106]
            return [66] if any(x in text for x in ['config','save','load','persist']) else [65]
        if stem=='base_scheduler':return [65]
        if stem=='evict_slow_store':
            if name=='ServeHTTP':return [141]
            if 'network' in text:
                return [81] if any(x in text for x in ['recover','activate','pause','addnetwork','deletenetwork']) else [80]
            return [79]
        if stem=='hot_region':return [95] if any(x in text for x in ['pending','influence','zombie']) else [94] if any(x in text for x in ['schedule','read','write']) else [86]
        if stem=='hot_region_solver':
            if any(x in text for x in ['bucket','split']):return [93]
            if any(x in text for x in ['pending','influence']):return [95]
            if any(x in text for x in ['target','dest']):return [90]
            if any(x in text for x in ['source','region','filter']):return [89]
            return [91] if any(x in text for x in ['range','placement','load']) else [92]
        if stem=='evict_slow_trend':return [153] if name=='ServeHTTP' else [84] if any(x in text for x in ['recover','evict','slowtrend']) else [83]
        if name=='ServeHTTP':return [n for n in ids if n>=137] or [ids[0]]
        if len(ids)>1 and any(x in text for x in ['config','persist','reload','clone','update','setdisable','prepareconfig','cleanconfig']):return [ids[1]] if ids[1]<137 else [ids[0]]
    return [ids[0]]

all_sources={}
for n in subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z'],cwd=ROOT).decode().split('\0'):
    if n and n.endswith('.go') and (ROOT/n).is_file():
        all_sources[n]=(ROOT/n).read_text()
reverse={p:[] for p in packages}
for path,s in all_sources.items():
    for imp in imports(s):
        q=imp.removeprefix('github.com/tikv/pd/')
        if q in reverse:reverse[q].append({'file':path,'package':str(pathlib.PurePosixPath(path).parent),'test':path.endswith('_test.go')})

records=[]
for path,ids in sorted(coverage.items()):
    s=all_sources[path]; m=mask(s); symbols=[]; funcs=[]
    for a in re.finditer(r'^func\s+(?:\(([^)]*)\)\s*)?([A-Za-z_]\w*)\s*(?:\[[^\]]*\])?\s*\(',m,re.M):
        recv=re.sub(r'\[[^]]*\]','',(a[1] or '').split()[-1] if a[1] else '').strip('*')
        end=balanced(m,a.end()-1)
        decl=s[a.start():end].strip().replace('\n',' ')
        item={'kind':'method' if recv else 'function','name':a[2],'receiver':recv,'line':line(s,a.start()),'signature':decl,'tasks':owner(path,a[2],recv),'exported':a[2][0].isupper()}
        funcs.append((a.start(),item));symbols.append(item)
    for a in re.finditer(r'^type\s+(\w+)\s*(?:\[[^\]]*\])?\s+([^\n]+)',m,re.M):
        item={'kind':'type','name':a[1],'line':line(s,a.start()),'tasks':owner(path,a[1]),'exported':a[1][0].isupper()}
        symbols.append(item)
        b=re.match(r'(?:struct|interface)\s*\{',a[2])
        if b:
            start=m.find('{',a.start());end=balanced(m,start,'{','}')
            pos=start+1;depth=1
            for ln in s[start+1:end-1].splitlines(keepends=True):
                masked=mask(ln);field=re.match(r'\s*([A-Z]\w*)\s*(\(|\s+)',masked)
                if field and depth==1:
                    symbols.append({'kind':'interface_method' if '(' in field[2] else 'embedded_interface' if not masked.strip().replace(field[1],'',1) else 'field','name':field[1],'receiver':a[1],'line':line(s,pos),'signature':ln.strip(),'tasks':owner(path,field[1],a[1]),'exported':True})
                depth+=masked.count('{')-masked.count('}');pos+=len(ln)
    for a in re.finditer(r'^(const|var)\s*(\(|[A-Za-z_]\w*)',m,re.M):
        if a[2]!='(':
            if a[2][0].isupper():symbols.append({'kind':a[1],'name':a[2],'line':line(s,a.start()),'tasks':owner(path,a[2]),'exported':True})
            continue
        start=m.find('(',a.start());end=balanced(m,start);depth=1;braces=0;pos=start+1
        for ln in m[start+1:end-1].splitlines(keepends=True):
            match=re.match(r'\s*([A-Z]\w*)\b',ln)
            if match and depth==1 and braces==0:symbols.append({'kind':a[1],'name':match[1],'line':line(s,pos),'tasks':owner(path,match[1]),'exported':True})
            depth+=ln.count('(')-ln.count(')');braces+=ln.count('{')-ln.count('}');pos+=len(ln)
    def context(pos):
        preceding=[v for start,v in funcs if start<=pos]
        return preceding[-1] if preceding else None
    def site(pos,kind,raw):
        ctx=context(pos)
        return {'line':line(s,pos),'kind':kind,'source':raw.strip(),'function':(ctx.get('receiver','')+'.'+ctx['name']).strip('.') if ctx else None,'tasks':ctx['tasks'] if ctx else owner(path)}
    failpoints=[]
    for a in re.finditer(r'failpoint\.(InjectCall|Inject|InjectContext)\s*\(\s*"([^"\n]+)"',s):
        failpoints.append({**site(a.start(),a[1],a[0]),'name':a[2],'qualified_name':'github.com/tikv/pd/'+str(pathlib.PurePosixPath(path).parent)+'/'+a[2]})
    metrics=[]
    for a in re.finditer(r'prometheus\.(New\w+)\s*\(',m):
        end=balanced(m,a.end()-1);raw=s[a.start():end]
        opts={n:re.search(r'\b'+n+r':\s*([^,\n]+)',raw)[1].strip() if re.search(r'\b'+n+r':\s*([^,\n]+)',raw) else None for n in ['Namespace','Subsystem','Name','Help']}
        labels=[]
        for group in re.findall(r'\[\]string\s*\{([^}]+)\}',raw):labels+=re.findall(r'"([^"\n]+)"',group)
        metrics.append({**site(a.start(),'metric_constructor',a[0]),'options':opts,'variable_labels':labels,'raw_constructor':raw})
    persistence=[]
    for a in re.finditer(r'\b(?:keypath\.\w+|\w+\.(?:SaveSchedulerConfig|LoadSchedulerConfig|LoadAllSchedulerConfigs|RemoveSchedulerConfig|SaveConfig|LoadConfig|Save|Load|Remove|RunInTxn|LoadRange))\b',m):
        raw=s[s.rfind('\n',0,a.start())+1:s.find('\n',a.start())]
        persistence.append(site(a.start(),'keypath_reference' if a[0].startswith('keypath.') else 'storage_call_candidate',raw))
    asynchronous=[]
    for a in re.finditer(r'\bgo\s+(?:func\b|[\w.]+\s*\()|\b(?:NewTicker|NewTimer|AfterFunc|GoWithRecover|NewConcurrentRunner)\s*\(',m):
        raw=s[s.rfind('\n',0,a.start())+1:s.find('\n',a.start())]
        asynchronous.append(site(a.start(),'goroutine' if a[0].startswith('go ') else 'timer_or_runner',raw))
    protocol=sorted(set(re.findall(r'\b(?:pdpb|metapb|eraftpb|http)\.\w+',m)))
    records.append({'path':path,'sha256':sha(ROOT/path),'file_tasks':ids,'direct_imports':imports(s),'public_symbols':[v for v in symbols if v['exported']],'internal_symbols':[v for v in symbols if not v['exported']],'failpoints':failpoints,'metrics':metrics,'persistence_sites':persistence,'asynchronous_sites':asynchronous,'protocol_references':protocol})

package_records=[]
for pkg in packages:
    tests=[]
    for p in sorted((ROOT/pkg).glob('*_test.go')):
        s=p.read_text();m=mask(s);roots={recv:root for root,recv in re.findall(r'func (Test\w+)\(t \*testing.T\)\s*\{\s*suite.Run\(t, new\((\w+)\)\)',s)}
        paired=str(p.relative_to(ROOT)).replace('_test.go','.go')
        if paired not in coverage:paired=next(x for x in coverage if str(pathlib.PurePosixPath(x).parent)==pkg)
        declarations=list(re.finditer(r'^func\s+(?:\(([^)]*)\)\s*)?((?:Test|Benchmark|Setup|TearDown)\w*)\s*\(',m,re.M))
        for i,a in enumerate(declarations):
            end=declarations[i+1].start() if i+1<len(declarations) else len(s)
            recv=(a[1] or '').split()[-1].strip('*') if a[1] else ''
            tests.append({'file':str(p.relative_to(ROOT)),'line':line(s,a.start()),'name':a[2],'receiver':recv,'run_name':roots.get(recv,'')+'/'+a[2] if recv else a[2],'tasks':owner(paired,a[2],recv),'failpoint_runtime_calls':re.findall(r'failpoint\.(EnableCall|Enable|Disable)\s*\(\s*([^\n]+)',s[a.start():end]),'classification':'existing_test_source_not_automatically_approved_to_run'})
    direct=sorted({imp for r in records if str(pathlib.PurePosixPath(r['path']).parent)==pkg for imp in r['direct_imports'] if imp.startswith('github.com/tikv/pd/')})
    package_records.append({'path':pkg,'direct_repository_dependencies':direct,'reverse_repository_dependencies':reverse[pkg],'tests':tests,'test_main_files':[t['file'] for t in tests if t['name']=='TestMain'],'native_test_count':sum(t['name'].startswith('Test') and t['name']!='TestMain' and not t['receiver'] for t in tests)})

init=(ROOT/'pkg/schedule/schedulers/init.go').read_text()
factory_sources=['balance_leader','balance_region','balance_witness','evict_leader','evict_slow_store','evict_stopping_store','grant_hot_region','hot_region','grant_leader','label','random_merge','scatter_range','shuffle_hot_region','shuffle_leader','shuffle_region','transfer_witness_leader','evict_slow_trend','balance_range']
factories=[]
for a,stem in zip(re.finditer(r'RegisterScheduler\(types\.(\w+)',init),factory_sources):
    path='pkg/schedule/schedulers/'+stem+'.go'
    factories.append({'type':a[1],'registration_file':'pkg/schedule/schedulers/init.go','registration_line':line(init,a.start()),'source_file':path,'source_sha256':sha(ROOT/path),'behavior_tasks':coverage[path],'registration_task':106,'runtime_evidence':'behavior_run_pending_in_owner_task; existing source/test route audited; no factory creation claimed as migration completion'})
assert len(factories)==18
baseline=json.loads((OUT/'before.json').read_text())
inventory={'schema_version':1,'source_commit':SOURCE,'current_head':baseline['head'],'scope':'14 source packages; all build-tag variants inventoried textually','semantics':'task mappings are implementation ownership, not claims of Rust parity or Go runtime coverage','tasks':{str(n):{'file':p,'title':pathlib.Path(p).stem.split('-',1)[1]} for n,p in tasks.items()},'readonly_start':baseline,'production_files':records,'packages':package_records,'scheduler_factories':factories,'exclusions':[{'behavior':'Go plugin ABI','task':117,'disposition':'unsupported boundary, no new plugin ABI'}]}
for r in records:
    for item in r['public_symbols']+r['internal_symbols']+r['failpoints']+r['metrics']+r['persistence_sites']+r['asynchronous_sites']:
        assert item['tasks'] and all(n in tasks for n in item['tasks']),(r['path'],item)
(OUT/'source-inventory.json').write_text(json.dumps(inventory,ensure_ascii=False,indent=2)+'\n')
summary={'production_files':len(records),'packages':len(packages),'factories':len(factories),'public_symbols':sum(len(r['public_symbols']) for r in records),'internal_symbols':sum(len(r['internal_symbols']) for r in records),'test_declarations':sum(len(p['tests']) for p in package_records),'failpoints':sum(len(r['failpoints']) for r in records),'metrics':sum(len(r['metrics']) for r in records),'persistence_candidates':sum(len(r['persistence_sites']) for r in records),'asynchronous_sites':sum(len(r['asynchronous_sites']) for r in records)}
(OUT/'inventory-summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary))
