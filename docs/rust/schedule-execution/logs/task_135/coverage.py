"""Require actual passing behavioral tests for each of the 18 concrete policies."""
import gzip,hashlib,json,pathlib
root=pathlib.Path.cwd();folder=root/'docs/rust/schedule-execution/logs/task_135'
mapping={
'balance-leader-scheduler':['070'], 'balance-region-scheduler':['072'],'balance-witness-scheduler':['073'],
'evict-leader-scheduler':['075'],'grant-leader-scheduler':['077'], 'evict-slow-store-scheduler':['079','081'],
'evict-stopping-store-scheduler':['082'],'evict-slow-trend-scheduler':['084'], 'balance-hot-region-scheduler':['094'],
'grant-hot-region-scheduler':['096'],'label-scheduler':['097'],'random-merge-scheduler':['098'], 'scatter-range-scheduler':['099'],
'shuffle-hot-region-scheduler':['100'],'shuffle-leader-scheduler':['101'],'shuffle-region-scheduler':['102'], 'transfer-witness-leader-scheduler':['103'],'balance-range-scheduler':['105']}
extra={'081':'real_operators_and_batches','082':'real_operators_and_batches','084':'real_operators','105':'roles_engines_and_faults'}
rows=[]
log=gzip.open(folder/'final-2.log.gz','rt').read()
for typ,ids in mapping.items():
 tests=[f'task_{i}_test::task_{i}_behavior_cases' for i in ids]
 tests += [f'task_{i}_test::task_{i}_{extra[i]}' for i in ids if i in extra]
 fixtures=[f'docs/rust/schedule-execution/fixtures/task_{i}.json' for i in ids]
 for test in tests:
  mod,name=test.split('::');p=root/'pkg/schedule/schedulers'/(mod+'.rs')
  assert 'fn '+name+'(' in p.read_text(),test
  assert f'test {test} ... ok' in log,('missing passing test',test)
 rows.append(dict(type=typ,tests=tests,fixtures={p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in fixtures}))
expected={r['type'] for r in json.loads((root/'docs/rust/schedule-execution/fixtures/task_106.json').read_text())['cases']}
assert set(mapping)==expected
(folder/'scheduler-coverage.json').write_text(json.dumps({'scope':'18 concrete policy behavioral suites; composition separately covers 4 scheduler policies plus checker/operator/range stories; no claim of 18 full-process cluster simulations','scheduler_count':18,'policies':rows},indent=2)+'\n')
print('18/18 concrete scheduler policies have actual passing behavior tests')
