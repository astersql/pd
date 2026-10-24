from pathlib import Path
import re
p=Path(__file__).parent
for name, matched in [('fields-final.out', True), ('mismatch-final.out', False)]:
    s=(p/name).read_text()
    assert not re.search(r'/oracle/.*\.commands:\d+:',s), name
    assert 'region is stale: region id:2' in s
    assert 'map[uint64]*github.com/tikv/pd/pkg/core.regionItem [\n\t3:' in s
    assert '\n12\n' in s
    if matched:
        assert 'writtenBytes: 101,' in s and 'writtenBytes: 303,' in s
        assert 'Version: 7,' in s
        assert 'leader: *github.com/pingcap/kvproto/pkg/metapb.Peer nil' in s
    else:
        assert s.count('writtenBytes: 0,') == 2
        assert s.count('bucketMeta: *github.com/pingcap/kvproto/pkg/metapb.BucketMeta nil') == 2
        assert s.count('leader: *github.com/pingcap/kvproto/pkg/metapb.Peer nil') == 2
s=(p/'load-final.out').read_text()
assert 'interface {}(func()) github.com/tikv/pd/pkg/storage/endpoint.(*StorageEndpoint).LoadRegions.func1' in s
assert 'error="context canceled"' in s
assert '--- PASS: TestLoadRegion' in s
assert 'Process 22 has exited with status 0' in s
assert len(re.findall(r'/oracle/load.commands:\d+:',s)) == 1  # normal continue reports exit
assert '\nPASS\n' in s
print('2 real response snapshots + enabled slowLoadRegion closure/cancel test verified')
