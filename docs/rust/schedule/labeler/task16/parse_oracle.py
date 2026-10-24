"""Extract actual Delve return values; never synthesize expected Rust results."""
import hashlib, json, pathlib, re
root=pathlib.Path(__file__).parent
text=(root/'go.out').read_text()
assert 'Command failed:' not in text and '~panic:' not in text
blocks=re.split(r'\(dlv\) COMMAND ',text)[1:]
def boundary(mode,id):
    if id>=1<<24: mode+=1;id=0
    return bytes([mode,(id>>16)&255,(id>>8)&255,id&255,0,0,0,0,251])
inputs=[(42,b'rx','keyspaces/42'),(43,b'x','keyspaces/43'),(255,b'x','keyspaces/255'),(256,b'x','keyspaces/256'),(16777215,b'rx','keyspaces/16777215'),(1,b'x','keyspaces/01'),(42,b'x','probe')]
rules=[dict(id=name,index=0,labels=[dict(key='id',value='01' if name=='keyspaces/01' else str(id))],rule_type='key-range',data=[dict(start_key=boundary(m,id).hex(),end_key=boundary(m,id+1).hex()) for m in modes]) for id,modes,name in inputs]
queries=[];splits=[];booleans=[]
for block in blocks:
    command,_,output=block.partition('\n')
    if command=='call l.ruleIndex.getSplitKeys(nil, nil)':
        actual=[bytes(map(int,row.split(','))).hex() for row in re.findall(r'\[([0-9]+(?:,[0-9]+){8})\]',output)]
        assert len(actual)==14
        splits.append(actual)
    elif '.getRangeRules(' in command or '.matchRule(' in command:
        id,n=map(int,re.search(r'keyspaces/(\d+)"\].Data.*?\[(\d+)\]',command).groups())
        modes=dict((id,modes) for id,modes,name in inputs if name==f'keyspaces/{id}')
        mode=modes[id][n];start=boundary(mode,id)
        is_match='.matchRule(' in command
        end=start if is_match else boundary(mode,id+1)
        if is_match:start=start[:4]
        ids=sorted(re.findall(r'\bID: "([^"]+)"',output))
        within=re.search(r'~r'+('1' if is_match else '2')+r': (true|false)',output)
        assert within
        queries.append(dict(start=start.hex(),end=end.hex(),ids=ids,within=within[1]=='true',keyspace_only=is_match))
    elif re.search(r'keyspaces\.(Contains|Add|Replace|Remove)\(',command):
        value=re.search(r'~r0: (true|false)',output);assert value,command
        booleans.append(dict(command=command,value=value[1]=='true'))
assert len(queries)==14 and len(booleans)==9 and len(splits)==2
assert splits[0]==splits[1]
(root/'go.json').write_text(json.dumps(dict(binary_sha256=hashlib.sha256(pathlib.Path('/tmp/pd-types-oracle/pd-classic-linux').read_bytes()).hexdigest(),rules=rules,queries=queries,splits=splits[0],operations=booleans),indent=2)+'\n')
print('Go runtime fixture: 14 queries, 14 split keys, 9 ownership/shape operations')
